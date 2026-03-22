use crate::config::AppConfig;
use crate::remote::feishu::FeishuBot;
use crate::remote::protocol::CommandType;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

/// Minimal HTTP server to receive Feishu/WeChat webhook events.
/// Runs on a local port, meant to be exposed via tunnel (ngrok/cloudflare).
pub struct WebhookServer {
    port: u16,
    feishu: Option<Arc<Mutex<FeishuBot>>>,
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

        Self {
            port: 18790,
            feishu,
            command_tx,
        }
    }

    /// Start the webhook HTTP server.
    /// This spawns a background task that listens for incoming webhooks.
    pub async fn start(&self) -> anyhow::Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let addr = format!("127.0.0.1:{}", self.port);
        let listener = TcpListener::bind(&addr).await?;
        tracing::info!("Webhook server listening on {}", addr);

        let feishu = self.feishu.clone();
        let tx = self.command_tx.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut stream, _)) => {
                        let feishu = feishu.clone();
                        let tx = tx.clone();

                        tokio::spawn(async move {
                            let mut buf = vec![0u8; 65536];
                            let n = stream.read(&mut buf).await.unwrap_or(0);
                            let request = String::from_utf8_lossy(&buf[..n]).to_string();

                            let response = handle_request(&request, &feishu, &tx).await;

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

async fn handle_request(
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
