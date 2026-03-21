use super::protocol::*;
use crate::config::FeishuConfig;
use crate::security::credentials::CredentialStore;
use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct FeishuBot {
    client: Client,
    config: FeishuConfig,
    access_token: Option<String>,
    token_expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize)]
struct TokenRequest {
    app_id: String,
    app_secret: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    code: i32,
    msg: String,
    tenant_access_token: Option<String>,
    expire: Option<i64>,
}

#[derive(Deserialize)]
struct MessageEvent {
    sender: Option<MessageSender>,
    message: Option<MessageContent>,
}

#[derive(Deserialize)]
struct MessageSender {
    sender_id: Option<SenderIds>,
}

#[derive(Deserialize)]
struct SenderIds {
    user_id: Option<String>,
    open_id: Option<String>,
}

#[derive(Deserialize)]
struct MessageContent {
    content: Option<String>,
    message_type: Option<String>,
}

impl FeishuBot {
    pub fn new(config: FeishuConfig) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            config,
            access_token: None,
            token_expires_at: None,
        }
    }

    async fn ensure_token(&mut self) -> Result<String> {
        if let (Some(ref token), Some(expires)) = (&self.access_token, self.token_expires_at) {
            if chrono::Utc::now() < expires {
                return Ok(token.clone());
            }
        }

        let secret = CredentialStore::resolve_ref(&self.config.app_secret_ref)?;

        let resp: TokenResponse = self.client
            .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
            .json(&TokenRequest {
                app_id: self.config.app_id.clone(),
                app_secret: secret,
            })
            .send()
            .await?
            .json()
            .await?;

        if resp.code != 0 {
            anyhow::bail!("Feishu token error: {}", resp.msg);
        }

        let token = resp.tenant_access_token
            .ok_or_else(|| anyhow::anyhow!("no token in response"))?;
        let expires_in = resp.expire.unwrap_or(7200);

        self.access_token = Some(token.clone());
        self.token_expires_at = Some(
            chrono::Utc::now() + chrono::Duration::seconds(expires_in - 300)
        );

        Ok(token)
    }

    pub async fn send_message(&mut self, user_open_id: &str, text: &str) -> Result<()> {
        let token = self.ensure_token().await?;

        let body = serde_json::json!({
            "receive_id": user_open_id,
            "msg_type": "text",
            "content": serde_json::json!({"text": text}).to_string(),
        });

        let resp = self.client
            .post("https://open.feishu.cn/open-apis/im/v1/messages?receive_id_type=open_id")
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Feishu send error: {}", body);
        }

        Ok(())
    }

    pub fn parse_event(&self, event_json: &serde_json::Value) -> Option<RemoteCommand> {
        let event: MessageEvent = serde_json::from_value(event_json.clone()).ok()?;

        // Prefer open_id because send_message uses receive_id_type=open_id.
        let user_id = event.sender
            .and_then(|s| s.sender_id)
            .and_then(|ids| ids.open_id.or(ids.user_id))?;

        if !validate_remote_user(&user_id, &self.config.allowed_user_ids, &RemoteSource::Feishu) {
            tracing::warn!("Rejected Feishu message from unauthorized user: {}", user_id);
            return None;
        }

        let content = event.message
            .and_then(|m| m.content)
            .unwrap_or_default();

        let text: String = serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(String::from))
            .unwrap_or(content);

        Some(parse_command(&text, &user_id, RemoteSource::Feishu))
    }
}
