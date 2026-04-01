use crate::config;
use crate::core::memory::{Conversation, ConversationSummary, MemoryStore};
use crate::models::provider::*;
use crate::security::audit::{AuditLogger, AuditSource, AuditStatus};
use crate::security::credentials::CredentialStore;
use crate::security::risk::RiskEngine;
use crate::tools::file_ops::FileOps;
use crate::tools::registry::ToolRegistry;
use crate::tools::shell::ShellExecutor;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

static DESKTOP_APPROVALS: std::sync::LazyLock<
    Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>,
> = std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

static SEARCH_CONFIG: std::sync::LazyLock<Mutex<config::SearchConfig>> =
    std::sync::LazyLock::new(|| Mutex::new(config::SearchConfig::default()));

/// Tracks monthly API usage: { "2026-03": { "serpapi": 42, "brave": 17 } }
static SEARCH_USAGE: std::sync::LazyLock<Mutex<HashMap<String, HashMap<String, u32>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Serialize, Deserialize)]
pub struct PerfEvent {
    pub event_type: String,
    pub label: String,
    pub duration_ms: u64,
    pub timestamp: String,
}

static PERF_EVENTS: std::sync::LazyLock<Mutex<Vec<PerfEvent>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

static APP_DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Call once at startup with Tauri's app_data_dir to unify all data paths.
pub fn init_app_data_dir(dir: PathBuf) {
    tracing::info!("[App] Data directory: {}", dir.display());
    let _ = APP_DATA_DIR.set(dir);
}

/// Get the unified app data directory. Falls back to %APPDATA%/com.zelex.auto-crab on Windows.
pub fn app_data_dir() -> PathBuf {
    APP_DATA_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| {
            if let Some(appdata) = std::env::var_os("APPDATA") {
                PathBuf::from(appdata).join("com.zelex.auto-crab")
            } else {
                PathBuf::from(".")
            }
        })
}

fn perf_log_path() -> PathBuf {
    app_data_dir().join("perf-events.jsonl")
}

/// Load persisted perf events from disk (call once at startup).
pub fn load_perf_events() {
    let path = perf_log_path();
    tracing::info!("[Perf] Data file path: {}", path.display());
    if !path.exists() {
        tracing::info!("[Perf] No historical data file found, starting fresh");
        return;
    }
    let Ok(content) = std::fs::read_to_string(&path) else { return };
    let mut events: Vec<PerfEvent> = content.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    if events.len() > 500 {
        events.drain(..events.len() - 500);
    }
    if let Ok(mut guard) = PERF_EVENTS.lock() {
        *guard = events;
    }
    tracing::info!("[Perf] Loaded {} historical events from disk", content.lines().count());
}

pub fn record_perf_event(event_type: &str, label: &str, duration_ms: u64) {
    let event = PerfEvent {
        event_type: event_type.to_string(),
        label: label.to_string(),
        duration_ms,
        timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };
    if let Ok(mut guard) = PERF_EVENTS.lock() {
        guard.push(event.clone());
        if guard.len() > 500 {
            let drain_count = guard.len() - 500;
            guard.drain(..drain_count);
        }
    }
    // Append to disk (fire-and-forget, non-blocking)
    std::thread::spawn(move || {
        if let Ok(line) = serde_json::to_string(&event) {
            use std::io::Write;
            let path = perf_log_path();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                let _ = writeln!(f, "{}", line);
            }
        }
    });
}

pub fn update_search_config(cfg: &config::SearchConfig) {
    if let Ok(mut guard) = SEARCH_CONFIG.lock() {
        *guard = cfg.clone();
    }
}

fn current_month_key() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs();
    // Approximate year-month from unix timestamp
    let days = secs / 86400;
    let years = days / 365;
    let year = 1970 + years;
    let remaining_days = days - years * 365;
    let month = remaining_days / 30 + 1;
    format!("{}-{:02}", year, month.min(12))
}

fn record_search_usage(provider: &str) {
    let month_key = current_month_key();
    if let Ok(mut guard) = SEARCH_USAGE.lock() {
        let month_map = guard.entry(month_key).or_insert_with(HashMap::new);
        *month_map.entry(provider.to_string()).or_insert(0) += 1;
    }
}

fn get_search_usage(provider: &str) -> u32 {
    let month_key = current_month_key();
    SEARCH_USAGE.lock().ok()
        .and_then(|g| g.get(&month_key).and_then(|m| m.get(provider).copied()))
        .unwrap_or(0)
}

pub fn desktop_approvals() -> &'static Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>> {
    &DESKTOP_APPROVALS
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: String,
    pub operation: String,
    pub risk_level: String,
    pub description: String,
    pub details: serde_json::Value,
    pub created_at: String,
}

#[derive(Default)]
pub struct ApprovalState {
    pending: Mutex<HashMap<String, PendingApproval>>,
}

impl ApprovalState {
    pub fn create(&self, approval: PendingApproval) {
        let mut pending = self.pending.lock().expect("approval state poisoned");
        pending.insert(approval.id.clone(), approval);
    }

    pub fn resolve(&self, id: &str) -> Option<PendingApproval> {
        let mut pending = self.pending.lock().expect("approval state poisoned");
        pending.remove(id)
    }

    pub fn list(&self) -> Vec<PendingApproval> {
        let pending = self.pending.lock().expect("approval state poisoned");
        pending.values().cloned().collect()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResult<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T: Serialize> ApiResult<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(msg: impl ToString) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.to_string()),
        }
    }
}

#[tauri::command]
pub async fn get_config(app: AppHandle) -> ApiResult<config::AppConfig> {
    match config::load_config(&app).await {
        Ok(cfg) => ApiResult::ok(cfg),
        Err(e) => ApiResult::err(format!("Failed to load config: {}", e)),
    }
}

#[tauri::command]
pub async fn save_config(app: AppHandle, config_data: config::AppConfig) -> ApiResult<()> {
    // Update global search config when config is saved
    update_search_config(&config_data.search);
    match config::save_config(&app, &config_data).await {
        Ok(()) => ApiResult::ok(()),
        Err(e) => ApiResult::err(format!("Failed to save config: {}", e)),
    }
}

// ─── Skills file-based CRUD ───

#[tauri::command]
pub async fn list_skills(app: AppHandle) -> ApiResult<Vec<config::UserSkill>> {
    let dir = config::skills_dir(&app);
    let skills = config::load_skills_from_dir(&dir).await;
    ApiResult::ok(skills)
}

#[tauri::command]
pub async fn save_skill(app: AppHandle, skill: config::UserSkill) -> ApiResult<()> {
    match config::save_single_skill(&app, &skill).await {
        Ok(()) => ApiResult::ok(()),
        Err(e) => ApiResult::err(format!("Failed to save skill: {}", e)),
    }
}

#[tauri::command]
pub async fn delete_skill(app: AppHandle, name: String) -> ApiResult<()> {
    match config::delete_single_skill(&app, &name).await {
        Ok(()) => ApiResult::ok(()),
        Err(e) => ApiResult::err(format!("Failed to delete skill: {}", e)),
    }
}

#[tauri::command]
pub async fn rename_skill_cmd(app: AppHandle, old_name: String, new_name: String) -> ApiResult<()> {
    match config::rename_skill(&app, &old_name, &new_name).await {
        Ok(()) => ApiResult::ok(()),
        Err(e) => ApiResult::err(format!("Failed to rename skill: {}", e)),
    }
}

