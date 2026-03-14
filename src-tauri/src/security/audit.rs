use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::config::RiskLevel;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub operation: String,
    pub risk_level: RiskLevel,
    pub status: AuditStatus,
    pub details: String,
    pub source: AuditSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditStatus {
    Approved,
    Rejected,
    AutoApproved,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditSource {
    Local,
    Feishu,
    WechatWork,
    Scheduled,
}

pub struct AuditLogger {
    log_path: PathBuf,
    buffer: Arc<Mutex<Vec<AuditEntry>>>,
}

impl AuditLogger {
    pub fn new(data_dir: PathBuf) -> Self {
        let log_path = data_dir.join("audit");
        Self {
            log_path,
            buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn log(
        &self,
        operation: &str,
        risk_level: RiskLevel,
        status: AuditStatus,
        details: &str,
        source: AuditSource,
    ) -> anyhow::Result<()> {
        let entry = AuditEntry {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            operation: operation.to_string(),
            risk_level,
            status,
            details: details.to_string(),
            source,
        };

        let date = entry.timestamp.format("%Y-%m-%d").to_string();
        let dir = self.log_path.clone();
        fs::create_dir_all(&dir).await?;

        let file_path = dir.join(format!("{}.jsonl", date));
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .await?;

        let line = serde_json::to_string(&entry)? + "\n";
        file.write_all(line.as_bytes()).await?;

        let mut buf = self.buffer.lock().await;
        buf.push(entry);
        if buf.len() > 1000 {
            buf.drain(..500);
        }

        Ok(())
    }

    pub async fn recent(&self, limit: usize) -> Vec<AuditEntry> {
        let buf = self.buffer.lock().await;
        buf.iter().rev().take(limit).cloned().collect()
    }
}
