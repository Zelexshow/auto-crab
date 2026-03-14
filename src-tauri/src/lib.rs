mod config;
mod security;
mod models;
mod tools;
mod remote;
mod commands;

use tracing_subscriber::{fmt, EnvFilter};

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
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = config::ensure_config_dir(&app_handle).await {
                    tracing::error!("Failed to initialize config directory: {}", e);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Auto Crab");
}