#[tauri::command]
pub async fn get_skills_dir(app: AppHandle) -> ApiResult<String> {
    let dir = config::skills_dir(&app);
    ApiResult::ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn get_search_usage_stats() -> ApiResult<serde_json::Value> {
    let cfg = SEARCH_CONFIG.lock().map(|g| g.clone()).unwrap_or_default();
    let serpapi_used = get_search_usage("serpapi");
    let brave_used = get_search_usage("brave");
    let tavily_used = get_search_usage("tavily");
    ApiResult::ok(serde_json::json!({
        "serpapi": {
            "used": serpapi_used,
            "quota": cfg.serpapi_monthly_quota,
            "remaining": cfg.serpapi_monthly_quota.saturating_sub(serpapi_used),
            "configured": !cfg.serpapi_api_key.is_empty(),
        },
        "brave": {
            "used": brave_used,
            "quota": cfg.brave_monthly_quota,
            "remaining": cfg.brave_monthly_quota.saturating_sub(brave_used),
            "configured": !cfg.brave_api_key.is_empty(),
        },
        "tavily": {
            "used": tavily_used,
            "quota": cfg.tavily_monthly_quota,
            "remaining": cfg.tavily_monthly_quota.saturating_sub(tavily_used),
            "configured": !cfg.tavily_api_key.is_empty(),
        }
    }))
}

/// Save content to the Obsidian knowledge vault from the frontend.
#[tauri::command]
pub async fn save_to_knowledge_base(
    app: AppHandle,
    title: String,
    content: String,
) -> ApiResult<String> {
    let cfg = match config::load_config(&app).await {
        Ok(c) => c,
        Err(e) => return ApiResult::err(format!("Config error: {}", e)),
    };
    if !cfg.knowledge.enabled || cfg.knowledge.vault_path.is_empty() {
        return ApiResult::err("知识库未启用或路径未配置");
    }
    crate::save_to_vault(&cfg.knowledge, &title, &content);
    ApiResult::ok("已保存到知识库".into())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatSendRequest {
    pub message: String,
    pub model_override: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatSendResponse {
    pub reply: String,
    pub model: String,
    pub usage: Option<TokenUsage>,
}

#[tauri::command]
pub async fn chat_send(app: AppHandle, request: ChatSendRequest) -> ApiResult<ChatSendResponse> {
    let cfg = match config::load_config(&app).await {
        Ok(c) => c,
        Err(e) => return ApiResult::err(format!("Config error: {}", e)),
    };

    let router = match crate::models::ModelRouter::from_config(&cfg) {
        Ok(r) => r,
        Err(e) => return ApiResult::err(format!("Model router error: {}", e)),
    };

    let chat_req = ChatRequest {
        messages: vec![
            ChatMessage {
                role: MessageRole::System,
                content: cfg.agent.system_prompt.clone(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: MessageRole::User,
                content: request.message,
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        tools: None,
        temperature: 0.7,
        max_tokens: None,
    };

    match router.chat_with_fallback(chat_req).await {
        Ok(resp) => ApiResult::ok(ChatSendResponse {
            reply: resp.message.content,
            model: resp.model,
            usage: resp.usage,
        }),
        Err(e) => ApiResult::err(format!("Chat error: {}", e)),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
}

/// Determine if the user message likely needs tool execution.
pub fn should_use_tools(message: &str) -> bool {
    let msg = message.to_lowercase();
    let len = msg.chars().count();

    if len <= 5 && !msg.contains("看") && !msg.contains("删") && !msg.contains("建") {
        return false;
    }

    let action_keywords = [
        "帮我", "帮忙", "请你", "创建", "新建", "删除", "打开", "运行", "执行",
        "文件", "目录", "文件夹", "桌面", "截图", "截屏", "屏幕", "看看",
        "列出", "查看", "读取", "写入", "保存", "修改", "编辑",
        "安装", "下载", "搜索", "抓取", "网页", "网站",
        "点击", "输入", "按键", "微信", "飞书",
        "监控", "盯盘", "分析",
        "cmd", "shell", "pip", "npm", "git",
        "c:\\", "c:/", "~/", "/users",
    ];

    action_keywords.iter().any(|kw| msg.contains(kw))
}

/// Build tool definitions from the tool registry.
pub fn build_tool_definitions() -> Vec<ToolDefinition> {
    ToolRegistry::new().to_tool_definitions()
}

/// Map tool name to the operation type used by the risk engine.
pub fn tool_operation_type(name: &str) -> &str {
    match name {
        "read_file" | "list_directory" | "fetch_webpage" | "read_pdf" => "read_file",
        "search_web" | "get_crypto_price" | "get_market_price" => "search_web",
        "screenshot" | "analyze_screen" | "get_ui_tree" => "read_file",
        "focus_window" => "write_file",
        "write_file" => "write_file",
        "execute_shell" | "analyze_and_act" | "quick_reply_wechat" => "execute_shell",
        "mouse_click" | "keyboard_type" | "key_press" => "execute_shell",
        "delete_file" => "delete_file",
        name if name.starts_with("mcp_") => "search_web",
        _ => "unknown",
    }
}

pub fn is_readonly_shell_command_pub(arguments: &str) -> bool { is_readonly_shell_command(arguments) }

/// Check if a shell command is read-only (safe to auto-approve).
fn is_readonly_shell_command(arguments: &str) -> bool {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let cmd = args["command"].as_str().unwrap_or("").trim().to_lowercase();

    if cmd.contains('>') || cmd.contains(">>") || cmd.contains('|') || cmd.contains("rm ")
        || cmd.contains("del ") || cmd.contains("move ") || cmd.contains("copy ")
        || cmd.contains("mkdir ") || cmd.contains("rmdir ")
    {
        return false;
    }

    let first = cmd.split_whitespace().next().unwrap_or("");
    matches!(first,
        "dir" | "ls" | "cat" | "type" | "where" | "which" | "whoami" |
        "echo" | "date" | "time" | "hostname" | "pwd" | "head" | "tail" |
        "find" | "wc" | "sort" | "grep" | "rg" | "tree" | "tasklist"
    )
}

/// Execute a single tool call with audit logging.
/// If an `AuditLogger` is provided, the operation will be recorded.
pub async fn dispatch_tool_with_audit(
    tc: &ToolCall,
    file_ops: &FileOps,
    shell: &ShellExecutor,
    audit: Option<&Arc<AuditLogger>>,
    source: AuditSource,
) -> String {
    let op = tool_operation_type(&tc.name);
    let risk_engine = RiskEngine::new(HashMap::new());
    let risk = risk_engine.assess(op);

    if risk == crate::config::RiskLevel::Forbidden {
        if let Some(a) = audit {
            let _ = a
                .log(op, risk.clone(), AuditStatus::Blocked, &tc.name, source)
                .await;
        }
        return format!("操作被禁止: {}", tc.name);
    }

    let tool_start = std::time::Instant::now();
    let result = dispatch_tool(tc, file_ops, shell).await;
    let tool_elapsed = tool_start.elapsed();
    record_perf_event("tool_call", &tc.name, tool_elapsed.as_millis() as u64);

    if let Some(a) = audit {
        let details: String = format!(
            "{}({})",
            tc.name,
            tc.arguments.chars().take(100).collect::<String>()
        );
        let _ = a
            .log(op, risk, AuditStatus::AutoApproved, &details, source)
            .await;
    }

    result
}

/// Execute a single tool call, returning its output as a string.
pub async fn dispatch_tool(tc: &ToolCall, file_ops: &FileOps, shell: &ShellExecutor) -> String {
    let args: serde_json::Value =
        serde_json::from_str(&tc.arguments).unwrap_or(serde_json::Value::Null);

    match tc.name.as_str() {
        "read_file" => {
            let path = args["path"].as_str().unwrap_or("");
            match file_ops.read_file(path).await {
                Ok(c) if c.len() > 12000 => {
                    format!("{}…\n[已截断，共 {} 字符]", &c[..12000], c.len())
                }
                Ok(c) => c,
                Err(e) => format!("read_file 失败: {}", e),
            }
        }
        "write_file" => {
            let path = args["path"].as_str().unwrap_or("");
            let content = args["content"].as_str().unwrap_or("");

            let snapshots = crate::core::snapshots::SnapshotStore::new(app_data_dir());
            let expanded = FileOps::expand_path(path);
            if expanded.exists() {
                if let Err(e) = snapshots
                    .take_snapshot(expanded.to_str().unwrap_or(path), "write_file")
                    .await
                {
                    tracing::warn!("Snapshot failed for {}: {}", path, e);
                }
            }

            match file_ops.write_file(path, content).await {
                Ok(()) => format!("文件已写入: {}（已自动快照，可用 /undo 撤回）", path),
                Err(e) => format!("write_file 失败: {}", e),
            }
        }
        "list_directory" => {
            let path = args["path"].as_str().unwrap_or(".");
            match file_ops.list_directory(path).await {
                Ok(entries) => entries
                    .iter()
                    .map(|e| {
                        if e.is_dir {
                            format!("📁 {}/", e.name)
                        } else {
                            format!("📄 {} ({} bytes)", e.name, e.size)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                Err(e) => format!("list_directory 失败: {}", e),
            }
        }
        "execute_shell" => {
            let command = args["command"].as_str().unwrap_or("");
            let working_dir = args["working_directory"].as_str();
            match shell.execute(command, working_dir).await {
                Ok(output) => {
                    let mut r = output.stdout.clone();
                    if !output.stderr.is_empty() {
                        if !r.is_empty() {
                            r.push('\n');
                        }
                        r.push_str("[stderr] ");
                        r.push_str(&output.stderr);
                    }
                    r.push_str(&format!("\n[exit: {}]", output.exit_code));
                    r
                }
                Err(e) => format!("execute_shell 失败: {}", e),
            }
        }
        "mouse_click" => {
            let x = args["x"].as_i64().unwrap_or(0) as i32;
            let y = args["y"].as_i64().unwrap_or(0) as i32;
            let click_type = args["click_type"].as_str().unwrap_or("left");
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            match do_mouse_click(x, y, click_type).await {
                Ok(msg) => {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    msg
                }
                Err(e) => format!("mouse_click 失败: {}", e),
            }
        }
        "keyboard_type" => {
            let text = args["text"].as_str().unwrap_or("");
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            match do_keyboard_type(text).await {
                Ok(msg) => {
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    msg
                }
                Err(e) => format!("keyboard_type 失败: {}", e),
            }
        }
        "key_press" => {
            let key = args["key"].as_str().unwrap_or("enter");
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            match do_key_press(key).await {
                Ok(msg) => {
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    msg
                }
                Err(e) => format!("key_press 失败: {}", e),
            }
        }
        "search_web" => {
            let query = args["query"].as_str().unwrap_or("");
            match search_web(query).await {
                Ok(results) => results,
                Err(e) => format!("search_web 失败: {}。可尝试 fetch_webpage 直接访问目标网站", e),
            }
        }
        "fetch_webpage" => {
            let url = args["url"].as_str().unwrap_or("");
            match fetch_webpage_text(url).await {
                Ok(text) => {
                    let preview: String = text.chars().take(8000).collect();
                    if text.len() > 8000 {
                        format!("{}...\n\n[网页内容已截断，共 {} 字符]", preview, text.len())
                    } else {
                        preview
                    }
                }
                Err(e) => format!("fetch_webpage 失败: {}", e),
            }
        }
        "screenshot" => {
            let data_dir = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_else(|_| ".".into());
            let default_path = format!(
                "{}\\Desktop\\auto-crab-screenshot-{}.png",
                data_dir,
                chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            );
            let output = args["output_path"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or(&default_path);
            match take_screenshot(output).await {
                Ok(path) => format!("截图已保存: {}", path),
                Err(e) => format!("截图失败: {}", e),
            }
        }
        "read_pdf" => {
            let path = args["path"].as_str().unwrap_or("");
            let max_pages = args["max_pages"].as_u64().unwrap_or(20) as usize;
            match read_pdf_text(path, max_pages).await {
                Ok(text) => {
                    let preview: String = text.chars().take(15000).collect();
                    if text.len() > 15000 {
                        format!("{}...\n\n[PDF内容已截断，共 {} 字符。如需完整内容，请减少页数或分批读取]", preview, text.len())
                    } else {
                        preview
                    }
                }
                Err(e) => format!("PDF读取失败: {}", e),
            }
        }
        "get_crypto_price" => {
            let symbol = args["symbol"].as_str().unwrap_or("BTCUSDT").to_uppercase();
            match fetch_crypto_price(&symbol).await {
                Ok(info) => info,
                Err(e) => format!("获取价格失败: {}", e),
            }
        }
        "get_market_price" => {
            let query = args["query"].as_str().unwrap_or("BTCUSDT");
            match fetch_market_price(query).await {
                Ok(info) => info,
                Err(e) => format!("获取行情失败: {}", e),
            }
        }
        "quick_reply_wechat" => {
            let contact = args["contact"].as_str().unwrap_or("");
            let message = args["message"].as_str().unwrap_or("");
            format!("__WECHAT_REPLY__:{}::{}", contact, message)
        }
        "get_ui_tree" => {
            let max_depth = args["max_depth"].as_u64().unwrap_or(8) as u32;
            match tokio::task::spawn_blocking(move || {
                crate::tools::ui_automation::get_foreground_ui_tree(max_depth)
            }).await {
                Ok(Ok(snapshot)) => {
                    if snapshot.has_useful_elements() {
                        snapshot.serialize_text()
                    } else {
                        format!("窗口 '{}' 的控件树信息不足（可能是 Canvas/自绘UI），建议使用 analyze_screen 替代", snapshot.window_title)
                    }
                }
                Ok(Err(e)) => format!("get_ui_tree 失败: {}", e),
                Err(e) => format!("get_ui_tree 执行错误: {}", e),
            }
        }
        "focus_window" => {
            let title = args["title"].as_str().unwrap_or("");
            let t = title.to_string();
            match tokio::task::spawn_blocking(move || {
                crate::tools::ui_automation::focus_window_by_title(&t)
            }).await {
                Ok(Ok(msg)) => msg,
                Ok(Err(e)) => format!("focus_window 失败: {}", e),
                Err(e) => format!("focus_window 执行错误: {}", e),
            }
        }
        "analyze_and_act" => {
            let task = args["task"].as_str().unwrap_or("");
            let max_steps = args["max_steps"].as_u64().unwrap_or(3) as usize;
            format!("__ANALYZE_AND_ACT__:{}::{}", max_steps, task)
        }
        "analyze_screen" => {
            let data_dir = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_else(|_| ".".into());
            let tmp_path = format!("{}\\AppData\\Local\\Temp\\auto-crab-screen.png", data_dir);
            let question = args["question"].as_str().unwrap_or("请详细描述截图中的内容");
            match take_screenshot(&tmp_path).await {
                Ok(path) => {
                    let screen_w = 2560;
                    let screen_h = 1440;
                    let prompt = format!(
                        "{}\n\n屏幕实际分辨率: {}x{}。截图已按比例缩小，请根据原始分辨率换算坐标。\
如果你看到需要点击的UI元素，请输出其在 {}x{} 屏幕上的坐标，格式: CLICK_TARGET: (x, y) 元素名称。\
不要编造坐标，如果看不清具体位置就说明无法确定。",
                        question, screen_w, screen_h, screen_w, screen_h
                    );
                    format!("__ANALYZE_SCREEN__:{}::{}", path, prompt)
                }
                Err(e) => format!("截图失败: {}", e),
            }
        }
        _ => format!("未知工具: {}", tc.name),
    }
}

/// Compress an image to JPEG ≤ 180KB for VL model input.
fn compress_image_for_vl(image_path: &str) -> anyhow::Result<Vec<u8>> {
    use image::GenericImageView;

    let img = image::open(image_path)?;
    let (w, h) = img.dimensions();

    let max_dim = 1280u32;
    let img = if w > max_dim || h > max_dim {
        let ratio = max_dim as f64 / w.max(h) as f64;
        let nw = (w as f64 * ratio) as u32;
        let nh = (h as f64 * ratio) as u32;
        img.resize(nw, nh, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let mut buf = std::io::Cursor::new(Vec::new());
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 60);
    img.write_with_encoder(encoder)?;
    let data = buf.into_inner();
    tracing::info!("Compressed screenshot: {}x{} → {} bytes", w, h, data.len());
    Ok(data)
}

/// Analyze a screenshot image using the VL (vision-language) model.
/// Uses a direct HTTP call to DashScope API (bypassing provider proxy settings).
pub async fn analyze_screenshot(
    cfg: &config::AppConfig,
    image_path: &str,
) -> anyhow::Result<String> {
    analyze_screenshot_with_prompt(cfg, image_path, "请详细描述这张截图中的内容。如果是聊天软件，请列出可见的消息内容和发送者。如果是网页，请总结页面内容。如果是代码编辑器，请描述代码内容。").await
}

pub async fn analyze_screenshot_with_prompt(
    cfg: &config::AppConfig,
    image_path: &str,
    prompt: &str,
) -> anyhow::Result<String> {
    let vl_entry = cfg
        .models
        .vision
        .as_ref()
        .or_else(|| {
            cfg.models
                .coding
                .as_ref()
                .filter(|e| e.provider == "dashscope_vl")
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "未配置视觉模型。请在 [models.vision] 中设置 provider = \"dashscope_vl\""
            )
        })?;

    let api_key = crate::security::credentials::CredentialStore::resolve_ref(
        vl_entry.api_key_ref.as_deref().unwrap_or(""),
    )?;

    let path_clone = image_path.to_string();
    let jpeg_data =
        tokio::task::spawn_blocking(move || compress_image_for_vl(&path_clone)).await??;

    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &jpeg_data);
    tracing::info!("VL input base64 length: {} chars", b64.len());

    let body = serde_json::json!({
        "model": vl_entry.model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "image_url", "image_url": {"url": format!("data:image/jpeg;base64,{}", b64)}},
                {"type": "text", "text": prompt}
            ]
        }],
        "max_tokens": 2000,
        "temperature": 0.3
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    let mut last_err = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            tracing::info!("VL API retry attempt {}", attempt + 1);
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        match client
            .post("https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let status = resp.status();
                    let err_body = resp.text().await.unwrap_or_default();
                    last_err = format!("VL API error {}: {}", status, err_body);
                    continue;
                }
                let api_resp: serde_json::Value = resp.json().await?;
                let content = api_resp["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap_or("视觉模型未返回内容")
                    .to_string();
                return Ok(content);
            }
            Err(e) => {
                last_err = format!("网络错误: {}", e);
                continue;
            }
        }
    }
    anyhow::bail!("VL 分析失败（重试3次）: {}", last_err)
}

pub async fn do_mouse_click_pub(x: i32, y: i32, ct: &str) -> anyhow::Result<String> { do_mouse_click(x, y, ct).await }
pub async fn do_keyboard_type_pub(text: &str) -> anyhow::Result<String> { do_keyboard_type(text).await }
pub async fn do_key_press_pub(key: &str) -> anyhow::Result<String> { do_key_press(key).await }

async fn do_mouse_click(x: i32, y: i32, click_type: &str) -> anyhow::Result<String> {
    let ct = click_type.to_string();
    tokio::task::spawn_blocking(move || {
        use enigo::{Button, Coordinate::Abs, Direction::Click, Enigo, Mouse, Settings};
        let mut enigo =
            Enigo::new(&Settings::default()).map_err(|e| anyhow::anyhow!("enigo init: {}", e))?;
        enigo
            .move_mouse(x, y, Abs)
            .map_err(|e| anyhow::anyhow!("mouse_move: {}", e))?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        match ct.as_str() {
            "right" => enigo
                .button(Button::Right, Click)
                .map_err(|e| anyhow::anyhow!("click: {}", e))?,
            "double" => {
                enigo
                    .button(Button::Left, Click)
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                std::thread::sleep(std::time::Duration::from_millis(50));
                enigo
                    .button(Button::Left, Click)
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
            }
            _ => enigo
                .button(Button::Left, Click)
                .map_err(|e| anyhow::anyhow!("click: {}", e))?,
        }
        Ok(format!("已点击 ({}, {})，类型: {}", x, y, ct))
    })
    .await?
}

async fn do_keyboard_type(text: &str) -> anyhow::Result<String> {
    let t = text.to_string();
    tokio::task::spawn_blocking(move || {
        use enigo::{Enigo, Keyboard, Settings};
        let mut enigo =
            Enigo::new(&Settings::default()).map_err(|e| anyhow::anyhow!("enigo init: {}", e))?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        enigo.text(&t).map_err(|e| anyhow::anyhow!("text: {}", e))?;
        let preview: String = t.chars().take(50).collect();
        Ok(format!("已输入文字: {}", preview))
    })
    .await?
}

async fn do_key_press(key_str: &str) -> anyhow::Result<String> {
    let k = key_str.to_lowercase();
    tokio::task::spawn_blocking(move || {
        use enigo::{
            Direction::{Click, Press, Release},
            Enigo, Key, Keyboard, Settings,
        };
        let mut enigo =
            Enigo::new(&Settings::default()).map_err(|e| anyhow::anyhow!("enigo init: {}", e))?;
        std::thread::sleep(std::time::Duration::from_millis(50));

        let map_key = |name: &str| -> Option<Key> {
            match name {
                "enter" | "return" => Some(Key::Return),
                "tab" => Some(Key::Tab),
                "escape" | "esc" => Some(Key::Escape),
                "backspace" => Some(Key::Backspace),
                "delete" | "del" => Some(Key::Delete),
                "space" => Some(Key::Space),
                "up" => Some(Key::UpArrow),
                "down" => Some(Key::DownArrow),
                "left" => Some(Key::LeftArrow),
                "right" => Some(Key::RightArrow),
                "home" => Some(Key::Home),
                "end" => Some(Key::End),
                "pageup" => Some(Key::PageUp),
                "pagedown" => Some(Key::PageDown),
                "ctrl" | "control" => Some(Key::Control),
                "alt" => Some(Key::Alt),
                "shift" => Some(Key::Shift),
                "win" | "meta" | "super" => Some(Key::Meta),
                s if s.len() == 1 => Some(Key::Unicode(s.chars().next().unwrap())),
                _ => None,
            }
        };

        if k.contains('+') {
            let parts: Vec<&str> = k.split('+').map(|s| s.trim()).collect();
            for &p in &parts[..parts.len() - 1] {
                if let Some(key) = map_key(p) {
                    enigo
                        .key(key, Press)
                        .map_err(|e| anyhow::anyhow!("{}", e))?;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
            if let Some(main) = parts.last().and_then(|p| map_key(p)) {
                enigo
                    .key(main, Click)
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
            for &p in parts[..parts.len() - 1].iter().rev() {
                if let Some(key) = map_key(p) {
                    enigo
                        .key(key, Release)
                        .map_err(|e| anyhow::anyhow!("{}", e))?;
                }
            }
        } else if let Some(key) = map_key(&k) {
            enigo
                .key(key, Click)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
        } else {
            return Ok(format!("未知按键: {}", k));
        }
        Ok(format!("已按下: {}", k))
    })
    .await?
}

pub async fn search_web_pub(query: &str) -> anyhow::Result<String> { search_web(query).await }
pub async fn fetch_webpage_pub(url: &str) -> anyhow::Result<String> { fetch_webpage_text(url).await }

async fn search_web(query: &str) -> anyhow::Result<String> {
    let cfg = SEARCH_CONFIG.lock().map(|g| g.clone()).unwrap_or_default();

    // If API keys are configured, use them (fast, reliable, structured JSON)
    let provider = cfg.provider.to_lowercase();

    // Priority: Tavily (free 1000/mo) → SerpApi (free 250/mo) → Brave (free 1000/mo, overage billed)

    // 1. Tavily — free 1000 credits/mo, AI-optimized results
    if (provider == "tavily" || provider == "auto") && !cfg.tavily_api_key.is_empty() {
        let used = get_search_usage("tavily");
        if used < cfg.tavily_monthly_quota {
            match search_tavily(query, &cfg.tavily_api_key).await {
                Ok(results) if !results.is_empty() => {
                    return Ok(format!("搜索 \"{}\" 的结果:\n\n{}", query, results));
                }
                Err(e) => tracing::warn!("Tavily search failed: {}", e),
                _ => tracing::warn!("Tavily returned no results"),
            }
        } else {
            tracing::warn!("Tavily quota exhausted ({}/{}), skipping", used, cfg.tavily_monthly_quota);
        }
    }

    // 2. SerpApi (Google) — free 250/mo
    if (provider == "serpapi" || provider == "auto") && !cfg.serpapi_api_key.is_empty() {
        let used = get_search_usage("serpapi");
        if used < cfg.serpapi_monthly_quota {
            match search_serpapi(query, &cfg.serpapi_api_key).await {
                Ok(results) if !results.is_empty() => {
                    return Ok(format!("搜索 \"{}\" 的结果:\n\n{}", query, results));
                }
                Err(e) => tracing::warn!("SerpApi search failed: {}", e),
                _ => tracing::warn!("SerpApi returned no results"),
            }
        } else {
            tracing::warn!("SerpApi quota exhausted ({}/{}), skipping", used, cfg.serpapi_monthly_quota);
        }
    }

    // 3. Brave Search — free 1000/mo but overage incurs charges
    if (provider == "brave" || provider == "auto") && !cfg.brave_api_key.is_empty() {
        let used = get_search_usage("brave");
        if used < cfg.brave_monthly_quota {
            match search_brave(query, &cfg.brave_api_key).await {
                Ok(results) if !results.is_empty() => {
                    return Ok(format!("搜索 \"{}\" 的结果:\n\n{}", query, results));
                }
                Err(e) => tracing::warn!("Brave search failed: {}", e),
                _ => tracing::warn!("Brave returned no results"),
            }
        } else {
            tracing::warn!("Brave quota exhausted ({}/{}), skipping", used, cfg.brave_monthly_quota);
        }
    }

    // Fallback: free scraping-based engines (no API key needed)
    match search_duckduckgo(query).await {
        Ok(results) if !results.is_empty() => {
            return Ok(format!("搜索 \"{}\" 的结果:\n\n{}", query, results));
        }
        Err(e) => tracing::warn!("DuckDuckGo search failed: {}, trying SearXNG", e),
        _ => tracing::warn!("DuckDuckGo returned no results, trying SearXNG"),
    }

    match search_searxng(query).await {
        Ok(results) if !results.is_empty() => {
            return Ok(format!("搜索 \"{}\" 的结果:\n\n{}", query, results));
        }
        Err(e) => tracing::warn!("SearXNG search failed: {}", e),
        _ => tracing::warn!("SearXNG returned no results"),
    }

    anyhow::bail!("搜索失败：所有搜索引擎均不可用，可尝试 fetch_webpage 直接访问目标网站")
}

/// SerpApi — Google Search scraping API, free 250/month
async fn search_serpapi(query: &str, api_key: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let url = format!(
        "https://serpapi.com/search?engine=google&q={}&api_key={}&num=8&output=json",
        urlencoding::encode(query),
        api_key,
    );

    let resp = client.get(&url)
        .send().await?;

    if !resp.status().is_success() {
        anyhow::bail!("SerpApi error: {}", resp.status());
    }

    record_search_usage("serpapi");
    let data: serde_json::Value = resp.json().await?;
    let mut results = Vec::new();

    if let Some(answer_box) = data["answer_box"]["answer"].as_str() {
        if !answer_box.is_empty() {
            results.push(format!("直接答案: {}", answer_box));
        }
    } else if let Some(snippet) = data["answer_box"]["snippet"].as_str() {
        if !snippet.is_empty() {
            results.push(format!("精选摘要: {}", snippet));
        }
    }

    if let Some(arr) = data["organic_results"].as_array() {
        for item in arr.iter().take(8) {
            let title = item["title"].as_str().unwrap_or("").trim();
            let link = item["link"].as_str().unwrap_or("");
            let snippet = item["snippet"].as_str().unwrap_or("").trim();
            if !title.is_empty() && !link.is_empty() {
                results.push(format!("{}. {}\n   {}\n   {}",
                    results.len() + 1, title,
                    snippet.chars().take(200).collect::<String>(), link));
            }
        }
    }

    if results.is_empty() {
        anyhow::bail!("SerpApi 未返回结果")
    }
    Ok(results.join("\n\n"))
}

/// Brave Search API — generous free tier (2000/month), global results
async fn search_brave(query: &str, api_key: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let url = format!("https://api.search.brave.com/res/v1/web/search?q={}&count=8",
        urlencoding::encode(query));

    let resp = client.get(&url)
        .header("Accept", "application/json")
        .header("Accept-Encoding", "gzip")
        .header("X-Subscription-Token", api_key)
        .send().await?;

    if !resp.status().is_success() {
        anyhow::bail!("Brave API error: {}", resp.status());
    }

    record_search_usage("brave");
    let data: serde_json::Value = resp.json().await?;
    let mut results = Vec::new();

    if let Some(arr) = data["web"]["results"].as_array() {
        for item in arr.iter().take(8) {
            let title = item["title"].as_str().unwrap_or("").trim();
            let url = item["url"].as_str().unwrap_or("");
            let desc = item["description"].as_str().unwrap_or("").trim();
            if !title.is_empty() && !url.is_empty() {
                results.push(format!("{}. {}\n   {}\n   {}",
                    results.len() + 1, title,
                    desc.chars().take(200).collect::<String>(), url));
            }
        }
    }

    if results.is_empty() {
        anyhow::bail!("Brave 未返回结果")
    }
    Ok(results.join("\n\n"))
}

/// Tavily Search API — AI-agent-optimized search, free 1000 credits/month
async fn search_tavily(query: &str, api_key: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let body = serde_json::json!({
        "query": query,
        "search_depth": "basic",
        "max_results": 8,
        "include_answer": true,
    });

    let resp = client.post("https://api.tavily.com/search")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send().await?;

    if !resp.status().is_success() {
        anyhow::bail!("Tavily API error: {}", resp.status());
    }

    record_search_usage("tavily");
    let data: serde_json::Value = resp.json().await?;
    let mut results = Vec::new();

    if let Some(answer) = data["answer"].as_str() {
        if !answer.is_empty() {
            results.push(format!("AI 摘要: {}", answer));
        }
    }

    if let Some(arr) = data["results"].as_array() {
        for item in arr.iter().take(8) {
            let title = item["title"].as_str().unwrap_or("").trim();
            let url = item["url"].as_str().unwrap_or("");
            let content = item["content"].as_str().unwrap_or("").trim();
            if !title.is_empty() && !url.is_empty() {
                results.push(format!("{}. {}\n   {}\n   {}",
                    results.len() + 1, title,
                    content.chars().take(200).collect::<String>(), url));
            }
        }
    }

    if results.is_empty() {
        anyhow::bail!("Tavily 未返回结果")
    }
    Ok(results.join("\n\n"))
}

/// SearXNG — open-source meta-search engine with JSON API (aggregates Google/Bing/etc.)
async fn search_searxng(query: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;

    // Try multiple public SearXNG instances
    let instances = [
        "https://search.sapti.me",
        "https://searx.be",
        "https://search.bus-hit.me",
        "https://searxng.site",
    ];

    for instance in &instances {
        let url = format!("{}/search?q={}&format=json&categories=general&language=en",
            instance, urlencoding::encode(query));
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(text) = resp.text().await {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(arr) = json["results"].as_array() {
                            let mut results = Vec::new();
                            for item in arr.iter().take(8) {
                                let title = item["title"].as_str().unwrap_or("").trim();
                                let url = item["url"].as_str().unwrap_or("");
                                let content = item["content"].as_str().unwrap_or("").trim();
                                if !title.is_empty() && !url.is_empty() {
                                    results.push(format!("{}. {}\n   {}\n   {}",
                                        results.len() + 1, title,
                                        content.chars().take(200).collect::<String>(), url));
                                }
                            }
                            if !results.is_empty() {
                                return Ok(results.join("\n\n"));
                            }
                        }
                    }
                }
            }
            Ok(_) => continue,
            Err(_) => continue,
        }
    }

    anyhow::bail!("SearXNG 所有实例均不可用")
}

async fn search_bing(query: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .build()?;

    // Use international Bing for global results, with ensearch=1
    let url = format!("https://www.bing.com/search?q={}&ensearch=1", urlencoding::encode(query));
    let resp = client.get(&url).send().await?;
    let html = resp.text().await?;

    let mut results = Vec::new();
    let mut pos = 0;

    for _ in 0..8 {
        // Bing results: <li class="b_algo"><h2><a href="...">title</a></h2><p>snippet</p></li>
        let algo_marker = if let Some(idx) = html[pos..].find("class=\"b_algo\"") {
            pos + idx
        } else {
            break;
        };

        let block_end = html[algo_marker..].find("class=\"b_algo\"")
            .map(|i| if i == 0 {
                html[algo_marker + 14..].find("class=\"b_algo\"").map(|j| algo_marker + 14 + j).unwrap_or(html.len())
            } else { algo_marker + i })
            .unwrap_or(html.len().min(algo_marker + 3000));
        let block = &html[algo_marker..block_end];

        let link_url = extract_attr(block, "<a", "href");
        let title = extract_tag_content(block, "<a");
        let snippet = extract_tag_content(block, "<p")
            .or_else(|| extract_class_content(block, "b_caption"));

        if let (Some(ref u), Some(ref t)) = (&link_url, &title) {
            if !u.starts_with("/") && !t.is_empty() {
                let clean_title = strip_html_tags(t);
                let clean_snippet = snippet.map(|s| strip_html_tags(&s)).unwrap_or_default();
                results.push(format!("{}. {}\n   {}\n   {}",
                    results.len() + 1, clean_title.trim(), clean_snippet.trim(), u));
            }
        }
        pos = block_end;
    }

    if results.is_empty() {
        anyhow::bail!("Bing 未返回结果")
    }
    Ok(results.join("\n\n"))
}

fn extract_attr(html: &str, tag: &str, attr: &str) -> Option<String> {
    let tag_start = html.find(tag)?;
    let attr_needle = format!("{}=\"", attr);
    let attr_start = html[tag_start..].find(&attr_needle)?;
    let val_start = tag_start + attr_start + attr_needle.len();
    let val_end = html[val_start..].find('"')?;
    let val = html[val_start..val_start + val_end].replace("&amp;", "&");
    Some(val)
}

fn extract_tag_content(html: &str, tag: &str) -> Option<String> {
    let tag_start = html.find(tag)?;
    let content_start = html[tag_start..].find('>')? + tag_start + 1;
    let close_tag = format!("</{}", &tag[1..]);
    let content_end = html[content_start..].find(&close_tag)
        .map(|i| content_start + i)
        .unwrap_or_else(|| html[content_start..].find("</").map(|i| content_start + i).unwrap_or(content_start + 200));
    Some(html[content_start..content_end].to_string())
}

fn extract_class_content(html: &str, class: &str) -> Option<String> {
    let marker = format!("class=\"{}\"", class);
    let start = html.find(&marker)?;
    let content_start = html[start..].find('>')? + start + 1;
    let content_end = html[content_start..].find("</div")
        .or_else(|| html[content_start..].find("</p"))
        .map(|i| content_start + i)
        .unwrap_or(content_start + 500);
    Some(html[content_start..content_end].to_string())
}

async fn search_duckduckgo(query: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()?;

    let url = format!("https://html.duckduckgo.com/html/?q={}", urlencoding::encode(query));
    let resp = client.get(&url).send().await?;
    let html = resp.text().await?;

    let mut results = Vec::new();
    let mut pos = 0;

    for _ in 0..12 {
        if let Some(start) = html[pos..].find("class=\"result__a\"") {
            let abs = pos + start;
            if let Some(href_start) = html[abs..].find("href=\"") {
                let href_abs = abs + href_start + 6;
                if let Some(href_end) = html[href_abs..].find('"') {
                    let raw_url = html[href_abs..href_abs + href_end].replace("&amp;", "&");

                    // DuckDuckGo wraps results in redirect URLs like //duckduckgo.com/l/?uddg=https%3A...
                    let final_url = if raw_url.contains("uddg=") {
                        if let Some(uddg_start) = raw_url.find("uddg=") {
                            let encoded = &raw_url[uddg_start + 5..];
                            let end = encoded.find('&').unwrap_or(encoded.len());
                            urlencoding::decode(&encoded[..end])
                                .map(|s| s.to_string())
                                .unwrap_or(encoded[..end].to_string())
                        } else { raw_url.clone() }
                    } else if raw_url.starts_with("http") {
                        raw_url.clone()
                    } else {
                        pos = href_abs + href_end + 1;
                        continue;
                    };

                    // Skip ad redirect URLs
                    if final_url.contains("duckduckgo.com/y.js") {
                        pos = href_abs + href_end + 1;
                        continue;
                    }

                    let title_start = html[href_abs + href_end..].find('>').map(|i| href_abs + href_end + i + 1).unwrap_or(0);
                    let title_end = html[title_start..].find('<').map(|i| title_start + i).unwrap_or(title_start);
                    let title = strip_html_tags(&html[title_start..title_end]).trim().to_string();

                    let snippet = if let Some(snip_start) = html[title_end..].find("class=\"result__snippet\"") {
                        let sa = title_end + snip_start;
                        if let Some(gt) = html[sa..].find('>') {
                            let content_start = sa + gt + 1;
                            let content_end = html[content_start..].find("</").map(|i| content_start + i).unwrap_or(content_start + 200);
                            strip_html_tags(&html[content_start..content_end]).trim().to_string()
                        } else { String::new() }
                    } else { String::new() };

                    if !title.is_empty() && !final_url.is_empty() {
                        results.push(format!("{}. {}\n   {}\n   {}", results.len() + 1, title, snippet, final_url));
                    }
                    pos = title_end;
                    if results.len() >= 8 { break; }
                } else { break; }
            } else { break; }
        } else { break; }
    }

    if results.is_empty() {
        anyhow::bail!("DuckDuckGo 未返回结果")
    }
    Ok(results.join("\n\n"))
}

async fn fetch_webpage_text(url: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Auto-Crab/1.0")
        .build()?;
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }
    let html = resp.text().await?;
    Ok(strip_html_tags(&html))
}

fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len() / 3);
    let mut in_tag = false;
    let mut in_script = false;
    let mut last_was_space = false;

    for c in html.chars() {
        if c == '<' {
            in_tag = true;
            continue;
        }
        if in_tag {
            if c == '>' {
                in_tag = false;
            }
            continue;
        }
        if in_script { continue; }

        let c = if c == '\n' || c == '\r' || c == '\t' { ' ' } else { c };
        if c == ' ' && last_was_space { continue; }
        last_was_space = c == ' ';
        result.push(c);
    }

    let text = result.trim().to_string();
    let script_re = regex::Regex::new(r"(?si)<script[^>]*>.*?</script>").unwrap();
    let style_re = regex::Regex::new(r"(?si)<style[^>]*>.*?</style>").unwrap();
    let cleaned = script_re.replace_all(html, "");
    let cleaned = style_re.replace_all(&cleaned, "");

    let mut final_text = String::with_capacity(cleaned.len() / 3);
    let mut tag = false;
    let mut ws = false;
    for c in cleaned.chars() {
        if c == '<' { tag = true; continue; }
        if tag { if c == '>' { tag = false; } continue; }
        let c = if c.is_whitespace() { ' ' } else { c };
        if c == ' ' && ws { continue; }
        ws = c == ' ';
        final_text.push(c);
    }
    let _ = text;
    final_text.trim().to_string()
}

/// Execute a vision-driven action loop: screenshot → VL analyze → execute actions → repeat
pub async fn execute_analyze_and_act(
    cfg: &config::AppConfig,
    task: &str,
    max_steps: usize,
) -> String {
    let tmp_dir = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into());
    let mut results = Vec::new();

    for step in 0..max_steps {
        let tmp_path = format!("{}\\AppData\\Local\\Temp\\auto-crab-act-{}.png", tmp_dir, step);

        let screenshot_ok = match take_screenshot(&tmp_path).await {
            Ok(_) => true,
            Err(e) => { results.push(format!("步骤{}: 截图失败: {}", step + 1, e)); false }
        };
        if !screenshot_ok { break; }

        let vl_prompt = format!(
r#"你是桌面操作助理。分析截图，在目标应用窗口中执行操作。
用户任务：{}
屏幕分辨率：2560x1440（截图已缩小，请按原始分辨率换算坐标）

重要规则：
- 只检查目标应用窗口的实际状态，忽略聊天窗口/终端中的文字
- 如果目标应用窗口中看不到任务要求的结果（如输入框为空），则任务未完成
- task_status=completed 仅当你在目标应用中确认操作已生效时才返回

输出 JSON（只输出JSON）：
{{"screen_description":"目标应用窗口状态","task_status":"need_action 或 completed 或 impossible","actions":[{{"type":"click 或 type 或 key_press","x":数字,"y":数字,"text":"字符串","key":"字符串","reason":"原因"}}]}}

click需要x,y; type需要text; key_press需要key。任务真正完成时actions返回空数组。"#,
            task
        );

        match analyze_screenshot_with_prompt(cfg, &tmp_path, &vl_prompt).await {
            Ok(vl_response) => {
                let parsed = parse_vl_actions(&vl_response);
                match parsed {
                    VlActionResult::Completed(desc) => {
                        results.push(format!("任务完成: {}", desc));
                        break;
                    }
                    VlActionResult::Impossible(desc) => {
                        results.push(format!("无法完成: {}", desc));
                        break;
                    }
                    VlActionResult::Actions(desc, actions) => {
                        results.push(format!("步骤{}: {} ({}个操作)", step + 1, desc, actions.len()));
                        for action in &actions {
                            match action.action_type.as_str() {
                                "click" => {
                                    if let Err(e) = do_mouse_click(action.x, action.y, "left").await {
                                        results.push(format!("  点击({},{})失败: {}", action.x, action.y, e));
                                    } else {
                                        results.push(format!("  已点击({},{}): {}", action.x, action.y, action.reason));
                                    }
                                }
                                "type" => {
                                    if let Err(e) = do_keyboard_type(&action.text).await {
                                        results.push(format!("  输入失败: {}", e));
                                    } else {
                                        let preview: String = action.text.chars().take(20).collect();
                                        results.push(format!("  已输入: {}", preview));
                                    }
                                }
                                "key_press" => {
                                    if let Err(e) = do_key_press(&action.key).await {
                                        results.push(format!("  按键失败: {}", e));
                                    } else {
                                        results.push(format!("  已按键: {}", action.key));
                                    }
                                }
                                _ => results.push(format!("  未知操作: {}", action.action_type)),
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                    }
                    VlActionResult::ParseError(raw) => {
                        results.push(format!("步骤{}: VL返回无法解析，原始内容: {}", step + 1, raw.chars().take(200).collect::<String>()));
                        break;
                    }
                }
            }
            Err(e) => {
                results.push(format!("步骤{}: VL分析失败: {}", step + 1, e));
                break;
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    }

    results.join("\n")
}

#[derive(Debug)]
struct VlAction {
    action_type: String,
    x: i32,
    y: i32,
    text: String,
    key: String,
    reason: String,
}

enum VlActionResult {
    Completed(String),
    Impossible(String),
    Actions(String, Vec<VlAction>),
    ParseError(String),
}

fn parse_vl_actions(response: &str) -> VlActionResult {
    let json_str = response.trim();
    let json_str = if let Some(start) = json_str.find('{') {
        if let Some(end) = json_str.rfind('}') {
            &json_str[start..=end]
        } else { json_str }
    } else { json_str };

    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return VlActionResult::ParseError(response.to_string()),
    };

    let desc = parsed["screen_description"].as_str().unwrap_or("").to_string();
    let status = parsed["task_status"].as_str().unwrap_or("need_action");

    match status {
        "completed" => VlActionResult::Completed(desc),
        "impossible" => VlActionResult::Impossible(desc),
        _ => {
            let actions_arr = parsed["actions"].as_array();
            let actions: Vec<VlAction> = actions_arr.map(|arr| {
                arr.iter().map(|a| VlAction {
                    action_type: a["type"].as_str().unwrap_or("").to_string(),
                    x: a["x"].as_i64().unwrap_or(0) as i32,
                    y: a["y"].as_i64().unwrap_or(0) as i32,
                    text: a["text"].as_str().unwrap_or("").to_string(),
                    key: a["key"].as_str().unwrap_or("").to_string(),
                    reason: a["reason"].as_str().unwrap_or("").to_string(),
                }).collect()
            }).unwrap_or_default();

            if actions.is_empty() {
                VlActionResult::Completed(desc)
            } else {
                VlActionResult::Actions(desc, actions)
            }
        }
    }
}

async fn read_pdf_text(path: &str, _max_pages: usize) -> anyhow::Result<String> {
    let expanded = crate::tools::file_ops::FileOps::expand_path(path);
    if !expanded.exists() {
        anyhow::bail!("文件不存在: {}", expanded.display());
    }

    let path_clone = expanded.clone();
    let text = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let bytes = std::fs::read(&path_clone)?;
        let result = std::panic::catch_unwind(|| {
            pdf_extract::extract_text_from_mem(&bytes)
        });
        match result {
            Ok(Ok(t)) => Ok(t),
            Ok(Err(e)) => anyhow::bail!("PDF 解析失败: {}。建议用 execute_shell 打开 PDF 后用 analyze_screen 截图分析", e),
            Err(_) => anyhow::bail!("PDF 编码不兼容。建议用 execute_shell 打开 PDF 后用 analyze_screen 截图查看内容"),
        }
    }).await??;

    if text.trim().is_empty() {
        anyhow::bail!("PDF 未提取到文本内容。可能是扫描件（纯图片），建议用 analyze_screen 截图分析")
    }

    Ok(text)
}

pub async fn fetch_crypto_price_pub(symbol: &str) -> anyhow::Result<String> { fetch_crypto_price(symbol).await }

/// Unified market data entry point. Detects market type from the query and fetches accordingly.
pub async fn fetch_market_price_pub(query: &str) -> anyhow::Result<String> {
    fetch_market_price(query).await
}

async fn fetch_market_price(query: &str) -> anyhow::Result<String> {
    let mut asset = parse_asset_query(query);

    // Dynamic search: resolve unknown symbols via Tencent smartbox search API
    if asset.symbol.starts_with("__SEARCH__") {
        let keyword = &asset.symbol[10..];
        match search_stock_symbol(keyword).await {
            Ok(resolved) => {
                asset = resolved;
            }
            Err(_) => {
                // Last resort: try as crypto
                let sym = format!("{}USDT", keyword.to_uppercase());
                asset = AssetQuery { market: MarketType::Crypto, symbol: sym, display_name: keyword.to_string() };
            }
        }
    }

    match asset.market {
        MarketType::Crypto => fetch_crypto_price(&asset.symbol).await,
        MarketType::CNStock => fetch_cn_stock(&asset.symbol, &asset.display_name).await,
        MarketType::HKStock => fetch_hk_stock(&asset.symbol, &asset.display_name).await,
        MarketType::USStock => fetch_us_stock(&asset.symbol, &asset.display_name).await,
        MarketType::JPStock => fetch_jp_stock(&asset.symbol, &asset.display_name).await,
        MarketType::Commodity => fetch_commodity(&asset.symbol, &asset.display_name).await,
        MarketType::Forex => fetch_forex(&asset.symbol, &asset.display_name).await,
    }
}

/// Search for a stock symbol by name/keyword using Tencent smartbox API.
/// Returns the best matching AssetQuery.
async fn search_stock_symbol(keyword: &str) -> anyhow::Result<AssetQuery> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()?;

    let encoded = urlencoding::encode(keyword);
    let url = format!(
        "https://smartbox.gtimg.cn/s3/?v=2&q={}&t=all&c=1",
        encoded
    );

    let resp = client
        .get(&url)
        .header("Referer", "https://finance.qq.com")
        .send()
        .await?;

    let text = resp.text().await?;
    // Response format: v_hint="code~name~market~...^code2~name2~..."
    let start = text.find('"').unwrap_or(0) + 1;
    let end = text.rfind('"').unwrap_or(text.len());
    if start >= end {
        anyhow::bail!("搜索无结果: {}", keyword);
    }

    let content = &text[start..end];
    // Parse the first result
    let first = content.split('^').next().unwrap_or("");
    let fields: Vec<&str> = first.split('~').collect();

    if fields.len() < 3 {
        anyhow::bail!("未找到匹配的标的: {}", keyword);
    }

    let code = fields[0];
    let name = fields[1];
    let market_hint = if fields.len() > 2 { fields[2] } else { "" };

    // Determine market from the code prefix / market hint
    let (market, symbol) = if code.starts_with("sh") || code.starts_with("sz") {
        (MarketType::CNStock, code.to_string())
    } else if code.starts_with("hk") {
        (MarketType::HKStock, code.to_string())
    } else if code.starts_with("us") {
        (MarketType::USStock, code.to_string())
    } else if market_hint.contains("us") || market_hint.contains("US") {
        (MarketType::USStock, format!("us{}", code))
    } else if market_hint.contains("hk") || market_hint.contains("HK") {
        (MarketType::HKStock, format!("hk{}", code))
    } else if code.starts_with('6') || code.starts_with("00") || code.starts_with("30") || code.starts_with("68") {
        // Likely A-share
        let prefix = if code.starts_with('6') || code.starts_with("68") { "sh" } else { "sz" };
        (MarketType::CNStock, format!("{}{}", prefix, code))
    } else {
        // Default to US stock
        (MarketType::USStock, format!("us{}", code.to_uppercase()))
    };

    Ok(AssetQuery {
        market,
        symbol,
        display_name: name.to_string(),
    })
}

#[derive(Debug, Clone)]
enum MarketType { Crypto, CNStock, HKStock, USStock, JPStock, Commodity, Forex }

#[derive(Debug, Clone)]
struct AssetQuery {
    market: MarketType,
    symbol: String,
    display_name: String,
}

fn parse_asset_query(query: &str) -> AssetQuery {
    let q = query.trim();
    let lower = q.to_lowercase();

    // Commodity detection — use multiple API identifiers: "sina_symbol|tencent_symbol"
    let commodity_map: Vec<(&str, &str, &str)> = vec![
        ("黄金", "hf_GC|AUTD", "黄金"),
        ("gold", "hf_GC|AUTD", "Gold"),
        ("xauusd", "hf_GC|AUTD", "黄金"),
        ("xau", "hf_GC|AUTD", "黄金"),
        ("白银", "hf_SI|AGTD", "白银"),
        ("silver", "hf_SI|AGTD", "Silver"),
        ("xagusd", "hf_SI|AGTD", "白银"),
        ("xag", "hf_SI|AGTD", "白银"),
        ("原油", "hf_CL|usOIL", "原油(WTI)"),
        ("crude", "hf_CL|usOIL", "Crude Oil(WTI)"),
        ("wti", "hf_CL|usOIL", "原油(WTI)"),
        ("brent", "hf_OIL|ukOIL", "布伦特原油"),
        ("天然气", "hf_NG|usNG", "天然气"),
        ("铜", "hf_HG|usCU", "铜(COMEX)"),
        ("copper", "hf_HG|usCU", "Copper(COMEX)"),
    ];
    for (kw, sym, name) in &commodity_map {
        if lower.contains(kw) {
            return AssetQuery { market: MarketType::Commodity, symbol: sym.to_string(), display_name: name.to_string() };
        }
    }

    // Forex detection — also with fallback symbols
    let forex_map: Vec<(&str, &str, &str)> = vec![
        ("美元指数", "dxy|UDI", "美元指数(DXY)"),
        ("usdcny", "fx_susdcny|USDCNY", "美元/人民币"),
        ("美元人民币", "fx_susdcny|USDCNY", "美元/人民币"),
        ("离岸人民币", "fx_susdcnh|USDCNH", "美元/离岸人民币"),
        ("eurusd", "fx_seurusd|EURUSD", "欧元/美元"),
        ("usdjpy", "fx_susdjpy|USDJPY", "美元/日元"),
        ("gbpusd", "fx_sgbpusd|GBPUSD", "英镑/美元"),
    ];
    for (kw, sym, name) in &forex_map {
        if lower.contains(kw) {
            return AssetQuery { market: MarketType::Forex, symbol: sym.to_string(), display_name: name.to_string() };
        }
    }

    // A-share detection: sh/sz prefix, 6-digit code, or Chinese stock names
    let cn_stocks: Vec<(&str, &str, &str)> = vec![
        ("茅台", "sh600519", "贵州茅台"),
        ("平安", "sh601318", "中国平安"),
        ("招行", "sh600036", "招商银行"),
        ("宁德", "sz300750", "宁德时代"),
        ("比亚迪a", "sz002594", "比亚迪"),
        ("中信", "sh600030", "中信证券"),
        ("万科", "sz000002", "万科A"),
        ("五粮液", "sz000858", "五粮液"),
        ("上证指数", "sh000001", "上证指数"),
        ("沪深300", "sh000300", "沪深300"),
        ("创业板", "sz399006", "创业板指"),
        ("深证成指", "sz399001", "深证成指"),
    ];
    for (kw, sym, name) in &cn_stocks {
        if lower.contains(kw) {
            return AssetQuery { market: MarketType::CNStock, symbol: sym.to_string(), display_name: name.to_string() };
        }
    }
    // Generic A-share code pattern: sh600xxx, sz000xxx, sz300xxx, sh688xxx, etc.
    if let Some(code) = extract_cn_stock_code(&lower) {
        let name = code.to_uppercase();
        return AssetQuery { market: MarketType::CNStock, symbol: code, display_name: name };
    }

    // HK stock detection
    let hk_stocks: Vec<(&str, &str, &str)> = vec![
        ("腾讯", "hk00700", "腾讯控股"),
        ("阿里", "hk09988", "阿里巴巴-W"),
        ("美团", "hk03690", "美团-W"),
        ("小米", "hk01810", "小米集团-W"),
        ("恒生指数", "hkHSI", "恒生指数"),
        ("恒指", "hkHSI", "恒生指数"),
        ("港股", "hkHSI", "恒生指数"),
        ("京东港", "hk09618", "京东集团-SW"),
        ("百度港", "hk09888", "百度集团-SW"),
        ("网易港", "hk09999", "网易-S"),
    ];
    for (kw, sym, name) in &hk_stocks {
        if lower.contains(kw) {
            return AssetQuery { market: MarketType::HKStock, symbol: sym.to_string(), display_name: name.to_string() };
        }
    }
    if let Some(code) = extract_hk_stock_code(&lower) {
        let name = code.to_uppercase();
        return AssetQuery { market: MarketType::HKStock, symbol: code, display_name: name };
    }

    // JP stock detection
    let jp_stocks: Vec<(&str, &str, &str)> = vec![
        ("日经", "b_N225", "日经225"),
        ("nikkei", "b_N225", "日经225指数"),
        ("丰田", "b_7203", "丰田汽车"),
        ("索尼", "b_6758", "索尼"),
        ("任天堂", "b_7974", "任天堂"),
        ("日股", "b_N225", "日经225"),
    ];
    for (kw, sym, name) in &jp_stocks {
        if lower.contains(kw) {
            return AssetQuery { market: MarketType::JPStock, symbol: sym.to_string(), display_name: name.to_string() };
        }
    }

    // US stock detection
    let us_stocks: Vec<(&str, &str, &str)> = vec![
        ("苹果", "usAAPL", "Apple(AAPL)"),
        ("特斯拉", "usTSLA", "Tesla(TSLA)"),
        ("英伟达", "usNVDA", "NVIDIA(NVDA)"),
        ("微软", "usMSFT", "Microsoft(MSFT)"),
        ("谷歌", "usGOOGL", "Alphabet(GOOGL)"),
        ("亚马逊", "usAMZN", "Amazon(AMZN)"),
        ("meta", "usMETA", "Meta(META)"),
        ("标普", "b_GSPC", "标普500"),
        ("纳斯达克", "b_IXIC", "纳斯达克综合"),
        ("纳指", "b_IXIC", "纳斯达克综合"),
        ("道琼斯", "b_DJI", "道琼斯工业"),
        ("道指", "b_DJI", "道琼斯工业"),
        ("美股", "b_GSPC", "标普500"),
    ];
    for (kw, sym, name) in &us_stocks {
        if lower.contains(kw) {
            return AssetQuery { market: MarketType::USStock, symbol: sym.to_string(), display_name: name.to_string() };
        }
    }
    // Generic US stock ticker: "AAPL", "TSLA", etc. - uppercase 1-5 letter codes
    if let Some(ticker) = extract_us_ticker(q) {
        let sym = format!("us{}", ticker);
        return AssetQuery { market: MarketType::USStock, symbol: sym, display_name: ticker.to_string() };
    }

    // Default: try as crypto if it looks like one, otherwise mark for dynamic search
    if lower.contains("usdt") || lower.contains("btc") || lower.contains("eth") {
        let sym = if lower.contains("usdt") { q.to_uppercase() } else { format!("{}USDT", q.to_uppercase()) };
        return AssetQuery { market: MarketType::Crypto, symbol: sym, display_name: q.to_string() };
    }

    // Fallback: dynamic search — symbol will be resolved at fetch time
    AssetQuery { market: MarketType::CNStock, symbol: format!("__SEARCH__{}", q), display_name: q.to_string() }
}

fn extract_cn_stock_code(text: &str) -> Option<String> {
    use regex::Regex;
    // Match "sh600519", "sz300750", "SH.600519", "600519.SH"
    if let Ok(re) = Regex::new(r"(?i)(sh|sz)\s*\.?\s*(\d{6})") {
        if let Some(cap) = re.captures(text) {
            return Some(format!("{}{}", &cap[1].to_lowercase(), &cap[2]));
        }
    }
    if let Ok(re) = Regex::new(r"(\d{6})\s*\.?\s*(?i)(sh|sz)") {
        if let Some(cap) = re.captures(text) {
            return Some(format!("{}{}", &cap[2].to_lowercase(), &cap[1]));
        }
    }
    // Bare 6-digit code starting with 6(SH), 0/3(SZ)
    if let Ok(re) = Regex::new(r"\b(6\d{5})\b") {
        if let Some(cap) = re.captures(text) {
            return Some(format!("sh{}", &cap[1]));
        }
    }
    if let Ok(re) = Regex::new(r"\b([03]\d{5})\b") {
        if let Some(cap) = re.captures(text) {
            return Some(format!("sz{}", &cap[1]));
        }
    }
    None
}

fn extract_hk_stock_code(text: &str) -> Option<String> {
    use regex::Regex;
    // "hk00700", "HK.00700", "00700.HK"
    if let Ok(re) = Regex::new(r"(?i)hk\s*\.?\s*(\d{5})") {
        if let Some(cap) = re.captures(text) {
            return Some(format!("hk{}", &cap[1]));
        }
    }
    if let Ok(re) = Regex::new(r"(\d{5})\s*\.?\s*(?i)hk") {
        if let Some(cap) = re.captures(text) {
            return Some(format!("hk{}", &cap[1]));
        }
    }
    None
}

fn extract_us_ticker(text: &str) -> Option<String> {
    use regex::Regex;
    // Match standalone uppercase 1-5 letter tickers like "AAPL", "TSLA"
    // Exclude common English words
    let exclude = ["THE", "AND", "FOR", "NOT", "YOU", "ALL", "CAN", "HER", "WAS", "ONE", "OUR", "OUT", "ARE", "HAS", "HIS", "HOW", "ITS", "LET", "MAY", "NEW", "NOW", "OLD", "SEE", "WAY", "WHO", "DID", "GET", "GOT", "HAD", "SAY", "SHE", "TOO", "USE", "MIN", "MAX", "BTC", "ETH", "SOL", "BNB", "XRP"];
    if let Ok(re) = Regex::new(r"\b([A-Z]{1,5})\b") {
        for cap in re.captures_iter(text) {
            let ticker = &cap[1];
            if !exclude.contains(&ticker) && ticker.len() >= 2 {
                return Some(ticker.to_string());
            }
        }
    }
    None
}

// ── Fetch functions for each market ──

async fn fetch_crypto_price(symbol: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .no_proxy()
        .build()?;

    let url = format!("https://api.binance.com/api/v3/ticker/24hr?symbol={}", symbol);
    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        anyhow::bail!("Binance API error: {}", resp.status());
    }

    let data: serde_json::Value = resp.json().await?;

    let price = data["lastPrice"].as_str().unwrap_or("N/A");
    let change = data["priceChangePercent"].as_str().unwrap_or("0");
    let high = data["highPrice"].as_str().unwrap_or("N/A");
    let low = data["lowPrice"].as_str().unwrap_or("N/A");
    let volume = data["volume"].as_str().unwrap_or("N/A");
    let quote_vol = data["quoteVolume"].as_str().unwrap_or("N/A");

    let change_f: f64 = change.parse().unwrap_or(0.0);
    let emoji = if change_f > 0.0 { "📈" } else if change_f < 0.0 { "📉" } else { "➡️" };

    Ok(format!(
        "{} {} 实时行情\n\
当前价格: ${}\n\
24h涨跌: {}% {}\n\
24h最高: ${}\n\
24h最低: ${}\n\
24h成交量: {} {}\n\
24h成交额: ${}\n\
数据来源: Binance API",
        emoji,
        symbol,
        price,
        change, emoji,
        high,
        low,
        volume, &symbol[..symbol.len().saturating_sub(4)],
        format_volume(quote_vol),
    ))
}

/// A-shares via Tencent Stock API (qt.gtimg.cn), accessible from mainland China
async fn fetch_cn_stock(symbol: &str, display: &str) -> anyhow::Result<String> {
    let data = fetch_tencent_stock(symbol).await?;
    format_tencent_stock(&data, display, "A股", "腾讯股票API")
}

/// HK stocks via Tencent Stock API
async fn fetch_hk_stock(symbol: &str, display: &str) -> anyhow::Result<String> {
    let data = fetch_tencent_stock(symbol).await?;
    format_tencent_stock(&data, display, "港股", "腾讯股票API")
}

/// US stocks via Tencent Stock API, with Yahoo Finance fallback for indices
async fn fetch_us_stock(symbol: &str, display: &str) -> anyhow::Result<String> {
    if symbol.starts_with("b_") {
        // Try Sina first
        if let Ok(result) = fetch_sina_index(symbol, display).await {
            return Ok(result);
        }
        let name = display;
        tracing::warn!("[Market] Sina failed for {}, trying Yahoo Finance", name);
        // Fallback: Yahoo Finance (more reliable for international indices)
        let yahoo_sym = match &symbol[2..] {
            "GSPC" => "^GSPC",
            "DJI" => "^DJI",
            "IXIC" => "^IXIC",
            other => other,
        };
        return fetch_yahoo_index(yahoo_sym, display).await;
    }
    let data = fetch_tencent_stock(symbol).await?;
    format_tencent_stock(&data, display, "美股", "腾讯股票API")
}

/// JP stocks via Sina Finance (international indices/stocks)
async fn fetch_jp_stock(symbol: &str, display: &str) -> anyhow::Result<String> {
    if let Ok(result) = fetch_sina_index(symbol, display).await {
        return Ok(result);
    }
    // Fallback: Yahoo Finance
    let yahoo_sym = match symbol {
        "b_N225" => "^N225",
        _ => return Err(anyhow::anyhow!("{} 获取失败：所有数据源均不可用", display)),
    };
    fetch_yahoo_index(yahoo_sym, display).await
}

/// Commodity: Tencent SGE (stable) → Sina futures → error
async fn fetch_commodity(symbol: &str, display: &str) -> anyhow::Result<String> {
    let parts: Vec<&str> = symbol.split('|').collect();
    let sina_sym = parts.first().unwrap_or(&symbol);
    let tencent_sym = parts.get(1).copied();

    // Primary: Tencent / Shanghai Gold Exchange (more stable)
    if let Some(tc) = tencent_sym {
        if let Ok(result) = fetch_commodity_tencent(tc, display).await {
            return Ok(result);
        }
    }

    // Fallback: Sina futures
    if let Ok(result) = fetch_sina_futures(sina_sym, display).await {
        return Ok(result);
    }

    anyhow::bail!("{} 获取失败：所有数据源均不可用", display)
}

/// Forex: frankfurter (stable, global) → Sina → error
async fn fetch_forex(symbol: &str, display: &str) -> anyhow::Result<String> {
    let parts: Vec<&str> = symbol.split('|').collect();
    let sina_sym = parts.first().unwrap_or(&symbol);
    let fallback_sym = parts.get(1).copied();

    // Primary: frankfurter.app (free, no key, stable)
    if let Some(fb) = fallback_sym {
        if let Ok(result) = fetch_forex_fallback(fb, display).await {
            return Ok(result);
        }
    }

    // Fallback: Sina
    if let Ok(result) = fetch_sina_futures(sina_sym, display).await {
        return Ok(result);
    }

    anyhow::bail!("{} 获取失败：所有数据源均不可用", display)
}

/// Primary commodity source: Shanghai Gold Exchange via Tencent (stable)
async fn fetch_commodity_tencent(symbol: &str, display: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build()?;

    let tencent_map: std::collections::HashMap<&str, &str> = [
        ("AUTD", "nf_AU9999"),
        ("AGTD", "nf_AG9999"),
    ].into();

    if let Some(tc_sym) = tencent_map.get(symbol) {
        let url = format!("https://qt.gtimg.cn/q={}", tc_sym);
        let resp = client.get(&url).header("Referer", "https://finance.qq.com").send().await?;
        let text = resp.text().await?;
        let start = text.find('"').unwrap_or(0) + 1;
        let end = text.rfind('"').unwrap_or(text.len());
        if end > start {
            let fields: Vec<&str> = text[start..end].split('~').collect();
            if fields.len() > 33 {
                let name = fields.get(1).unwrap_or(&display);
                let price = fields.get(3).unwrap_or(&"--");
                let change = fields.get(31).unwrap_or(&"--");
                let change_pct = fields.get(32).unwrap_or(&"--");
                return Ok(format!("{} | 价格: {} 元/克 | 涨跌: {} ({}%) | 来源: 上海黄金交易所",
                    name, price, change, change_pct));
            }
        }
    }

    anyhow::bail!("{} Tencent fallback 无数据", symbol)
}

/// Fallback for forex: frankfurter.app (free, no API key)
async fn fetch_forex_fallback(symbol: &str, display: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build()?;

    let forex_pairs: std::collections::HashMap<&str, (&str, &str)> = [
        ("USDCNY", ("USD", "CNY")), ("USDCNH", ("USD", "CNY")),
        ("EURUSD", ("EUR", "USD")), ("USDJPY", ("USD", "JPY")),
        ("GBPUSD", ("GBP", "USD")), ("UDI", ("USD", "EUR")),
    ].into();

    if let Some((from, to)) = forex_pairs.get(symbol) {
        let url = format!("https://api.frankfurter.app/latest?from={}&to={}", from, to);
        let resp = client.get(&url).send().await?;
        let text = resp.text().await?;
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(rate) = json["rates"][to].as_f64() {
                return Ok(format!("{} | 汇率: {:.4} {}/{} | 来源: frankfurter.app", display, rate, from, to));
            }
        }
    }

    anyhow::bail!("{} forex fallback 无数据", display)
}

