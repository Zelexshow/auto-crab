mod commands;
mod config;
mod core;
mod models;
mod plugins;
mod remote;
mod security;
mod tools;

use std::collections::HashMap;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Manager,
};
use tokio::sync::Mutex;
use tracing_subscriber::{fmt, EnvFilter};

const REMOTE_HISTORY_MAX_MESSAGES: usize = 20;

#[derive(Default)]
struct MonitorState {
    tasks: Mutex<HashMap<String, MonitorTask>>,
}

struct MonitorTask {
    description: String,
    interval_secs: u64,
    cancel: tokio::sync::watch::Sender<bool>,
}

impl MonitorState {
    async fn add(&self, id: String, task: MonitorTask) {
        let mut tasks = self.tasks.lock().await;
        if let Some(old) = tasks.remove(&id) {
            let _ = old.cancel.send(true);
        }
        tasks.insert(id, task);
    }

    async fn remove(&self, id: &str) -> bool {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.remove(id) {
            let _ = task.cancel.send(true);
            true
        } else {
            false
        }
    }

    async fn list(&self) -> Vec<(String, String, u64)> {
        let tasks = self.tasks.lock().await;
        tasks
            .iter()
            .map(|(id, t)| (id.clone(), t.description.clone(), t.interval_secs))
            .collect()
    }
}

#[derive(Default)]
struct RemoteConversationState {
    sessions: Mutex<HashMap<String, Vec<crate::models::provider::ChatMessage>>>,
    active_sessions: Mutex<HashMap<String, String>>,
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

    async fn set_active_session(&self, user_id: &str, session_key: &str) {
        let mut active = self.active_sessions.lock().await;
        active.insert(user_id.to_string(), session_key.to_string());
    }

    async fn get_active_session(&self, user_id: &str) -> String {
        let active = self.active_sessions.lock().await;
        active.get(user_id).cloned().unwrap_or_default()
    }

    async fn list_sessions(&self, user_id: &str) -> Vec<String> {
        let sessions = self.sessions.lock().await;
        let active = self.active_sessions.lock().await;
        let prefix = format!("feishu:{}", user_id);
        let mut all: std::collections::HashSet<String> = sessions
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();
        if let Some(active_key) = active.get(user_id) {
            if active_key.starts_with(&prefix) {
                all.insert(active_key.clone());
            }
        }
        let mut list: Vec<String> = all.into_iter().collect();
        list.sort();
        list
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
    audit: Option<&std::sync::Arc<security::audit::AuditLogger>>,
) -> anyhow::Result<String> {
    use crate::models::provider::*;
    use crate::tools::file_ops::FileOps;
    use crate::tools::shell::ShellExecutor;

    let router = crate::models::ModelRouter::from_config(cfg)?;
    let provider = router
        .get_primary()
        .ok_or_else(|| anyhow::anyhow!("No model provider configured"))?;

    let mut messages = vec![ChatMessage {
        role: MessageRole::System,
        content: cfg.agent.system_prompt.clone(),
        name: None,
        tool_calls: None,
        tool_call_id: None,
    }];
    messages.extend(history.iter().cloned());
    messages.push(ChatMessage {
        role: MessageRole::User,
        content: user_input.to_string(),
        name: None,
        tool_calls: None,
        tool_call_id: None,
    });

    let file_roots: Vec<std::path::PathBuf> = cfg
        .tools
        .file_access
        .iter()
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
            tools: if use_tools {
                Some(tool_defs.clone())
            } else {
                None
            },
            temperature: 0.7,
            max_tokens: None,
        };

        let resp = provider.chat(req).await?;
        let tool_calls = resp.message.tool_calls.clone().unwrap_or_default();

