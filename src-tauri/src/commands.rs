use crate::config;
use crate::core::memory::{Conversation, ConversationSummary, MemoryStore};
use crate::models::provider::*;
use crate::security::credentials::CredentialStore;
use crate::tools::file_ops::FileOps;
use crate::tools::shell::ShellExecutor;
use crate::tools::registry::ToolRegistry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, State};
use std::sync::Mutex;
use uuid::Uuid;

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
        Self { success: true, data: Some(data), error: None }
    }

    pub fn err(msg: impl ToString) -> Self {
        Self { success: false, data: None, error: Some(msg.to_string()) }
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

/// Execute a single tool call, returning its output as a string.
pub async fn dispatch_tool(
    tc: &ToolCall,
    file_ops: &FileOps,
    shell: &ShellExecutor,
) -> String {
    let args: serde_json::Value = serde_json::from_str(&tc.arguments)
        .unwrap_or(serde_json::Value::Null);

    match tc.name.as_str() {
        "read_file" => {
            let path = args["path"].as_str().unwrap_or("");
            match file_ops.read_file(path).await {
                Ok(c) if c.len() > 12000 => format!("{}…\n[已截断，共 {} 字符]", &c[..12000], c.len()),
                Ok(c) => c,
                Err(e) => format!("read_file 失败: {}", e),
            }
        }
        "write_file" => {
            let path = args["path"].as_str().unwrap_or("");
            let content = args["content"].as_str().unwrap_or("");
            match file_ops.write_file(path, content).await {
                Ok(()) => format!("文件已写入: {}", path),
                Err(e) => format!("write_file 失败: {}", e),
            }
        }
        "list_directory" => {
            let path = args["path"].as_str().unwrap_or(".");
            match file_ops.list_directory(path).await {
                Ok(entries) => entries.iter().map(|e| {
                    if e.is_dir { format!("📁 {}/", e.name) }
                    else { format!("📄 {} ({} bytes)", e.name, e.size) }
                }).collect::<Vec<_>>().join("\n"),
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
                        if !r.is_empty() { r.push('\n'); }
                        r.push_str("[stderr] ");
                        r.push_str(&output.stderr);
                    }
                    r.push_str(&format!("\n[exit: {}]", output.exit_code));
                    r
                }
                Err(e) => format!("execute_shell 失败: {}", e),
            }
        }
        _ => format!("未知工具: {}", tc.name),
    }
}

#[tauri::command]
pub async fn chat_stream_start(
    app: AppHandle,
    message: String,
    history: Vec<HistoryMessage>,
    window: tauri::Window,
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
        None => return ApiResult::err("No model provider configured"),
    };

    tokio::spawn(async move {

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
                "system"    => MessageRole::System,
                _           => MessageRole::User,
            };
            messages.push(ChatMessage { role, content: h.content.clone(), name: None, tool_calls: None, tool_call_id: None });
        }
        messages.push(ChatMessage {
            role: MessageRole::User,
            content: message.clone(),
            name: None, tool_calls: None, tool_call_id: None,
        });

        // ── Build tool context ───────────────────────────────────────────
        let file_roots: Vec<PathBuf> = cfg.tools.file_access.iter()
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
                tools: if use_tools { Some(tool_defs.clone()) } else { None },
                temperature: 0.7,
                max_tokens: None,
            };

            // Use non-streaming for intermediate tool-call rounds,
            // streaming only for the final answer round.
            let has_pending_tool_calls = {
                // check last call via non-streaming first
                match provider.chat(req.clone()).await {
                    Err(e) => {
                        let _ = window.emit("chat-stream-error", serde_json::json!({
                            "stream_id": &sid, "error": e.to_string()
                        }));
                        return;
                    }
                    Ok(resp) => {
                        let tool_calls = resp.message.tool_calls.clone()
                            .unwrap_or_default();
                        if !tool_calls.is_empty() {
                            // Emit agent-step for each tool call
                            for tc in &tool_calls {
                                let step_id = Uuid::new_v4().to_string();
                                let _ = window.emit("agent-step", serde_json::json!({
                                    "id": &step_id,
                                    "stream_id": &sid,
                                    "type": "tool_call",
                                    "tool": &tc.name,
                                    "content": &tc.arguments,
                                    "status": "running",
                                    "timestamp": chrono::Utc::now().timestamp_millis(),
                                }));

                                let result = dispatch_tool(tc, &file_ops, &shell).await;

                                let _ = window.emit("agent-step", serde_json::json!({
                                    "id": &step_id,
                                    "stream_id": &sid,
                                    "type": "tool_result",
                                    "tool": &tc.name,
                                    "content": &result,
                                    "status": "done",
                                    "timestamp": chrono::Utc::now().timestamp_millis(),
                                }));

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
                            // Final answer: stream it
                            let final_content = resp.message.content.clone();
                            // Stream the content in chunks for smooth display
                            let chars: Vec<char> = final_content.chars().collect();
                            for chunk in chars.chunks(8) {
                                let delta: String = chunk.iter().collect();
                                let _ = window.emit("chat-stream-chunk", serde_json::json!({
                                    "stream_id": &sid,
                                    "delta": delta,
                                    "done": false,
                                }));
                            }
                            let _ = window.emit("chat-stream-chunk", serde_json::json!({
                                "stream_id": &sid, "delta": "", "done": true,
                            }));
                            let _ = window.emit("agent-done", serde_json::json!({ "stream_id": &sid }));
                            false
                        }
                    }
                }
            };

            if !has_pending_tool_calls {
                break;
            }

            if round == max_rounds {
                let _ = window.emit("chat-stream-chunk", serde_json::json!({
                    "stream_id": &sid,
                    "delta": "\n\n⚠️ 已达到最大工具调用轮次，操作停止。",
                    "done": true,
                }));
                let _ = window.emit("agent-done", serde_json::json!({ "stream_id": &sid }));
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

    let models: Vec<ModelInfo> = router.list_available().into_iter().map(|p| ModelInfo {
        name: p.name,
        display_name: p.display_name,
        is_local: p.is_local,
        supports_streaming: p.supports_streaming,
    }).collect();

    ApiResult::ok(models)
}

#[tauri::command]
pub async fn get_audit_log(_limit: Option<usize>) -> ApiResult<Vec<serde_json::Value>> {
    ApiResult::ok(vec![])
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
    state
        .resolve(id)
        .ok_or_else(|| format!("No pending approval with id '{}'", id))
}

pub fn reject_operation_internal(
    state: &ApprovalState,
    id: &str,
) -> Result<PendingApproval, String> {
    state
        .resolve(id)
        .ok_or_else(|| format!("No pending approval with id '{}'", id))
}

#[tauri::command]
pub fn approve_operation(
    state: State<'_, ApprovalState>,
    id: String,
) -> ApiResult<()> {
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
pub fn list_pending_approvals(
    state: State<'_, ApprovalState>,
) -> ApiResult<Vec<PendingApproval>> {
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
pub fn get_risk_level(operation: String) -> ApiResult<String> {
    let engine = crate::security::risk::RiskEngine::new(std::collections::HashMap::new());
    let level = engine.assess(&operation);
    ApiResult::ok(format!("{:?}", level))
}

fn get_memory_store(app: &AppHandle) -> MemoryStore {
    let data_dir = app.path().app_data_dir().expect("failed to resolve app data dir");
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