/// Fetch from Tencent stock API - covers A-shares, HK, US stocks
async fn fetch_tencent_stock(symbol: &str) -> anyhow::Result<Vec<String>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let url = format!("https://qt.gtimg.cn/q={}", symbol);
    let resp = client
        .get(&url)
        .header("Referer", "https://finance.qq.com")
        .send()
        .await?;

    let text = resp.text().await?;
    // Response format: v_shXXXXXX="1~名称~代码~价格~...";
    let start = text.find('"').unwrap_or(0) + 1;
    let end = text.rfind('"').unwrap_or(text.len());
    if start >= end {
        anyhow::bail!("无法解析腾讯股票数据: {}", &text[..text.len().min(200)]);
    }
    let fields: Vec<String> = text[start..end].split('~').map(|s| s.to_string()).collect();
    if fields.len() < 10 {
        anyhow::bail!("数据字段不足: 仅{}个字段", fields.len());
    }
    Ok(fields)
}

/// Format Tencent stock data into readable text.
/// Fields: 0=market, 1=name, 2=code, 3=current price, 4=yesterday close,
///         5=open, 6=volume(hands), 7=outer, 8=inner, 9=buy1 price, ...
///         31=high, 32=low, 33=change%, ...
fn format_tencent_stock(fields: &[String], display: &str, market: &str, source: &str) -> anyhow::Result<String> {
    let name = if !fields[1].is_empty() { &fields[1] } else { display };
    let current = &fields[3];
    let yesterday_close = &fields[4];
    let open = &fields[5];

    let high = if fields.len() > 33 { &fields[33] } else if fields.len() > 31 { &fields[31] } else { "N/A" };
    let low = if fields.len() > 34 { &fields[34] } else if fields.len() > 32 { &fields[32] } else { "N/A" };
    let volume = if fields.len() > 36 { &fields[36] } else if fields.len() > 6 { &fields[6] } else { "N/A" };
    let amount = if fields.len() > 37 { &fields[37] } else { "N/A" };

    let cur_f: f64 = current.parse().unwrap_or(0.0);
    let yest_f: f64 = yesterday_close.parse().unwrap_or(0.0);
    let change_pct = if yest_f > 0.0 { (cur_f - yest_f) / yest_f * 100.0 } else { 0.0 };
    let change_abs = cur_f - yest_f;
    let emoji = if change_pct > 0.0 { "📈" } else if change_pct < 0.0 { "📉" } else { "➡️" };

    Ok(format!(
        "{emoji} {name}({market}) 实时行情\n\
当前价格: {current}\n\
涨跌幅: {change_pct:+.2}% ({change_abs:+.2}) {emoji}\n\
今开: {open}\n\
最高: {high}\n\
最低: {low}\n\
成交量: {volume}\n\
成交额: {amount}\n\
昨收: {yesterday_close}\n\
数据来源: {source}",
    ))
}

