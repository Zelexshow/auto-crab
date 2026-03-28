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

    /// Verify the URL callback signature from WeChat Work.
    /// signature = SHA1(sort(token, timestamp, nonce))
    pub fn verify_url(&self, msg_signature: &str, timestamp: &str, nonce: &str, echostr: &str) -> Option<String> {
        let signature = compute_signature(&self.config.token, timestamp, nonce, "");
        if signature != msg_signature {
            tracing::warn!("WeChat Work URL verification signature mismatch");
            return None;
        }

        if self.config.encoding_aes_key.is_empty() {
            return Some(echostr.to_string());
        }

        match decrypt_msg(&self.config.encoding_aes_key, echostr) {
            Ok((msg, _corp_id)) => Some(msg),
            Err(e) => {
                tracing::warn!("WeChat Work echostr decrypt failed: {}", e);
                Some(echostr.to_string())
            }
        }
    }

    /// Parse a callback message (POST body is XML, possibly encrypted).
    pub fn parse_callback(
        &self,
        body: &str,
        msg_signature: &str,
        timestamp: &str,
        nonce: &str,
    ) -> Option<RemoteCommand> {
        let encrypt_content = extract_xml_field(body, "Encrypt");

        let content_text = if let Some(ref encrypted) = encrypt_content {
            let expected_sig = compute_signature(&self.config.token, timestamp, nonce, encrypted);
            if expected_sig != msg_signature {
                tracing::warn!("WeChat Work message signature mismatch");
                return None;
            }

            match decrypt_msg(&self.config.encoding_aes_key, encrypted) {
                Ok((plaintext, corp_id)) => {
                    if corp_id != self.config.corp_id {
                        tracing::warn!("WeChat Work corp_id mismatch: expected {}, got {}", self.config.corp_id, corp_id);
                    }
                    plaintext
                }
                Err(e) => {
                    tracing::error!("WeChat Work decrypt failed: {}", e);
                    return None;
                }
            }
        } else {
            body.to_string()
        };

        let msg_type = extract_xml_field(&content_text, "MsgType").unwrap_or_default();
        if msg_type != "text" {
            tracing::info!("Ignoring WeChat Work message type: {}", msg_type);
            return None;
        }

        let from_user = extract_xml_field(&content_text, "FromUserName").unwrap_or_default();
        let text = extract_xml_field(&content_text, "Content").unwrap_or_default();

        if from_user.is_empty() || text.is_empty() {
            return None;
        }

        self.parse_message(&text, &from_user)
    }
}

fn extract_xml_field(xml: &str, field: &str) -> Option<String> {
    // Handle <Field><![CDATA[value]]></Field>
    let cdata_open = format!("<{}><![CDATA[", field);
    if let Some(start) = xml.find(&cdata_open) {
        let val_start = start + cdata_open.len();
        let close = format!("]]></{}>", field);
        if let Some(end) = xml[val_start..].find(&close) {
            return Some(xml[val_start..val_start + end].to_string());
        }
    }

    // Handle <Field>value</Field>
    let open_tag = format!("<{}>", field);
    let close_tag = format!("</{}>", field);
    if let Some(start) = xml.find(&open_tag) {
        let val_start = start + open_tag.len();
        if let Some(end) = xml[val_start..].find(&close_tag) {
            return Some(xml[val_start..val_start + end].to_string());
        }
    }
    None
}

/// SHA1(sort(token, timestamp, nonce, encrypt_msg))
fn compute_signature(token: &str, timestamp: &str, nonce: &str, encrypt_msg: &str) -> String {
    use sha1::Digest;

    let mut parts = vec![token, timestamp, nonce];
    if !encrypt_msg.is_empty() {
        parts.push(encrypt_msg);
    }
    parts.sort();
    let joined = parts.join("");

    let hash = sha1::Sha1::digest(joined.as_bytes());
    hex::encode(hash)
}

/// Decrypt an AES-256-CBC encrypted message from WeChat Work.
/// EncodingAESKey is Base64-encoded (43 chars), actual key = base64decode(key + "=") → 32 bytes.
/// IV = key[0..16], padding = PKCS7.
/// Decrypted = random(16) + msg_len(4, big-endian) + msg + corp_id
fn decrypt_msg(encoding_aes_key: &str, ciphertext_b64: &str) -> Result<(String, String)> {
    use aes::cipher::{BlockDecryptMut, KeyIvInit};
    use base64::Engine;

    let key_b64 = format!("{}=", encoding_aes_key);
    let aes_key = base64::engine::general_purpose::STANDARD.decode(&key_b64)?;
    if aes_key.len() != 32 {
        anyhow::bail!("Invalid EncodingAESKey length: {} (expected 32)", aes_key.len());
    }

    let iv = &aes_key[..16];
    let ciphertext = base64::engine::general_purpose::STANDARD.decode(ciphertext_b64)?;

    type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
    let mut buf = ciphertext.clone();
    let decrypted = Aes256CbcDec::new_from_slices(&aes_key, iv)?
        .decrypt_padded_mut::<cbc::cipher::block_padding::Pkcs7>(&mut buf)
        .map_err(|e| anyhow::anyhow!("AES decrypt failed: {:?}", e))?;

    if decrypted.len() < 20 {
        anyhow::bail!("Decrypted message too short: {} bytes", decrypted.len());
    }

    let msg_len = u32::from_be_bytes([decrypted[16], decrypted[17], decrypted[18], decrypted[19]]) as usize;
    if 20 + msg_len > decrypted.len() {
        anyhow::bail!("Message length {} exceeds decrypted buffer {}", msg_len, decrypted.len());
    }

    let msg = String::from_utf8_lossy(&decrypted[20..20 + msg_len]).to_string();
    let corp_id = String::from_utf8_lossy(&decrypted[20 + msg_len..]).to_string();

    Ok((msg, corp_id))
}

/// Hex-encode a byte slice (avoid adding `hex` crate dependency).
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
    }
}
