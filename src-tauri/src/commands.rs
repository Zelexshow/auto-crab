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

/// Build tool definitions from the tool registry.
pub fn build_tool_definitions() -> Vec<ToolDefinition> {
    ToolRegistry::new().to_tool_definitions()
}

/// Map tool name to the operation type used by the risk engine.
fn tool_operation_type(name: &str) -> &str {
    match name {
        "read_file" | "list_directory" | "fetch_webpage" => "read_file",
        "write_file" => "write_file",
        "execute_shell" => "execute_shell",
        "delete_file" => "delete_file",
        "screenshot" | "analyze_screen" => "read_file",
        "mouse_click" | "keyboard_type" | "key_press" => "execute_shell",
        _ => "unknown",
    }
}

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
        "find" | "wc" | "sort" | "grep" | "rg" | "tree"
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
            match do_mouse_click(x, y, click_type).await {
                Ok(msg) => msg,
                Err(e) => format!("mouse_click 失败: {}", e),
            }
        }
        "keyboard_type" => {
            let text = args["text"].as_str().unwrap_or("");
            match do_keyboard_type(text).await {
                Ok(msg) => msg,
                Err(e) => format!("keyboard_type 失败: {}", e),
            }
        }
        "key_press" => {
            let key = args["key"].as_str().unwrap_or("enter");
            match do_key_press(key).await {
                Ok(msg) => msg,
                Err(e) => format!("key_press 失败: {}", e),
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
        "analyze_screen" => {
            let data_dir = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_else(|_| ".".into());
            let tmp_path = format!("{}\\AppData\\Local\\Temp\\auto-crab-screen.png", data_dir);
            match take_screenshot(&tmp_path).await {
                Ok(path) => {
                    let prompt = args["question"]
                        .as_str()
                        .unwrap_or("请详细描述截图中的内容");
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

    let resp = client
        .post("https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().await.unwrap_or_default();
        anyhow::bail!("VL API error {}: {}", status, err_body);
    }

    let api_resp: serde_json::Value = resp.json().await?;
    let content = api_resp["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("视觉模型未返回内容")
        .to_string();

    Ok(content)
}

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
    let cfg = match config::load_config(&app).await {
        Ok(c) => c,
        Err(e) => return ApiResult::err(format!("Config error: {}", e)),
    };

    let router = match crate::models::ModelRouter::from_config(&cfg) {
        Ok(r) => r,
        Err(e) => return ApiResult::err(format!("Model router error: {}", e)),
    };

    let stream_id = Uuid::new_v4().to_string();
    let sid = stream_id.clone();

    let provider = match router.get_primary() {
        Some(p) => p,
        None => {
            tracing::error!("[Desktop] No primary provider! Config models.primary: {:?}", cfg.models.primary.as_ref().map(|m| &m.provider));
            return ApiResult::err("No model provider configured. 请检查模型配置和 API Key。");
        }
    };

    let emitter = app;
    tokio::spawn(async move {
        tracing::info!("[Desktop] Agent loop starting for message: {}", message.chars().take(50).collect::<String>());

        // ── Build initial messages ───────────────────────────────────────
        let mut messages = vec![ChatMessage {
            role: MessageRole::System,
            content: cfg.agent.system_prompt.clone(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }];
        for h in &history {
            let role = match h.role.as_str() {
                "assistant" => MessageRole::Assistant,
                "system" => MessageRole::System,
                _ => MessageRole::User,
            };
            messages.push(ChatMessage {
                role,
                content: h.content.clone(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            });
        }
        messages.push(ChatMessage {
            role: MessageRole::User,
            content: message.clone(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });

        // ── Build tool context ───────────────────────────────────────────
        let file_roots: Vec<PathBuf> = cfg
            .tools
            .file_access
            .iter()
            .map(|s| PathBuf::from(shellexpand::tilde(s).to_string()))
            .collect();
        let file_ops = FileOps::new(file_roots);
        let shell = ShellExecutor::new(
            cfg.tools.shell_enabled,
            cfg.tools.shell_allowed_commands.clone(),
        );
        let tool_defs = build_tool_definitions();

        // ── Agent loop (max 8 tool-call rounds) ─────────────────────────
        let max_rounds = 8usize;
        for round in 0..=max_rounds {
            let use_tools = round < max_rounds && cfg.tools.shell_enabled;
            let req = ChatRequest {
                messages: messages.clone(),
                tools: if use_tools {
                    Some(tool_defs.clone())
                } else {
                    None
                },
                temperature: 0.7,
                max_tokens: None,
            };

            let thinking_id = Uuid::new_v4().to_string();
            let _ = emitter.emit("agent-step", serde_json::json!({
                "id": &thinking_id, "stream_id": &sid,
                "type": "thinking",
                "content": if round == 0 { "正在分析你的请求..." } else { "继续思考中..." },
                "status": "running",
                "timestamp": chrono::Utc::now().timestamp_millis(),
            }));

            let _ = emitter.emit("chat-stream-chunk", serde_json::json!({
                "stream_id": &sid,
                "delta": if round == 0 { "" } else { "" },
                "done": false,
            }));

            tracing::info!("[Desktop] Round {} - calling model with {} messages, tools: {}", round, messages.len(), use_tools);

            let has_pending_tool_calls = {
                match provider.chat(req.clone()).await {
                    Err(e) => {
                        tracing::error!("[Desktop] Model call FAILED: {}", e);
                        let _ = emitter.emit(
                            "chat-stream-error",
                            serde_json::json!({
                                "stream_id": &sid, "error": e.to_string()
                            }),
                        );
                        return;
                    }
                    Ok(resp) => {
                        tracing::info!("[Desktop] Round {} - model responded, tool_calls: {}, content_len: {}",
                            round,
                            resp.message.tool_calls.as_ref().map(|t| t.len()).unwrap_or(0),
                            resp.message.content.len()
                        );

                        let _ = emitter.emit("agent-step", serde_json::json!({
                            "id": &thinking_id, "stream_id": &sid,
                            "type": "thinking",
                            "content": format!("第{}轮思考完成", round + 1),
                            "status": "done",
                            "timestamp": chrono::Utc::now().timestamp_millis(),
                        }));

                        let tool_calls = resp.message.tool_calls.clone().unwrap_or_default();
                        if !tool_calls.is_empty() {
                            for tc in &tool_calls {
                                let step_id = Uuid::new_v4().to_string();
                                let op = tool_operation_type(&tc.name);
                                let risk_engine = RiskEngine::new(HashMap::new());
                                let risk = risk_engine.assess(op);

                                if risk == crate::config::RiskLevel::Forbidden {
                                    let _ = emitter.emit(
                                        "agent-step",
                                        serde_json::json!({
                                            "id": &step_id, "stream_id": &sid,
                                            "type": "tool_result", "tool": &tc.name,
                                            "content": format!("操作被禁止: {}", tc.name),
                                            "status": "error",
                                            "timestamp": chrono::Utc::now().timestamp_millis(),
                                        }),
                                    );
                                    messages.push(ChatMessage {
                                        role: MessageRole::Tool,
                                        content: format!("操作被禁止: {}", tc.name),
                                        name: Some(tc.name.clone()),
                                        tool_calls: None,
                                        tool_call_id: Some(tc.id.clone()),
                                    });
                                    continue;
                                }

                                let is_safe_shell = tc.name == "execute_shell" && is_readonly_shell_command(&tc.arguments);
                                let needs_approval = !is_safe_shell && matches!(
                                    risk,
                                    crate::config::RiskLevel::Moderate
                                        | crate::config::RiskLevel::Dangerous
                                );

                                if needs_approval {
                                    let approval_id = Uuid::new_v4().to_string();
                                    let risk_str = match risk {
                                        crate::config::RiskLevel::Dangerous => "dangerous",
                                        _ => "moderate",
                                    };
                                    let _ = emitter.emit("approval-request", serde_json::json!({
                                        "id": &approval_id,
                                        "operation": &tc.name,
                                        "risk_level": risk_str,
                                        "description": format!("{}({})", tc.name, tc.arguments.chars().take(80).collect::<String>()),
                                    }));
                                    let _ = emitter.emit("agent-step", serde_json::json!({
                                        "id": &step_id, "stream_id": &sid,
                                        "type": "tool_call", "tool": &tc.name,
                                        "content": format!("[等待审批] {}({})", tc.name, tc.arguments.chars().take(60).collect::<String>()),
                                        "status": "blocked",
                                        "timestamp": chrono::Utc::now().timestamp_millis(),
                                    }));

                                    let (atx, arx) = tokio::sync::oneshot::channel::<bool>();
                                    {
                                        let mut pending = DESKTOP_APPROVALS.lock().unwrap();
                                        pending.insert(approval_id.clone(), atx);
                                    }

                                    let approved = tokio::time::timeout(
                                        std::time::Duration::from_secs(120),
                                        arx,
                                    )
                                    .await
                                    .ok()
                                    .and_then(|r| r.ok())
                                    .unwrap_or(false);

                                    if !approved {
                                        let _ = emitter.emit(
                                            "agent-step",
                                            serde_json::json!({
                                                "id": &step_id, "stream_id": &sid,
                                                "type": "tool_result", "tool": &tc.name,
                                                "content": "用户拒绝或审批超时",
                                                "status": "error",
                                                "timestamp": chrono::Utc::now().timestamp_millis(),
                                            }),
                                        );
                                        messages.push(ChatMessage {
                                            role: MessageRole::Tool,
                                            content: "操作被用户拒绝或审批超时".to_string(),
                                            name: Some(tc.name.clone()),
                                            tool_calls: None,
                                            tool_call_id: Some(tc.id.clone()),
                                        });
                                        continue;
                                    }
                                }

                                let _ = emitter.emit(
                                    "agent-step",
                                    serde_json::json!({
                                        "id": &step_id, "stream_id": &sid,
                                        "type": "tool_call", "tool": &tc.name,
                                        "content": &tc.arguments,
                                        "status": "running",
                                        "timestamp": chrono::Utc::now().timestamp_millis(),
                                    }),
                                );

                                let result = dispatch_tool(tc, &file_ops, &shell).await;

                                let _ = emitter.emit(
                                    "agent-step",
                                    serde_json::json!({
                                        "id": &step_id, "stream_id": &sid,
                                        "type": "tool_result", "tool": &tc.name,
                                        "content": &result,
                                        "status": "done",
                                        "timestamp": chrono::Utc::now().timestamp_millis(),
                                    }),
                                );

                                // Add assistant tool-call message + tool result to history
                                messages.push(ChatMessage {
                                    role: MessageRole::Assistant,
                                    content: String::new(),
                                    name: None,
                                    tool_calls: Some(vec![tc.clone()]),
                                    tool_call_id: None,
                                });
                                messages.push(ChatMessage {
                                    role: MessageRole::Tool,
                                    content: result,
                                    name: Some(tc.name.clone()),
                                    tool_calls: None,
                                    tool_call_id: Some(tc.id.clone()),
                                });
                            }
                            true // continue agent loop
                        } else {
                            let final_content = resp.message.content.clone();
                            tracing::info!("[Desktop] Emitting final answer ({} chars)", final_content.len());

                            let test_result = emitter.emit(
                                "chat-stream-chunk",
                                serde_json::json!({
                                    "stream_id": &sid,
                                    "delta": &final_content,
                                    "done": false,
                                }),
                            );
                            tracing::info!("[Desktop] Emit result: {:?}", test_result);

                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                            let _ = emitter.emit(
                                "chat-stream-chunk",
                                serde_json::json!({
                                    "stream_id": &sid, "delta": "", "done": true,
                                }),
                            );
                            let _ =
                                emitter.emit("agent-done", serde_json::json!({ "stream_id": &sid }));
                            false
                        }
                    }
                }
            };

            if !has_pending_tool_calls {
                break;
            }

            if round == max_rounds {
                let _ = emitter.emit(
                    "chat-stream-chunk",
                    serde_json::json!({
                        "stream_id": &sid,
                        "delta": "\n\n⚠️ 已达到最大工具调用轮次，操作停止。",
                        "done": true,
                    }),
                );
                let _ = emitter.emit("agent-done", serde_json::json!({ "stream_id": &sid }));
            }
        }
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