/// Fetch from Sina Finance API for futures/commodities/indices
async fn fetch_sina_futures(symbol: &str, display: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let url = format!("https://hq.sinajs.cn/list={}", symbol);
    let resp = client
        .get(&url)
        .header("Referer", "https://finance.sina.com.cn")
        .send()
        .await?;

    let body_bytes = resp.bytes().await?;
    // Sina returns GBK encoding for Chinese content
    let text = decode_gbk_or_utf8(&body_bytes);

    // Format: var hq_str_hf_GC="...,price,change,...";
    let start = text.find('"').unwrap_or(0) + 1;
    let end = text.rfind('"').unwrap_or(text.len());
    if start >= end || end - start < 5 {
        anyhow::bail!("Sina API 返回空数据: {}", display);
    }
    let content = &text[start..end];
    let fields: Vec<&str> = content.split(',').collect();

    if fields.len() < 8 {
        anyhow::bail!("Sina数据字段不足: {} 仅{}个字段", display, fields.len());
    }

    // Futures format: 0=current, 1=?, 2=buy, 3=sell, 4=high, 5=low, 6=time, 7=yesterday_close, 8=open, ...
    let current = fields[0];
    let high = if fields.len() > 4 { fields[4] } else { "N/A" };
    let low = if fields.len() > 5 { fields[5] } else { "N/A" };
    let yesterday = if fields.len() > 7 { fields[7] } else { "0" };
    let open = if fields.len() > 8 { fields[8] } else { "N/A" };
    let time_str = if fields.len() > 12 { fields[12] } else if fields.len() > 6 { fields[6] } else { "" };

    let cur_f: f64 = current.parse().unwrap_or(0.0);
    let yest_f: f64 = yesterday.parse().unwrap_or(0.0);
    let change_pct = if yest_f > 0.0 { (cur_f - yest_f) / yest_f * 100.0 } else { 0.0 };
    let change_abs = cur_f - yest_f;
    let emoji = if change_pct > 0.0 { "📈" } else if change_pct < 0.0 { "📉" } else { "➡️" };

    Ok(format!(
        "{emoji} {display} 实时行情\n\
当前价格: {current}\n\
涨跌幅: {change_pct:+.2}% ({change_abs:+.2}) {emoji}\n\
今开: {open}\n\
最高: {high}\n\
最低: {low}\n\
昨收: {yesterday}\n\
更新时间: {time_str}\n\
数据来源: 新浪财经",
    ))
}

