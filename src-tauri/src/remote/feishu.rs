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

        let resp: TokenResponse = self
            .client
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

        let token = resp
            .tenant_access_token
            .ok_or_else(|| anyhow::anyhow!("no token in response"))?;
        let expires_in = resp.expire.unwrap_or(7200);

        self.access_token = Some(token.clone());
        self.token_expires_at =
            Some(chrono::Utc::now() + chrono::Duration::seconds(expires_in - 300));

        Ok(token)
    }

    pub async fn send_message(&mut self, user_open_id: &str, text: &str) -> Result<()> {
        let token = self.ensure_token().await?;
        let md_text = markdown_to_feishu_md(text);

        // Use post message with md tag for native Markdown rendering
        let post_content = serde_json::json!({
            "zh_cn": {
                "content": [[{
                    "tag": "md",
                    "text": md_text
                }]]
            }
        });

        let body = serde_json::json!({
            "receive_id": user_open_id,
            "msg_type": "post",
            "content": post_content.to_string(),
        });

        let resp = self
            .client
            .post("https://open.feishu.cn/open-apis/im/v1/messages?receive_id_type=open_id")
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let resp_body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Feishu send error: {}", resp_body);
        }

        Ok(())
    }

    pub fn parse_event(&self, event_json: &serde_json::Value) -> Option<RemoteCommand> {
        let event: MessageEvent = serde_json::from_value(event_json.clone()).ok()?;

        // Prefer open_id because send_message uses receive_id_type=open_id.
        let user_id = event
            .sender
            .and_then(|s| s.sender_id)
            .and_then(|ids| ids.open_id.or(ids.user_id))?;

        if !validate_remote_user(
            &user_id,
            &self.config.allowed_user_ids,
            &RemoteSource::Feishu,
        ) {
            tracing::warn!(
                "Rejected Feishu message from unauthorized user: {}",
                user_id
            );
            return None;
        }

        let content = event.message.and_then(|m| m.content).unwrap_or_default();

        let text: String = serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(String::from))
            .unwrap_or(content);

        Some(parse_command(&text, &user_id, RemoteSource::Feishu))
    }
}

/// Convert standard Markdown to Feishu md-tag compatible Markdown.
///
/// Feishu's post message `md` tag natively supports:
/// - `**bold**`, `*italic*`, `***bold italic***`
/// - `~~lineThrough~~`
/// - `[text](url)` links
/// - `> quote`
/// - `- unordered list`, `1. ordered list`
/// - ` ```code blocks``` `
/// - ` --- ` horizontal rule
///
/// Only minimal conversion needed:
/// - `## heading` → `**heading**` (Feishu md doesn't support # headings)
fn markdown_to_feishu_md(md: &str) -> String {
    let mut out = String::with_capacity(md.len());

    for line in md.lines() {
        let trimmed = line.trim();

        // Convert headings to bold (Feishu md tag doesn't render # headings)
        if let Some(rest) = trimmed.strip_prefix("### ") {
            out.push_str(&format!("**{}**", rest.trim().replace("**", "")));
            out.push('\n');
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            out.push_str(&format!("**{}**", rest.trim().replace("**", "")));
            out.push('\n');
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            out.push_str(&format!("**{}**", rest.trim().replace("**", "")));
            out.push('\n');
            continue;
        }

        // Everything else passes through as-is (Feishu md supports it natively)
        out.push_str(line);
        out.push('\n');
    }

    while out.ends_with("\n\n\n") {
        out.pop();
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_md_preserves_bold() {
        let md = "**结论**：BTC 价格为 **$66,161.68**";
        let result = markdown_to_feishu_md(md);
        assert!(result.contains("**结论**"), "Should preserve bold: {}", result);
        assert!(result.contains("**$66,161.68**"), "Should preserve price: {}", result);
    }

    #[test]
    fn test_md_preserves_list() {
        let result = markdown_to_feishu_md("- 24小时最高：$68,955");
        assert!(result.contains("- 24小时最高"), "Should preserve list: {}", result);
    }

    #[test]
    fn test_md_heading_to_bold() {
        assert_eq!(markdown_to_feishu_md("## 简要分析"), "**简要分析**");
    }

    #[test]
    fn test_md_preserves_link() {
        let result = markdown_to_feishu_md("查看 [Binance](https://binance.com)");
        assert!(result.contains("[Binance](https://binance.com)"), "Should preserve: {}", result);
    }

    #[test]
    fn test_md_preserves_quote() {
        let result = markdown_to_feishu_md("> 数据来源：Binance API");
        assert!(result.contains("> 数据来源"), "Should preserve quote: {}", result);
    }

    #[test]
    fn test_md_preserves_strike() {
        let result = markdown_to_feishu_md("~~已过期~~");
        assert!(result.contains("~~已过期~~"), "Should preserve: {}", result);
    }

    #[test]
    fn test_md_plain_text() {
        let plain = "这是一段普通文本";
        assert_eq!(markdown_to_feishu_md(plain), plain);
    }

    #[test]
    fn test_md_full_btc() {
        let md = "\
**结论**：价格为 **$66,161.68**

**关键数据**：
- 24小时最高：$68,955.53
- 24小时最低：$65,548.25

> 数据来源：Binance API";

        let result = markdown_to_feishu_md(md);
        assert!(result.contains("**结论**"), "Bold preserved");
        assert!(result.contains("- 24小时最高"), "List preserved");
        assert!(result.contains("> 数据来源"), "Quote preserved");
    }
}
