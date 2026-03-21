use crate::models::provider::ChatMessage;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<StoredMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub model: Option<String>,
}

impl From<&ChatMessage> for StoredMessage {
    fn from(msg: &ChatMessage) -> Self {
        Self {
            role: format!("{:?}", msg.role).to_lowercase(),
            content: msg.content.clone(),
            timestamp: Utc::now(),
            model: None,
        }
    }
}

pub struct MemoryStore {
    data_dir: PathBuf,
}

impl MemoryStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir: data_dir.join("conversations"),
        }
    }

    pub async fn save_conversation(&self, conv: &Conversation) -> Result<()> {
        fs::create_dir_all(&self.data_dir).await?;
        let path = self.data_dir.join(format!("{}.json", conv.id));
        let json = serde_json::to_string_pretty(conv)?;
        fs::write(&path, json).await?;
        Ok(())
    }

    pub async fn load_conversation(&self, id: &str) -> Result<Conversation> {
        let path = self.data_dir.join(format!("{}.json", id));
        let content = fs::read_to_string(&path).await?;
        let conv: Conversation = serde_json::from_str(&content)?;
        Ok(conv)
    }

    pub async fn list_conversations(&self) -> Result<Vec<ConversationSummary>> {
        fs::create_dir_all(&self.data_dir).await?;
        let mut summaries = Vec::new();
        let mut entries = fs::read_dir(&self.data_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(content) = fs::read_to_string(&path).await {
                    if let Ok(conv) = serde_json::from_str::<Conversation>(&content) {
                        summaries.push(ConversationSummary {
                            id: conv.id,
                            title: conv.title,
                            updated_at: conv.updated_at,
                            message_count: conv.messages.len(),
                        });
                    }
                }
            }
        }

        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(summaries)
    }

    pub async fn delete_conversation(&self, id: &str) -> Result<()> {
        let path = self.data_dir.join(format!("{}.json", id));
        fs::remove_file(&path).await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
}