/// Fetch index data from Sina Finance
async fn fetch_sina_index(symbol: &str, display: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    // Strip "b_" prefix for Sina: b_N225 → int_N225, b_GSPC → int_$GSPC, b_DJI → int_$DJI
    let sina_sym = if symbol.starts_with("b_") {
        let rest = &symbol[2..];
        match rest {
            "GSPC" => "int_$GSPC".to_string(),
            "DJI" => "int_$DJI".to_string(),
            "IXIC" => "int_$IXIC".to_string(),
            "N225" => "int_nikkei".to_string(),
            _ => format!("int_{}", rest),
        }
    } else {
        symbol.to_string()
    };

    let url = format!("https://hq.sinajs.cn/list={}", sina_sym);
    let resp = client
        .get(&url)
        .header("Referer", "https://finance.sina.com.cn")
        .send()
        .await?;

    let body_bytes = resp.bytes().await?;
    let text = decode_gbk_or_utf8(&body_bytes);

    let start = text.find('"').unwrap_or(0) + 1;
    let end = text.rfind('"').unwrap_or(text.len());
    if start >= end || end - start < 5 {
        anyhow::bail!("Sina指数API返回空数据: {}", display);
    }
    let content = &text[start..end];
    let fields: Vec<&str> = content.split(',').collect();

    if fields.len() < 3 {
        anyhow::bail!("Sina指数数据不足: {} 仅{}个字段", display, fields.len());
    }

    // Index format varies, try common patterns
    // International: name,current,change,...
    let name_field = fields[0];
    let current = fields.get(1).unwrap_or(&"N/A");
    let change_str = fields.get(7).unwrap_or(fields.get(2).unwrap_or(&"0"));
    let change_pct_str = fields.get(8).unwrap_or(fields.get(3).unwrap_or(&"0"));

    let cur_f: f64 = current.parse().unwrap_or(0.0);
    let change_f: f64 = change_str.parse().unwrap_or(0.0);
    let change_pct: f64 = change_pct_str.parse().unwrap_or(
        if cur_f > 0.0 && change_f.abs() > 0.0 { change_f / (cur_f - change_f) * 100.0 } else { 0.0 }
    );
    let emoji = if change_f > 0.0 { "📈" } else if change_f < 0.0 { "📉" } else { "➡️" };

    let time_str = fields.get(fields.len().saturating_sub(1)).unwrap_or(&"");
    let display_name = if !name_field.is_empty() && !name_field.chars().all(|c| c.is_numeric() || c == '.' || c == '-') {
        format!("{} ({})", display, name_field)
    } else {
        display.to_string()
    };

    Ok(format!(
        "{emoji} {display_name} 实时行情\n\
当前点位: {current}\n\
涨跌: {change_f:+.2} ({change_pct:+.2}%) {emoji}\n\
更新时间: {time_str}\n\
数据来源: 新浪财经",
    ))
}

