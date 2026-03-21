use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::Command;

/// Browser automation via Chrome DevTools Protocol (CDP).
///
/// Uses headless Chrome/Edge that's already installed on the system.
/// No extra dependency required — communicates via CDP WebSocket.
///
/// For full browser automation, a dedicated library like chromiumoxide
/// can be added later. This module provides basic page fetching and
/// screenshot capabilities.
pub struct BrowserAutomation {
    chrome_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageContent {
    pub url: String,
    pub title: String,
    pub text_content: String,
    pub status: String,
}

impl BrowserAutomation {
    pub fn new() -> Self {
        let chrome_path = detect_chrome();
        if let Some(ref path) = chrome_path {
            tracing::info!("Chrome detected at: {}", path);
        } else {
            tracing::warn!("Chrome/Edge not found, browser automation unavailable");
        }
        Self { chrome_path }
    }

    pub fn is_available(&self) -> bool {
        self.chrome_path.is_some()
    }

    /// Fetch a page's text content using headless Chrome --dump-dom.
    pub async fn fetch_page_text(&self, url: &str) -> Result<PageContent> {
        let chrome = self.chrome_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Chrome/Edge not found"))?;

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Command::new(chrome)
                .args([
                    "--headless=new",
                    "--disable-gpu",
                    "--no-sandbox",
                    "--dump-dom",
                    url,
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        ).await
            .map_err(|_| anyhow::anyhow!("Chrome timed out after 30s"))??;

        let html = String::from_utf8_lossy(&output.stdout).to_string();

        let text = extract_text_from_html(&html);

        Ok(PageContent {
            url: url.to_string(),
            title: extract_title(&html).unwrap_or_default(),
            text_content: if text.len() > 50000 {
                format!("{}...\n[内容截断]", &text[..50000])
            } else {
                text
            },
            status: if output.status.success() { "ok".into() } else { "error".into() },
        })
    }

    /// Take a screenshot of a page (saves to file).
    pub async fn screenshot(&self, url: &str, output_path: &str) -> Result<String> {
        let chrome = self.chrome_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Chrome/Edge not found"))?;

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Command::new(chrome)
                .args([
                    "--headless=new",
                    "--disable-gpu",
                    "--no-sandbox",
                    &format!("--screenshot={}", output_path),
                    "--window-size=1280,720",
                    url,
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        ).await
            .map_err(|_| anyhow::anyhow!("Chrome screenshot timed out"))??;

        if output.status.success() {
            Ok(output_path.to_string())
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Screenshot failed: {}", err)
        }
    }
}

fn detect_chrome() -> Option<String> {
    let candidates = if cfg!(target_os = "windows") {
        vec![
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ]
    } else {
        vec![
            "google-chrome",
            "google-chrome-stable",
            "chromium-browser",
            "chromium",
            "microsoft-edge",
        ]
    };

    for path in candidates {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    // Try `which` on Linux/macOS
    if !cfg!(target_os = "windows") {
        for name in ["google-chrome", "chromium-browser", "chromium"] {
            if std::process::Command::new("which").arg(name).output().is_ok_and(|o| o.status.success()) {
                return Some(name.to_string());
            }
        }
    }

    None
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title>")?;
    let end = lower[start..].find("</title>")?;
    Some(html[start + 7..start + end].trim().to_string())
}

fn extract_text_from_html(html: &str) -> String {
    let mut text = String::with_capacity(html.len() / 3);
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;

    let lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        if !in_tag && chars[i] == '<' {
            in_tag = true;
            let rest: String = lower_chars[i..].iter().take(10).collect();
            if rest.starts_with("<script") { in_script = true; }
            if rest.starts_with("<style") { in_style = true; }
            if rest.starts_with("</script") { in_script = false; }
            if rest.starts_with("</style") { in_style = false; }
        } else if in_tag && chars[i] == '>' {
            in_tag = false;
        } else if !in_tag && !in_script && !in_style {
            text.push(chars[i]);
        }
        i += 1;
    }

    // Clean up whitespace
    let lines: Vec<&str> = text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    lines.join("\n")
}
