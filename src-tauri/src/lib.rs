mod config;
mod core;
mod security;
mod models;
mod tools;
mod remote;
mod plugins;
mod commands;

use std::collections::HashMap;
use tauri::{
    Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};
use tokio::sync::Mutex;
use tracing_subscriber::{fmt, EnvFilter};

const REMOTE_HISTORY_MAX_MESSAGES: usize = 20;

#[derive(Default)]
struct RemoteConversationState {
    sessions: Mutex<HashMap<String, Vec<crate::models::provider::ChatMessage>>>,
}

impl RemoteConversationState {
    async fn get_history(&self, session_key: &str) -> Vec<crate::models::provider::ChatMessage> {
        let sessions = self.sessions.lock().await;
        sessions.get(session_key).cloned().unwrap_or_default()
    }

    async fn append_turn(&self, session_key: &str, user: &str, assistant: &str) {
        let mut sessions = self.sessions.lock().await;
        let entry = sessions.entry(session_key.to_string()).or_default();
        entry.push(crate::models::provider::ChatMessage {
            role: crate::models::provider::MessageRole::User,
            content: user.to_string(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });
        entry.push(crate::models::provider::ChatMessage {
            role: crate::models::provider::MessageRole::Assistant,
            content: assistant.to_string(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });

        if entry.len() > REMOTE_HISTORY_MAX_MESSAGES {
            let overflow = entry.len() - REMOTE_HISTORY_MAX_MESSAGES;
            entry.drain(0..overflow);
        }
    }

    async fn clear(&self, session_key: &str) {
        let mut sessions = self.sessions.lock().await;
        sessions.remove(session_key);
    }
}

fn build_remote_reply(text: &str) -> String {
    let cmd = text.trim();
    if cmd.starts_with("/status") {
        format!(
            "🦀 Auto Crab 在线\n时间: {}\n状态: webhook 正常，远程通道已连通",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        )
    } else if cmd.starts_with("/task") {
        "已收到任务指令。当前版本正在接入任务执行链路，请先在桌面端查看执行结果。".into()
    } else if cmd.starts_with("/approve") || cmd.starts_with("/reject") {
        "已收到审批指令。当前版本正在接入审批回传链路，请先在桌面端审批弹窗操作。".into()
    } else {
        format!(
            "已收到你的消息：{}\n远程会话执行链路正在接入中，可先用 /status 验证连通性。",
            cmd
        )
    }
}

async fn run_remote_chat(
    cfg: &config::AppConfig,
    history: &[crate::models::provider::ChatMessage],
    user_input: &str,
) -> anyhow::Result<String> {
    use crate::models::provider::*;
    use crate::tools::file_ops::FileOps;
    use crate::tools::shell::ShellExecutor;

    let router = crate::models::ModelRouter::from_config(cfg)?;
    let provider = router.get_primary()
        .ok_or_else(|| anyhow::anyhow!("No model provider configured"))?;

    let mut messages = vec![ChatMessage {
        role: MessageRole::System,
        content: cfg.agent.system_prompt.clone(),
        name: None, tool_calls: None, tool_call_id: None,
    }];
    messages.extend(history.iter().cloned());
    messages.push(ChatMessage {
        role: MessageRole::User,
        content: user_input.to_string(),
        name: None, tool_calls: None, tool_call_id: None,
    });

    let file_roots: Vec<std::path::PathBuf> = cfg.tools.file_access.iter()
        .map(|s| std::path::PathBuf::from(shellexpand::tilde(s).to_string()))
        .collect();
    let file_ops = FileOps::new(file_roots);
    let shell = ShellExecutor::new(
        cfg.tools.shell_enabled,
        cfg.tools.shell_allowed_commands.clone(),
    );
    let tool_defs = commands::build_tool_definitions();

    for round in 0..=6usize {
        let use_tools = round < 6 && cfg.tools.shell_enabled;
        let req = ChatRequest {
            messages: messages.clone(),
            tools: if use_tools { Some(tool_defs.clone()) } else { None },
            temperature: 0.7,
            max_tokens: None,
        };

        let resp = provider.chat(req).await?;
        let tool_calls = resp.message.tool_calls.clone().unwrap_or_default();

        if !tool_calls.is_empty() {
            for tc in &tool_calls {
                let args_preview: String = tc.arguments.chars().take(120).collect();
                tracing::info!("Remote tool call: {}({})", tc.name, args_preview);
                let result = commands::dispatch_tool(tc, &file_ops, &shell).await;
                let result_preview: String = result.chars().take(300).collect();
                tracing::info!("Remote tool result: {}", result_preview);

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
            continue;
        }

        return Ok(resp.message.content);
    }

    Ok("已达到最大工具调用轮次，操作停止。".to_string())
}

async fn handle_remote_control_command(
    app: &tauri::AppHandle,
    cfg: &config::AppConfig,
    cmd: &remote::webhook_server::WebhookCommand,
) -> String {
    let session_key = format!("{}:{}", cmd.source, cmd.user_id);
    let conv_state = app.state::<RemoteConversationState>();
    match cmd.command_type.as_str() {
        "status" => build_remote_reply("/status"),
        "chat" => {
            if cmd.text.trim().eq_ignore_ascii_case("/reset") {
                conv_state.clear(&session_key).await;
                return "已清空当前飞书会话上下文。".to_string();
            }

            let history = conv_state.get_history(&session_key).await;
            match run_remote_chat(cfg, &history, &cmd.text).await {
                Ok(answer) => {
                    conv_state.append_turn(&session_key, &cmd.text, &answer).await;
                    answer
                }
                Err(e) => format!("远程对话失败: {}", e),
            }
        }
        ,
        "task_create" => {
            let state = app.state::<commands::ApprovalState>();
            let task_text = cmd.text.trim();
            if task_text.is_empty() {
                return "用法：/task <任务描述>".to_string();
            }
            let approval = commands::create_remote_task_approval(app, &state, &cmd.user_id, task_text);
            format!(
                "🟡 任务已进入审批队列\nID: {}\n任务: {}\n\n发送 /approve {} 执行\n发送 /reject {} 拒绝",
                approval.id, task_text, approval.id, approval.id
            )
        }
        "approve" => {
            let approval_id = cmd.text.split_whitespace().next().unwrap_or("");
            if approval_id.is_empty() {
                "用法：/approve <审批ID>".to_string()
            } else {
                let state = app.state::<commands::ApprovalState>();
                match commands::approve_operation_internal(&state, approval_id) {
                    Ok(approval) => {
                        if approval.operation == "remote_task" {
                            let task_text = approval
                                .details
                                .get("task_text")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            if task_text.is_empty() {
                                return format!("审批 {} 已通过，但任务内容为空", approval_id);
                            }

                            let history = conv_state.get_history(&session_key).await;
                            match run_remote_chat(cfg, &history, &task_text).await {
                                Ok(answer) => {
                                    conv_state.append_turn(&session_key, &task_text, &answer).await;
                                    format!("✅ 审批已通过并执行任务\nID: {}\n\n{}", approval_id, answer)
                                }
                                Err(e) => format!("✅ 审批已通过，但任务执行失败\nID: {}\n错误: {}", approval_id, e),
                            }
                        } else {
                            format!("已处理审批：{}", approval_id)
                        }
                    }
                    Err(err) => format!("审批失败：{}", err),
                }
            }
        }
        "reject" => {
            let mut parts = cmd.text.splitn(2, ' ');
            let approval_id = parts.next().unwrap_or("").trim();
            let reject_reason = parts.next().unwrap_or("from_feishu").trim().to_string();
            if approval_id.is_empty() {
                "用法：/reject <审批ID>".to_string()
            } else {
                let state = app.state::<commands::ApprovalState>();
                match commands::reject_operation_internal(&state, approval_id) {
                    Ok(_) => format!("已拒绝审批：{}\n原因：{}", approval_id, reject_reason),
                    Err(err) => format!("拒绝失败：{}", err),
                }
            }
        }
        "task_cancel" => "已收到取消任务指令。当前版本将在后续接入任务队列后生效。".to_string(),
        _ => build_remote_reply(&cmd.text),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("auto_crab=info".parse().unwrap()))
        .with_target(false)
        .init();

    tracing::info!("Auto Crab starting...");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(commands::ApprovalState::default())
        .manage(RemoteConversationState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::chat_send,
            commands::chat_stream_start,
            commands::list_models,
            commands::get_audit_log,
            commands::approve_operation,
            commands::reject_operation,
            commands::list_pending_approvals,
            commands::store_credential,
            commands::delete_credential,
            commands::get_risk_level,
            commands::save_conversation,
            commands::load_conversation,
            commands::list_conversations,
            commands::delete_conversation,
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Initialize config + webhook server
            let handle_clone = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = config::ensure_config_dir(&handle_clone).await {
                    tracing::error!("Failed to initialize config directory: {}", e);
                }

                // Start webhook server if remote control is enabled
                match config::load_config(&handle_clone).await {
                    Ok(cfg) => {
                        if cfg.remote.enabled {
                            let (tx, mut rx) = tokio::sync::mpsc::channel(32);
                            let server = remote::webhook_server::WebhookServer::new(&cfg, tx);
                            if let Err(e) = server.start().await {
                                tracing::error!("Failed to start webhook server: {}", e);
                            } else {
                                tracing::info!("Webhook server started on port 18790");
                            }
                            let mut feishu_bot = cfg
                                .remote
                                .feishu
                                .as_ref()
                                .map(|c| remote::feishu::FeishuBot::new(c.clone()));
                            // Process incoming commands in background
                            let cfg_for_remote = cfg.clone();
                            tokio::spawn(async move {
                                while let Some(cmd) = rx.recv().await {
                                    tracing::info!(
                                        "Remote command from {}: {} [{}] -> {}",
                                        cmd.source, cmd.user_id, cmd.command_type, cmd.text
                                    );
                                    if cmd.source == "feishu" {
                                        if let Some(bot) = feishu_bot.as_mut() {
                                            let reply = handle_remote_control_command(&handle_clone, &cfg_for_remote, &cmd).await;
                                            if let Err(e) = bot.send_message(&cmd.user_id, &reply).await {
                                                tracing::warn!(
                                                    "Failed to reply Feishu message to {}: {}",
                                                    cmd.user_id, e
                                                );
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    }
                    Err(e) => tracing::warn!("Could not load config for webhook: {}", e),
                }
            });

            // System tray
            let show_item = MenuItemBuilder::with_id("show", "显示窗口").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出 Auto Crab").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .menu(&tray_menu)
                .tooltip("Auto Crab - AI Desktop Assistant")
                .on_menu_event(move |app, event| {
                    match event.id().as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            tracing::info!("System tray initialized");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Auto Crab");
}
