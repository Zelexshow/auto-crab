use super::protocol::*;
use crate::config::WechatWorkConfig;
use crate::security::credentials::CredentialStore;
use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;

pub struct WechatWorkBot {
    client: Client,
    config: WechatWorkConfig,
    access_token: Option<String>,
    token_expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Deserialize)]
struct TokenResponse {
    errcode: i32,
    errmsg: String,
    access_token: Option<String>,
    expires_in: Option<i64>,
}

impl WechatWorkBot {
    pub fn new(config: WechatWorkConfig) -> Self {
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

        let secret = CredentialStore::resolve_ref(&self.config.secret_ref)?;

        let url = format!(
            "https://qyapi.weixin.qq.com/cgi-bin/gettoken?corpid={}&corpsecret={}",
            self.config.corp_id, secret
        );

        let resp: TokenResponse = self.client.get(&url).send().await?.json().await?;

        if resp.errcode != 0 {
            anyhow::bail!("WeChat Work token error: {}", resp.errmsg);
        }

        let token = resp
            .access_token
            .ok_or_else(|| anyhow::anyhow!("no token in response"))?;
        let expires_in = resp.expires_in.unwrap_or(7200);

        self.access_token = Some(token.clone());
        self.token_expires_at =
            Some(chrono::Utc::now() + chrono::Duration::seconds(expires_in - 300));

        Ok(token)
    }

    pub async fn send_message(&mut self, user_id: &str, text: &str) -> Result<()> {
        let token = self.ensure_token().await?;

        let body = serde_json::json!({
            "touser": user_id,
            "msgtype": "text",
            "agentid": self.config.agent_id.parse::<i64>().unwrap_or(0),
            "text": {
                "content": text,
            },
        });

        let url = format!(
            "https://qyapi.weixin.qq.com/cgi-bin/message/send?access_token={}",
            token
        );

        let resp = self.client.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("WeChat Work send error: {}", body);
        }

        Ok(())
    }

    pub fn parse_message(&self, xml_or_json: &str, user_id: &str) -> Option<RemoteCommand> {
        if !validate_remote_user(
            user_id,
            &self.config.allowed_user_ids,
            &RemoteSource::WechatWork,
        ) {
            tracing::warn!(
                "Rejected WeChat Work message from unauthorized user: {}",
                user_id
            );
            return None;
        }

        Some(parse_command(
            xml_or_json,
            user_id,
            RemoteSource::WechatWork,
        ))
    }
}