/// Fetch index data from Yahoo Finance (tries v8 chart then v6 quote).
async fn fetch_yahoo_index(yahoo_symbol: &str, display: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()?;

    // Try v8 chart API first
    if let Ok(result) = fetch_yahoo_v8(&client, yahoo_symbol, display).await {
        return Ok(result);
    }
    let label = display;
    tracing::warn!("[Yahoo] v8 chart failed for {}, trying v6 quote", label);

    // Fallback: v6 quote API (different endpoint, often more stable)
    fetch_yahoo_v6(&client, yahoo_symbol, display).await
}

async fn fetch_yahoo_v8(client: &reqwest::Client, yahoo_symbol: &str, display: &str) -> anyhow::Result<String> {
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=1d",
        urlencoding::encode(yahoo_symbol)
    );

    let resp = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .send()
        .await?;

    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() || body.contains("<html") || body.contains("<!DOCTYPE") {
        anyhow::bail!("Yahoo v8 returned non-JSON for {}: HTTP {}", display, status);
    }

    let data: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| anyhow::anyhow!("Yahoo v8 JSON parse error for {}: {}", display, e))?;

    let result = &data["chart"]["result"];
    if !result.is_array() || result.as_array().map(|a| a.is_empty()).unwrap_or(true) {
        anyhow::bail!("Yahoo v8 无数据: {}", display);
    }

    let meta = &result[0]["meta"];
    let current = meta["regularMarketPrice"].as_f64().unwrap_or(0.0);
    let prev_close = meta["chartPreviousClose"].as_f64()
        .or_else(|| meta["previousClose"].as_f64())
        .unwrap_or(0.0);

    if current == 0.0 {
        anyhow::bail!("Yahoo v8 无效价格: {}", display);
    }

    format_yahoo_result(current, prev_close, meta["marketState"].as_str().unwrap_or("UNKNOWN"), display)
}

