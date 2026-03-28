use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

const MAX_MEMORIES: usize = 500;
const TOP_K: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub embedding: Vec<f32>,
    pub source: String,
    pub created_at: String,
}

pub struct LongTermMemory {
    dir: PathBuf,
    api_key: String,
}

impl LongTermMemory {
    pub fn new(data_dir: PathBuf, api_key: String) -> Self {
        Self {
            dir: data_dir.join("memory"),
            api_key,
        }
    }

    pub async fn store(&self, content: &str, source: &str) -> Result<()> {
        if content.trim().len() < 20 { return Ok(()); }

        let embedding = self.get_embedding(content).await?;
        let entry = MemoryEntry {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            content: content.chars().take(500).collect(),
            embedding,
            source: source.to_string(),
            created_at: Utc::now().to_rfc3339(),
        };

        let mut memories = self.load_all().await;
        memories.push(entry);
        if memories.len() > MAX_MEMORIES {
            memories.drain(0..memories.len() - MAX_MEMORIES);
        }
        self.save_all(&memories).await?;
        tracing::info!("[Memory] Stored: {} chars from {}", content.len().min(500), source);
        Ok(())
    }

    pub async fn recall(&self, query: &str, top_k: Option<usize>) -> Result<Vec<String>> {
        let k = top_k.unwrap_or(TOP_K);
        let memories = self.load_all().await;
        if memories.is_empty() { return Ok(vec![]); }

        let query_emb = self.get_embedding(query).await?;

        let mut scored: Vec<(f32, &MemoryEntry)> = memories.iter()
            .map(|m| (cosine_similarity(&query_emb, &m.embedding), m))
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let results: Vec<String> = scored.iter()
            .take(k)
            .filter(|(score, _)| *score > 0.3)
            .map(|(score, m)| format!("[相关度:{:.0}%] {}", score * 100.0, m.content))
            .collect();

        tracing::info!("[Memory] Recalled {} entries for query", results.len());
        Ok(results)
    }

    async fn get_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        let body = serde_json::json!({
            "model": "text-embedding-v2",
            "input": { "texts": [text.chars().take(2000).collect::<String>()] },
            "parameters": { "text_type": "document" }
        });

        let resp = client
            .post("https://dashscope.aliyuncs.com/api/v1/services/embeddings/text-embedding/text-embedding")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err = resp.text().await.unwrap_or_default();
            anyhow::bail!("Embedding API error {}: {}", status, err);
        }

        let data: serde_json::Value = resp.json().await?;
        let emb = data["output"]["embeddings"][0]["embedding"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("No embedding in response"))?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect::<Vec<f32>>();

        if emb.is_empty() {
            anyhow::bail!("Empty embedding returned");
        }
        Ok(emb)
    }

    async fn load_all(&self) -> Vec<MemoryEntry> {
        let path = self.dir.join("vectors.json");
        if !path.exists() { return vec![]; }
        match fs::read_to_string(&path).await {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => vec![],
        }
    }

    async fn save_all(&self, memories: &[MemoryEntry]) -> Result<()> {
        fs::create_dir_all(&self.dir).await?;
        let path = self.dir.join("vectors.json");
        let json = serde_json::to_string(memories)?;
        fs::write(path, json).await?;
        Ok(())
    }

    pub async fn memory_count(&self) -> usize {
        self.load_all().await.len()
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { return 0.0; }
    dot / (norm_a * norm_b)
}
