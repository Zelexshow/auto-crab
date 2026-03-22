use anyhow::{bail, Result};
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

pub struct WebRequester {
    client: Client,
    enabled: bool,
    allowed_domains: Vec<String>,
}

impl WebRequester {
    pub fn new(enabled: bool, allowed_domains: Vec<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("AutoCrab/0.1")
            .build()
            .expect("failed to build HTTP client");

        Self {
            client,
            enabled,
            allowed_domains,
        }
    }

    fn check_domain(&self, url: &str) -> Result<()> {
        if !self.enabled {
            bail!("Network access is disabled in configuration");
        }

        if self.allowed_domains.is_empty() {
            return Ok(());
        }

        let parsed = url::Url::parse(url).map_err(|e| anyhow::anyhow!("Invalid URL: {}", e))?;
        let host = parsed.host_str().unwrap_or("");

        for pattern in &self.allowed_domains {
            let pattern = pattern.trim();
            if pattern.starts_with("*.") {
                let suffix = &pattern[1..];
                if host.ends_with(suffix) || host == &pattern[2..] {
                    return Ok(());
                }
            } else if host == pattern {
                return Ok(());
            }
        }

        bail!(
            "Domain '{}' is not in the allowed list: {:?}",
            host,
            self.allowed_domains
        );
    }

    pub async fn get(&self, url: &str) -> Result<WebResponse> {
        self.check_domain(url)?;

        let resp = self.client.get(url).send().await?;
        let status = resp.status().as_u16();
        let headers: Vec<(String, String)> = resp
            .headers()
            .iter()
            .take(20)
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        let body = resp.text().await?;
        let body = if body.len() > 50000 {
            format!("{}...\n[内容截断，共 {} 字符]", &body[..50000], body.len())
        } else {
            body
        };

        Ok(WebResponse {
            status,
            headers,
            body,
        })
    }

    pub async fn post_json(&self, url: &str, json_body: &serde_json::Value) -> Result<WebResponse> {
        self.check_domain(url)?;

        let resp = self.client.post(url).json(json_body).send().await?;

        let status = resp.status().as_u16();
        let body = resp.text().await?;

        Ok(WebResponse {
            status,
            headers: vec![],
            body,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct WebResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}