async fn fetch_yahoo_v6(client: &reqwest::Client, yahoo_symbol: &str, display: &str) -> anyhow::Result<String> {
    let url = format!(
        "https://query1.finance.yahoo.com/v6/finance/quote?symbols={}",
        urlencoding::encode(yahoo_symbol)
    );

    let resp = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .send()
        .await?;

    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() || body.contains("<html") {
        anyhow::bail!("Yahoo v6 returned non-JSON for {}: HTTP {}", display, status);
    }

    let data: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| anyhow::anyhow!("Yahoo v6 JSON parse error for {}: {}", display, e))?;

    let quotes = &data["quoteResponse"]["result"];
    if !quotes.is_array() || quotes.as_array().map(|a| a.is_empty()).unwrap_or(true) {
        anyhow::bail!("Yahoo v6 无数据: {}", display);
    }

    let q = &quotes[0];
    let current = q["regularMarketPrice"].as_f64().unwrap_or(0.0);
    let prev_close = q["regularMarketPreviousClose"].as_f64()
        .or_else(|| q["previousClose"].as_f64())
        .unwrap_or(0.0);

    if current == 0.0 {
        anyhow::bail!("Yahoo v6 无效价格: {}", display);
    }

    format_yahoo_result(current, prev_close, q["marketState"].as_str().unwrap_or("UNKNOWN"), display)
}

fn format_yahoo_result(current: f64, prev_close: f64, market_state: &str, display: &str) -> anyhow::Result<String> {
    let change = current - prev_close;
    let change_pct = if prev_close > 0.0 { change / prev_close * 100.0 } else { 0.0 };
    let emoji = if change > 0.0 { "📈" } else if change < 0.0 { "📉" } else { "➡️" };

    let state_text = match market_state {
        "REGULAR" => "交易中",
        "PRE" => "盘前",
        "POST" | "POSTPOST" => "盘后",
        "CLOSED" => "已收盘",
        _ => market_state,
    };

    Ok(format!(
        "{emoji} {display} 实时行情\n\
当前点位: {current:.2}\n\
涨跌: {change:+.2} ({change_pct:+.2}%) {emoji}\n\
市场状态: {state_text}\n\
数据来源: Yahoo Finance",
    ))
}

