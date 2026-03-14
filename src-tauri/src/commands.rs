use crate::config;
use crate::models::provider::*;
use crate::security::credentials::CredentialStore;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

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

#[tauri::command]
pub async fn chat_stream_start(
    app: AppHandle,
    message: String,
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

    let stream_id = uuid::Uuid::new_v4().to_string();
    let sid = stream_id.clone();

    let provider = match router.get_primary() {
        Some(p) => p,
        None => return ApiResult::err("No model provider configured"),
    };

    tokio::spawn(async move {
        use futures::StreamExt;

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
                    content: message,
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            tools: None,
            temperature: 0.7,
            max_tokens: None,
        };

        match provider.chat_stream(chat_req).await {
            Ok(mut stream) => {
                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            let _ = window.emit("chat-stream-chunk", serde_json::json!({
                                "stream_id": &sid,
                                "delta": chunk.delta,
                                "done": chunk.finish_reason.is_some(),
                            }));
                            if chunk.finish_reason.is_some() {
                                break;
                            }
                        }
                        Err(e) => {
                            let _ = window.emit("chat-stream-error", serde_json::json!({
                                "stream_id": &sid,
                                "error": e.to_string(),
                            }));
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                let _ = window.emit("chat-stream-error", serde_json::json!({
                    "stream_id": &sid,
                    "error": e.to_string(),
                }));
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

#[tauri::command]
pub async fn approve_operation(id: String) -> ApiResult<()> {
    tracing::info!("Operation approved: {}", id);
    ApiResult::ok(())
}

#[tauri::command]
pub async fn reject_operation(id: String, reason: Option<String>) -> ApiResult<()> {
    tracing::info!("Operation rejected: {} (reason: {:?})", id, reason);
    ApiResult::ok(())
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