        if !tool_calls.is_empty() {
            for tc in &tool_calls {
                let args_preview: String = tc.arguments.chars().take(120).collect();
                tracing::info!("Remote tool call: {}({})", tc.name, args_preview);
                let mut result = commands::dispatch_tool_with_audit(
                    tc,
                    &file_ops,
                    &shell,
                    audit,
                    security::audit::AuditSource::Feishu,
                )
                .await;

                if result.starts_with("__ANALYZE_SCREEN__:") {
                    let rest = &result["__ANALYZE_SCREEN__:".len()..];
                    if let Some(sep) = rest.find("::") {
                        let img_path = &rest[..sep];
                        let question = &rest[sep + 2..];
                        tracing::info!("Analyzing screenshot with VL model: {}", img_path);
                        match commands::analyze_screenshot(cfg, img_path).await {
                            Ok(analysis) => {
                                result = format!(
                                    "屏幕截图分析结果（问题：{}）：\n\n{}",
                                    question, analysis
                                );
                            }
                            Err(e) => {
                                result =
                                    format!("截图已保存到 {}，但视觉分析失败: {}", img_path, e);
                            }
                        }
                    }
                }

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
    let audit = app.try_state::<std::sync::Arc<security::audit::AuditLogger>>();
    let audit_ref = audit.as_deref();
    match cmd.command_type.as_str() {
        "status" => {
            let sub = cmd.text.trim();
            if sub == "models" {
                let mut lines = vec!["🦀 模型配置:".to_string()];
                if let Some(ref m) = cfg.models.primary {
                    lines.push(format!("  主模型: {} ({})", m.model, m.provider));
                }
                if let Some(ref m) = cfg.models.vision {
                    lines.push(format!("  视觉: {} ({})", m.model, m.provider));
                }
                if let Some(ref m) = cfg.models.coding {
                    lines.push(format!("  编码: {} ({})", m.model, m.provider));
                }
                if let Some(ref m) = cfg.models.fallback {
                    lines.push(format!("  回退: {} ({})", m.model, m.provider));
                }
                lines.join("\n")
            } else {
                format!(
                    "🦀 Auto Crab 在线\n时间: {}\n主模型: {}\n视觉: {}\n工具: {}\n会话: /sessions 查看\n\n/status models — 查看详细模型配置",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    cfg.models.primary.as_ref().map(|m| m.model.as_str()).unwrap_or("未配置"),
                    cfg.models.vision.as_ref().map(|m| m.model.as_str()).unwrap_or("未配置"),
                    if cfg.tools.shell_enabled { "Shell+文件+截图+鼠标键盘" } else { "受限" },
                )
            }
        }
        "chat" => {
            let text = cmd.text.trim();

            if text.eq_ignore_ascii_case("/reset") {
                conv_state.clear(&session_key).await;
                return "已清空当前会话上下文。".to_string();
            }

            if text.eq_ignore_ascii_case("/undo") {
                let data_dir = directories::ProjectDirs::from("com", "zelex", "auto-crab")
                    .map(|d| d.data_dir().to_path_buf())
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let snapshots = crate::core::snapshots::SnapshotStore::new(data_dir);
                match snapshots.list(1).await {
                    Ok(list) if !list.is_empty() => {
                        let snap = &list[0];
                        match snapshots.restore(&snap.id).await {
                            Ok(path) => return format!("已撤回文件修改: {}", path),
                            Err(e) => return format!("撤回失败: {}", e),
                        }
                    }
                    _ => return "没有可撤回的操作。".to_string(),
                }
            }

            if text.starts_with("/monitor stop") {
                let monitor_id = text.strip_prefix("/monitor stop").unwrap_or("").trim();
                let monitor = app.state::<MonitorState>();
                if monitor_id.is_empty() {
                    return "用法: /monitor stop <ID>".to_string();
                }
                if monitor.remove(monitor_id).await {
                    return format!("已停止监控: {}", monitor_id);
                } else {
                    return format!("未找到监控任务: {}", monitor_id);
                }
            }

            if text.eq_ignore_ascii_case("/monitors") {
                let monitor = app.state::<MonitorState>();
                let list = monitor.list().await;
                if list.is_empty() {
                    return "当前没有活跃的监控任务。\n用法: /monitor <间隔秒数> <监控内容描述>"
                        .to_string();
                }
                let lines: Vec<String> = list
                    .iter()
                    .map(|(id, desc, interval)| format!("  • [{}] 每{}秒 — {}", id, interval, desc))
                    .collect();
                return format!(
                    "活跃监控任务:\n{}\n\n/monitor stop <ID> 停止",
                    lines.join("\n")
                );
            }

            if text.starts_with("/monitor ") {
                let rest = text.strip_prefix("/monitor ").unwrap_or("").trim();
                let mut parts = rest.splitn(2, ' ');
                let interval_str = parts.next().unwrap_or("60");
                let description = parts.next().unwrap_or("监控屏幕变化").to_string();
                let interval: u64 = interval_str.parse().unwrap_or(60).max(10);

                let monitor_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
                let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);

                let task = MonitorTask {
                    description: description.clone(),
                    interval_secs: interval,
                    cancel: cancel_tx,
                };

                let monitor = app.state::<MonitorState>();
                monitor.add(monitor_id.clone(), task).await;

                let cfg_clone = cfg.clone();
                let feishu_config = cfg.remote.feishu.clone();
                let user_id = cmd.user_id.clone();
                let mid = monitor_id.clone();
                let desc = description.clone();

                tokio::spawn(async move {
                    let mut history: Vec<(String, String)> = Vec::new(); // (timestamp, analysis)
                    let max_history = 5;
                    let mut round = 0u32;

                    loop {
                        tokio::select! {
                            _ = tokio::time::sleep(std::time::Duration::from_secs(interval)) => {},
                            _ = cancel_rx.changed() => {
                                tracing::info!("Monitor {} cancelled", mid);
                                break;
                            }
                        }
                        if *cancel_rx.borrow() {
                            break;
                        }
                        round += 1;

                        tracing::info!("Monitor {} round {}: {}", mid, round, desc);
                        let tmp = format!(
                            "{}\\AppData\\Local\\Temp\\auto-crab-monitor-{}.png",
                            std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into()),
                            mid,
                        );
                        match commands::take_screenshot_sync(&tmp) {
                            Ok(_) => {
                                let history_context = if history.is_empty() {
                                    "这是首次监控分析，没有历史数据。".to_string()
                                } else {
                                    let entries: Vec<String> = history
                                        .iter()
                                        .map(|(ts, a)| {
                                            format!(
                                                "[{}] {}",
                                                ts,
                                                a.chars().take(150).collect::<String>()
                                            )
                                        })
                                        .collect();
                                    format!(
                                        "以下是最近{}次监控记录，请结合历史走势进行对比分析：\n{}",
                                        entries.len(),
                                        entries.join("\n")
                                    )
                                };

                                let monitor_prompt = format!(
"你是一位资深投资交易分析师。用户正在持续监控：「{desc}」（第{round}轮，每{interval}秒一次）。

{history_context}

请根据当前截图，只关注与监控主题相关的内容（忽略浏览器UI、任务栏等），输出：
1. 当前关键数据（价格/涨跌幅/成交量等）
2. 与上次对比的变化（价格变动方向、幅度）
3. K线形态和趋势判断
4. 操作建议（持有/加仓/减仓/观望，附理由）
5. 风险预警（如有异常波动或关键支撑位/阻力位突破）

如果不是交易界面，请简洁描述与监控主题相关的变化。
限300字以内。",
                                );
                                match commands::analyze_screenshot_with_prompt(
                                    &cfg_clone,
                                    &tmp,
                                    &monitor_prompt,
                                )
                                .await
                                {
                                    Ok(analysis) => {
                                        let now =
                                            chrono::Local::now().format("%H:%M:%S").to_string();
                                        let msg = format!(
                                            "🔍 [{}] {} (第{}轮)\n\n{}",
                                            mid, desc, round, analysis
                                        );

                                        history.push((now, analysis));
                                        if history.len() > max_history {
                                            history.remove(0);
                                        }

                                        if let Some(ref fc) = feishu_config {
                                            let mut bot =
                                                crate::remote::feishu::FeishuBot::new(fc.clone());
                                            let _ = bot.send_message(&user_id, &msg).await;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("Monitor {} analysis failed: {}", mid, e);
                                    }
                                }
                            }
                            Err(e) => tracing::warn!("Monitor {} screenshot failed: {}", mid, e),
                        }
                    }
                });

                return format!(
                    "🔍 监控已启动\nID: {}\n间隔: {}秒\n内容: {}\n\n/monitor stop {} 停止\n/monitors 查看所有",
                    monitor_id, interval, description, monitor_id
                );
            }

            if text.eq_ignore_ascii_case("/sessions") {
                let list = conv_state.list_sessions(&cmd.user_id).await;
                if list.is_empty() {
                    return "当前没有活跃会话。发送任意消息开始默认会话。".to_string();
                }
                let active = conv_state.get_active_session(&cmd.user_id).await;
                let lines: Vec<String> = list
                    .iter()
                    .map(|s| {
                        let name = s.rsplit(':').next().unwrap_or(s);
                        if *s == active {
                            format!("  ▶ {} (当前)", name)
                        } else {
                            format!("  • {}", name)
                        }
                    })
                    .collect();
                return format!("活跃会话列表:\n{}", lines.join("\n"));
            }

            if text.starts_with("/session ") {
                let session_name = text.strip_prefix("/session ").unwrap_or("").trim();
                if session_name.is_empty() {
                    return "用法: /session <名称> — 切换到指定会话\n/sessions — 查看所有会话\n/reset — 清空当前会话".to_string();
                }
                let new_key = format!("{}:{}:{}", cmd.source, cmd.user_id, session_name);
                conv_state.set_active_session(&cmd.user_id, &new_key).await;
                return format!("已切换到会话「{}」", session_name);
            }

            let active_key = conv_state.get_active_session(&cmd.user_id).await;
            let final_key = if active_key.is_empty() {
                session_key.clone()
            } else {
                active_key
            };

            let history = conv_state.get_history(&final_key).await;
            match run_remote_chat(cfg, &history, text, audit_ref).await {
                Ok(answer) => {
                    conv_state.append_turn(&final_key, text, &answer).await;
                    answer
                }
                Err(e) => format!("远程对话失败: {}", e),
            }
        }
        "task_create" => {
            let state = app.state::<commands::ApprovalState>();
            let task_text = cmd.text.trim();
            if task_text.is_empty() {
                return "用法：/task <任务描述>".to_string();
            }
            let approval =
                commands::create_remote_task_approval(app, &state, &cmd.user_id, task_text);
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
                            match run_remote_chat(cfg, &history, &task_text, audit_ref).await {
                                Ok(answer) => {
                                    conv_state
                                        .append_turn(&session_key, &task_text, &answer)
                                        .await;
                                    format!(
                                        "✅ 审批已通过并执行任务\nID: {}\n\n{}",
                                        approval_id, answer
                                    )
                                }
                                Err(e) => format!(
                                    "✅ 审批已通过，但任务执行失败\nID: {}\n错误: {}",
                                    approval_id, e
                                ),
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
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("auto_crab=info".parse().unwrap()),
        )
        .with_target(false)
        .init();

    tracing::info!("Auto Crab starting...");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(commands::ApprovalState::default())
        .manage(RemoteConversationState::default())
        .manage(MonitorState::default())
        .manage({
            let data_dir = directories::ProjectDirs::from("com", "zelex", "auto-crab")
                .map(|d| d.data_dir().to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            std::sync::Arc::new(security::audit::AuditLogger::new(data_dir))
        })
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
            commands::check_credentials,
            commands::get_credential_preview,
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
                                        cmd.source,
                                        cmd.user_id,
                                        cmd.command_type,
                                        cmd.text
                                    );
                                    if cmd.source == "feishu" {
                                        if let Some(bot) = feishu_bot.as_mut() {
                                            let reply = handle_remote_control_command(
                                                &handle_clone,
                                                &cfg_for_remote,
                                                &cmd,
                                            )
                                            .await;
                                            if let Err(e) =
                                                bot.send_message(&cmd.user_id, &reply).await
                                            {
                                                tracing::warn!(
                                                    "Failed to reply Feishu message to {}: {}",
                                                    cmd.user_id,
                                                    e
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
                .on_menu_event(move |app, event| match event.id().as_ref() {
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