fn decode_gbk_or_utf8(bytes: &[u8]) -> String {
    // Try UTF-8 first
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    // Fall back to GBK decoding
    use encoding_rs::GBK;
    let (cow, _, _) = GBK.decode(bytes);
    cow.to_string()
}

fn format_volume(v: &str) -> String {
    let f: f64 = v.parse().unwrap_or(0.0);
    if f >= 1_000_000_000.0 { format!("{:.2}B", f / 1_000_000_000.0) }
    else if f >= 1_000_000.0 { format!("{:.2}M", f / 1_000_000.0) }
    else { format!("{:.0}", f) }
}

pub fn take_screenshot_sync(output_path: &str) -> anyhow::Result<String> {
    let monitors = xcap::Monitor::all().map_err(|e| anyhow::anyhow!("枚举显示器失败: {}", e))?;
    let monitor = monitors
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("未找到显示器"))?;
    let image = monitor
        .capture_image()
        .map_err(|e| anyhow::anyhow!("截图失败: {}", e))?;
    let p = std::path::Path::new(output_path);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    image
        .save(output_path)
        .map_err(|e| anyhow::anyhow!("保存截图失败: {}", e))?;
    Ok(output_path.to_string())
}

async fn take_screenshot(output_path: &str) -> anyhow::Result<String> {
    let path = output_path.to_string();
    tokio::task::spawn_blocking(move || {
        let monitors =
            xcap::Monitor::all().map_err(|e| anyhow::anyhow!("枚举显示器失败: {}", e))?;
        let monitor = monitors
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("未找到显示器"))?;
        let image = monitor
            .capture_image()
            .map_err(|e| anyhow::anyhow!("截图失败: {}", e))?;
        let p = std::path::Path::new(&path);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        image
            .save(&path)
            .map_err(|e| anyhow::anyhow!("保存截图失败: {}", e))?;
        Ok(path)
    })
    .await?
}

#[tauri::command]
pub async fn chat_stream_start(
    app: AppHandle,
    message: String,
    history: Vec<HistoryMessage>,
) -> ApiResult<String> {
    use crate::core::engine::*;

    let cfg = match config::load_config(&app).await {
        Ok(c) => c,
        Err(e) => return ApiResult::err(format!("Config error: {}", e)),
    };

    let engine = match AgentEngine::from_config(&cfg) {
        Ok(e) => e,
        Err(e) => return ApiResult::err(format!("Engine error: {}", e)),
    };

    let stream_id = Uuid::new_v4().to_string();
    let sid = stream_id.clone();
    let sink = TauriEventSink::new(app);

    tokio::spawn(async move {
        let history_msgs = history_messages_to_chat(&history);
        let full_prompt = crate::build_full_system_prompt(&cfg, Some(&message));
        let messages = build_messages(&full_prompt, &history_msgs, &message);

        let agent_cfg = AgentConfig {
            max_rounds: 8,
            tools_enabled: cfg.tools.shell_enabled,
            audit: None,
            audit_source: crate::security::audit::AuditSource::Local,
            memory: None,
            planning_enabled: true,
        };

        engine.run(messages, &sid, &sink, &agent_cfg).await;
    });

    ApiResult::ok(stream_id)
}

#[derive(Debug, Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub display_name: String,
    pub is_local: bool,
    pub supports_streaming: bool,
}

#[tauri::command]
pub async fn list_models(app: AppHandle) -> ApiResult<Vec<ModelInfo>> {
    let cfg = match config::load_config(&app).await {
        Ok(c) => c,
        Err(e) => return ApiResult::err(format!("Config error: {}", e)),
    };

    let router = match crate::models::ModelRouter::from_config(&cfg) {
        Ok(r) => r,
        Err(e) => return ApiResult::err(format!("Model router error: {}", e)),
    };

    let models: Vec<ModelInfo> = router
        .list_available()
        .into_iter()
        .map(|p| ModelInfo {
            name: p.name,
            display_name: p.display_name,
            is_local: p.is_local,
            supports_streaming: p.supports_streaming,
        })
        .collect();

    ApiResult::ok(models)
}

#[tauri::command]
pub async fn get_audit_log(
    state: State<'_, Arc<AuditLogger>>,
    limit: Option<usize>,
) -> Result<ApiResult<Vec<serde_json::Value>>, String> {
    let entries = state.recent(limit.unwrap_or(100)).await;
    let values: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|e| serde_json::to_value(e).unwrap_or_default())
        .collect();
    Ok(ApiResult::ok(values))
}

pub fn create_remote_task_approval(
    app: &AppHandle,
    state: &ApprovalState,
    user_id: &str,
    task_text: &str,
) -> PendingApproval {
    let approval = PendingApproval {
        id: uuid::Uuid::new_v4().to_string(),
        operation: "remote_task".to_string(),
        risk_level: "moderate".to_string(),
        description: format!("远程任务执行审批（来自飞书用户 {}）", user_id),
        details: serde_json::json!({
            "source": "feishu",
            "user_id": user_id,
            "task_text": task_text,
        }),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    state.create(approval.clone());
    let _ = app.emit("approval-request", approval.clone());
    approval
}

pub fn approve_operation_internal(
    state: &ApprovalState,
    id: &str,
) -> Result<PendingApproval, String> {
    if let Ok(mut pending) = DESKTOP_APPROVALS.lock() {
        if let Some(tx) = pending.remove(id) {
            let _ = tx.send(true);
        }
    }
    state
        .resolve(id)
        .ok_or_else(|| format!("No pending approval with id '{}'", id))
}

pub fn reject_operation_internal(
    state: &ApprovalState,
    id: &str,
) -> Result<PendingApproval, String> {
    if let Ok(mut pending) = DESKTOP_APPROVALS.lock() {
        if let Some(tx) = pending.remove(id) {
            let _ = tx.send(false);
        }
    }
    state
        .resolve(id)
        .ok_or_else(|| format!("No pending approval with id '{}'", id))
}

#[tauri::command]
pub fn approve_operation(state: State<'_, ApprovalState>, id: String) -> ApiResult<()> {
    match approve_operation_internal(&state, &id) {
        Ok(approval) => {
            tracing::info!("Operation approved: {} ({})", id, approval.operation);
            ApiResult::ok(())
        }
        Err(err) => ApiResult::err(err),
    }
}

#[tauri::command]
pub fn reject_operation(
    state: State<'_, ApprovalState>,
    id: String,
    reason: Option<String>,
) -> ApiResult<()> {
    match reject_operation_internal(&state, &id) {
        Ok(approval) => {
            tracing::info!(
                "Operation rejected: {} ({}) (reason: {:?})",
                id,
                approval.operation,
                reason
            );
            ApiResult::ok(())
        }
        Err(err) => ApiResult::err(err),
    }
}

#[tauri::command]
pub fn list_pending_approvals(state: State<'_, ApprovalState>) -> ApiResult<Vec<PendingApproval>> {
    ApiResult::ok(state.list())
}

#[tauri::command]
pub async fn store_credential(key: String, secret: String) -> ApiResult<()> {
    match CredentialStore::store(&key, &secret) {
        Ok(()) => ApiResult::ok(()),
        Err(e) => ApiResult::err(format!("Failed to store credential: {}", e)),
    }
}

#[tauri::command]
pub async fn delete_credential(key: String) -> ApiResult<()> {
    match CredentialStore::delete(&key) {
        Ok(()) => ApiResult::ok(()),
        Err(e) => ApiResult::err(format!("Failed to delete credential: {}", e)),
    }
}

#[tauri::command]
pub fn get_credential_preview(key: String) -> ApiResult<String> {
    match CredentialStore::retrieve(&key) {
        Ok(secret) => {
            let len = secret.len();
            if len <= 8 {
                ApiResult::ok("****".to_string())
            } else {
                let preview = format!("{}****{}", &secret[..4], &secret[len - 4..]);
                ApiResult::ok(preview)
            }
        }
        Err(_) => ApiResult::ok(String::new()),
    }
}

#[tauri::command]
pub fn check_credentials(keys: Vec<String>) -> ApiResult<Vec<CredentialStatus>> {
    let statuses = keys
        .into_iter()
        .map(|key| {
            let exists = CredentialStore::exists(&key);
            CredentialStatus { key, exists }
        })
        .collect();
    ApiResult::ok(statuses)
}

#[derive(Debug, Serialize)]
pub struct CredentialStatus {
    pub key: String,
    pub exists: bool,
}

#[tauri::command]
pub fn get_risk_level(operation: String) -> ApiResult<String> {
    let engine = crate::security::risk::RiskEngine::new(std::collections::HashMap::new());
    let level = engine.assess(&operation);
    ApiResult::ok(format!("{:?}", level))
}

fn get_memory_store(app: &AppHandle) -> MemoryStore {
    let data_dir = app
        .path()
        .app_data_dir()
        .expect("failed to resolve app data dir");
    MemoryStore::new(data_dir)
}

#[tauri::command]
pub async fn save_conversation(app: AppHandle, conversation: Conversation) -> ApiResult<()> {
    let store = get_memory_store(&app);
    match store.save_conversation(&conversation).await {
        Ok(()) => ApiResult::ok(()),
        Err(e) => ApiResult::err(format!("Failed to save conversation: {}", e)),
    }
}

#[tauri::command]
pub async fn load_conversation(app: AppHandle, id: String) -> ApiResult<Conversation> {
    let store = get_memory_store(&app);
    match store.load_conversation(&id).await {
        Ok(conv) => ApiResult::ok(conv),
        Err(e) => ApiResult::err(format!("Failed to load conversation: {}", e)),
    }
}

#[tauri::command]
pub async fn list_conversations(app: AppHandle) -> ApiResult<Vec<ConversationSummary>> {
    let store = get_memory_store(&app);
    match store.list_conversations().await {
        Ok(list) => ApiResult::ok(list),
        Err(e) => ApiResult::err(format!("Failed to list conversations: {}", e)),
    }
}

#[tauri::command]
pub async fn delete_conversation(app: AppHandle, id: String) -> ApiResult<()> {
    let store = get_memory_store(&app);
    match store.delete_conversation(&id).await {
        Ok(()) => ApiResult::ok(()),
        Err(e) => ApiResult::err(format!("Failed to delete conversation: {}", e)),
    }
}

#[tauri::command]
pub async fn get_mcp_status(
    app: AppHandle,
) -> ApiResult<Vec<(String, usize)>> {
    match app.try_state::<std::sync::Arc<crate::mcp::client::McpClientManager>>() {
        Some(mgr) => ApiResult::ok(mgr.status().await),
        None => ApiResult::ok(vec![]),
    }
}

#[tauri::command]
pub async fn get_perf_metrics() -> ApiResult<serde_json::Value> {
    let events = PERF_EVENTS.lock().ok().map(|g| g.clone()).unwrap_or_default();
    let total = events.len();
    let chat_events: Vec<&PerfEvent> = events.iter().filter(|e| e.event_type == "remote_chat").collect();
    let enrich_events: Vec<&PerfEvent> = events.iter().filter(|e| e.event_type == "enrich").collect();
    let tool_events: Vec<&PerfEvent> = events.iter().filter(|e| e.event_type == "tool_call").collect();
    let sched_events: Vec<&PerfEvent> = events.iter().filter(|e| e.event_type == "scheduled_task").collect();

    let avg_chat_ms = if chat_events.is_empty() { 0 } else {
        chat_events.iter().map(|e| e.duration_ms).sum::<u64>() / chat_events.len() as u64
    };
    let avg_enrich_ms = if enrich_events.is_empty() { 0 } else {
        enrich_events.iter().map(|e| e.duration_ms).sum::<u64>() / enrich_events.len() as u64
    };

    ApiResult::ok(serde_json::json!({
        "total_events": total,
        "summary": {
            "chat_count": chat_events.len(),
            "chat_avg_ms": avg_chat_ms,
            "enrich_count": enrich_events.len(),
            "enrich_avg_ms": avg_enrich_ms,
            "tool_call_count": tool_events.len(),
            "scheduled_task_count": sched_events.len(),
        },
        "recent_events": events.iter().rev().take(50).collect::<Vec<_>>(),
    }))
}
