use crate::config::AppConfig;
use crate::remote::feishu::FeishuBot;
use crate::remote::protocol::CommandType;
use crate::remote::wechat_work::WechatWorkBot;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

pub struct WebhookServer {
    port: u16,
    feishu: Option<Arc<Mutex<FeishuBot>>>,
    wechat_work: Option<Arc<Mutex<WechatWorkBot>>>,
    command_tx: mpsc::Sender<WebhookCommand>,
}

#[derive(Debug, Clone)]
pub struct WebhookCommand {
    pub source: String,
    pub user_id: String,
    pub command_type: String,
    pub text: String,
}

impl WebhookServer {
    pub fn new(config: &AppConfig, command_tx: mpsc::Sender<WebhookCommand>) -> Self {
        let feishu = config
            .remote
            .feishu
            .as_ref()
            .map(|c| Arc::new(Mutex::new(FeishuBot::new(c.clone()))));

        let wechat_work = config
            .remote
            .wechat_work
            .as_ref()
            .map(|c| Arc::new(Mutex::new(WechatWorkBot::new(c.clone()))));

        Self {
            port: 18790,
            feishu,
            wechat_work,
            command_tx,
        }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let addr = format!("127.0.0.1:{}", self.port);
        let listener = TcpListener::bind(&addr).await?;
        tracing::info!("Webhook server listening on {}", addr);

        let feishu = self.feishu.clone();
        let wechat_work = self.wechat_work.clone();
        let tx = self.command_tx.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut stream, _)) => {
                        let feishu = feishu.clone();
                        let wechat_work = wechat_work.clone();
                        let tx = tx.clone();

                        tokio::spawn(async move {
                            let mut buf = vec![0u8; 65536];
                            let n = stream.read(&mut buf).await.unwrap_or(0);
                            let request = String::from_utf8_lossy(&buf[..n]).to_string();

                            let response = handle_request(&request, &feishu, &wechat_work, &tx).await;

                            let http_response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                response.len(),
                                response,
                            );
                            let _ = stream.write_all(http_response.as_bytes()).await;
                        });
                    }
                    Err(e) => {
                        tracing::error!("Webhook accept error: {}", e);
                    }
                }
            }
        });

        Ok(())
    }
}

fn parse_request_line(raw: &str) -> (&str, &str) {
    let first_line = raw.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("POST");
    let path = parts.next().unwrap_or("/");
    (method, path)
}

fn parse_query_param<'a>(path: &'a str, key: &str) -> Option<&'a str> {
    let query = path.split('?').nth(1)?;
    for pair in query.split('&') {
        let mut kv = pair.splitn(2, '=');
        if kv.next() == Some(key) {
            return kv.next();
        }
    }
    None
}

async fn handle_request(
    raw: &str,
    feishu: &Option<Arc<Mutex<FeishuBot>>>,
    wechat_work: &Option<Arc<Mutex<WechatWorkBot>>>,
    tx: &mpsc::Sender<WebhookCommand>,
) -> String {
    let (method, path) = parse_request_line(raw);
    let base_path = path.split('?').next().unwrap_or(path);

    match base_path {
        "/webhook/wechat_work" | "/webhook/wechat" => {
            handle_wechat_work(raw, method, path, wechat_work, tx).await
        }
        _ => {
            // Default: Feishu webhook (backward compatible)
            handle_feishu(raw, feishu, tx).await
        }
    }
}

async fn handle_feishu(
    raw: &str,
    feishu: &Option<Arc<Mutex<FeishuBot>>>,
    tx: &mpsc::Sender<WebhookCommand>,
) -> String {
    let body = raw.split("\r\n\r\n").nth(1).unwrap_or("{}");

    let json: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return r#"{"error":"invalid json"}"#.to_string(),
    };

    // Feishu URL verification challenge
    if let Some(challenge) = json.get("challenge").and_then(|c| c.as_str()) {
        tracing::info!("Feishu verification challenge received");
        return format!(r#"{{"challenge":"{}"}}"#, challenge);
    }

    // Feishu event callback
    if let Some(event) = json.get("event") {
        if let Some(ref feishu_bot) = feishu {
            let bot = feishu_bot.lock().await;
            if let Some(cmd) = bot.parse_event(event) {
                let _ = tx
                    .send(WebhookCommand {
                        source: "feishu".into(),
                        user_id: cmd.user_id,
                        command_type: match cmd.command_type {
                            CommandType::Chat => "chat".into(),
                            CommandType::StatusQuery => "status".into(),
                            CommandType::TaskCreate => "task_create".into(),
                            CommandType::TaskCancel => "task_cancel".into(),
                            CommandType::ApproveAction => "approve".into(),
                            CommandType::RejectAction => "reject".into(),
                        },
                        text: cmd.content,
                    })
                    .await;
                tracing::info!("Feishu command received: {:?}", cmd.command_type);
            }
        }
    }

    r#"{"ok":true}"#.to_string()
}

async fn handle_wechat_work(
    raw: &str,
    method: &str,
    path: &str,
    wechat_work: &Option<Arc<Mutex<WechatWorkBot>>>,
    tx: &mpsc::Sender<WebhookCommand>,
) -> String {
    let bot = match wechat_work {
        Some(b) => b,
        None => {
            tracing::warn!("WeChat Work webhook received but not configured");
            return r#"{"error":"wechat_work not configured"}"#.to_string();
        }
    };

    let msg_signature = parse_query_param(path, "msg_signature").unwrap_or("");
    let timestamp = parse_query_param(path, "timestamp").unwrap_or("");
    let nonce = parse_query_param(path, "nonce").unwrap_or("");

    // GET = URL verification
    if method == "GET" {
        let echostr = parse_query_param(path, "echostr").unwrap_or("");
        if echostr.is_empty() {
            return r#"{"error":"missing echostr"}"#.to_string();
        }

        let bot = bot.lock().await;
        match bot.verify_url(msg_signature, timestamp, nonce, echostr) {
            Some(reply) => {
                tracing::info!("WeChat Work URL verification successful");
                reply
            }
            None => {
                tracing::warn!("WeChat Work URL verification failed");
                r#"{"error":"verification failed"}"#.to_string()
            }
        }
    }
    // POST = message callback
    else {
        let body = raw.split("\r\n\r\n").nth(1).unwrap_or("");
        if body.is_empty() {
            return "success".to_string();
        }

        let bot = bot.lock().await;
        if let Some(cmd) = bot.parse_callback(body, msg_signature, timestamp, nonce) {
            let _ = tx
                .send(WebhookCommand {
                    source: "wechat_work".into(),
                    user_id: cmd.user_id,
                    command_type: match cmd.command_type {
                        CommandType::Chat => "chat".into(),
                        CommandType::StatusQuery => "status".into(),
                        CommandType::TaskCreate => "task_create".into(),
                        CommandType::TaskCancel => "task_cancel".into(),
                        CommandType::ApproveAction => "approve".into(),
                        CommandType::RejectAction => "reject".into(),
                    },
                    text: cmd.content,
                })
                .await;
            tracing::info!("WeChat Work command received: {:?}", cmd.command_type);
        }

        "success".to_string()
    }
}
