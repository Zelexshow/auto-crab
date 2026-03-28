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
    match config::save_config(&app, &config_data).await {
        Ok(()) => ApiResult::ok(()),
        Err(e) => ApiResult::err(format!("Failed to save config: {}", e)),
    }
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
        "search_web" | "get_crypto_price" => "search_web",
        "screenshot" | "analyze_screen" | "get_ui_tree" => "read_file",
        "focus_window" => "write_file",
        "write_file" => "write_file",
        "execute_shell" | "analyze_and_act" | "quick_reply_wechat" => "execute_shell",
        "mouse_click" | "keyboard_type" | "key_press" => "execute_shell",
        "delete_file" => "delete_file",
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

    let result = dispatch_tool(tc, file_ops, shell).await;

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

            let data_dir = directories::ProjectDirs::from("com", "zelex", "auto-crab")
                .map(|d| d.data_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            let snapshots = crate::core::snapshots::SnapshotStore::new(data_dir);
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

async fn search_web(query: &str) -> anyhow::Result<String> {
    // Try Bing first (accessible in China), then fallback to DuckDuckGo
    match search_bing(query).await {
        Ok(results) if !results.is_empty() => {
            return Ok(format!("搜索 \"{}\" 的结果:\n\n{}", query, results));
        }
        Err(e) => tracing::warn!("Bing search failed: {}, trying DuckDuckGo", e),
        _ => tracing::warn!("Bing returned no results, trying DuckDuckGo"),
    }

    match search_duckduckgo(query).await {
        Ok(results) if !results.is_empty() => {
            Ok(format!("搜索 \"{}\" 的结果:\n\n{}", query, results))
        }
        Ok(_) => anyhow::bail!("未找到搜索结果"),
        Err(e) => anyhow::bail!("搜索失败: {}", e),
    }
}

async fn search_bing(query: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()?;

    let url = format!("https://cn.bing.com/search?q={}&ensearch=0", urlencoding::encode(query));
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

    for _ in 0..8 {
        if let Some(start) = html[pos..].find("class=\"result__a\"") {
            let abs = pos + start;
            if let Some(href_start) = html[abs..].find("href=\"") {
                let href_abs = abs + href_start + 6;
                if let Some(href_end) = html[href_abs..].find('"') {
                    let url = &html[href_abs..href_abs + href_end];
                    let url = url.replace("&amp;", "&");

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

                    if !title.is_empty() && !url.starts_with("/") {
                        results.push(format!("{}. {}\n   {}\n   {}", results.len() + 1, title, snippet, url));
                    }
                    pos = title_end;
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
        let messages = build_messages(&cfg.agent.system_prompt, &history_msgs, &message);

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
