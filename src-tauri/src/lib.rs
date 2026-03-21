mod config;
mod core;
mod security;
mod models;
mod tools;
mod remote;
mod plugins;
mod commands;

use tauri::{
    Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};
use tracing_subscriber::{fmt, EnvFilter};

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
    user_input: &str,
) -> anyhow::Result<String> {
    let router = crate::models::ModelRouter::from_config(cfg)?;
    let req = crate::models::provider::ChatRequest {
        messages: vec![
            crate::models::provider::ChatMessage {
                role: crate::models::provider::MessageRole::System,
                content: cfg.agent.system_prompt.clone(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            crate::models::provider::ChatMessage {
                role: crate::models::provider::MessageRole::User,
                content: user_input.to_string(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ],
        tools: None,
        temperature: 0.7,
        max_tokens: None,
    };
    let resp = router.chat_with_fallback(req).await?;
    Ok(resp.message.content)
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
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::chat_send,
            commands::chat_stream_start,
            commands::list_models,
            commands::get_audit_log,
            commands::approve_operation,
            commands::reject_operation,
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
                                            let reply = match cmd.command_type.as_str() {
                                                "status" => build_remote_reply("/status"),
                                                "chat" => match run_remote_chat(&cfg_for_remote, &cmd.text).await {
                                                    Ok(answer) => answer,
                                                    Err(e) => format!("远程对话失败: {}", e),
                                                },
                                                _ => build_remote_reply(&cmd.text),
                                            };
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
