use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

/// File snapshot system for operation rollback.
/// Before any write/delete operation, a snapshot is taken.
/// Users can undo by restoring from snapshot.
pub struct SnapshotStore {
    snapshot_dir: PathBuf,
    max_snapshots: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub original_path: String,
    pub snapshot_path: String,
    pub operation: String,
    pub timestamp: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotIndex {
    pub snapshots: Vec<Snapshot>,
}

impl SnapshotStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            snapshot_dir: data_dir.join("snapshots"),
            max_snapshots: 100,
        }
    }

    /// Take a snapshot of a file before modifying/deleting it.
    pub async fn take_snapshot(&self, file_path: &str, operation: &str) -> Result<Snapshot> {
        let path = Path::new(file_path);
        if !path.exists() {
            anyhow::bail!("File does not exist: {}", file_path);
        }

        fs::create_dir_all(&self.snapshot_dir).await?;

        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let file_name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into());

        let snapshot_filename = format!("{}_{}", timestamp, file_name);
        let snapshot_path = self.snapshot_dir.join(&snapshot_filename);

        fs::copy(path, &snapshot_path).await?;

        let metadata = fs::metadata(&snapshot_path).await?;

        let snapshot = Snapshot {
            id,
            original_path: file_path.to_string(),
            snapshot_path: snapshot_path.to_string_lossy().to_string(),
            operation: operation.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            size_bytes: metadata.len(),
        };

        self.append_to_index(&snapshot).await?;
        self.cleanup_old_snapshots().await?;

        tracing::info!("Snapshot taken: {} -> {}", file_path, snapshot_filename);
        Ok(snapshot)
    }

    /// Restore a file from a snapshot (undo).
    pub async fn restore(&self, snapshot_id: &str) -> Result<String> {
        let index = self.load_index().await?;
        let snapshot = index.snapshots.iter()
            .find(|s| s.id == snapshot_id)
            .ok_or_else(|| anyhow::anyhow!("Snapshot not found: {}", snapshot_id))?;

        let src = Path::new(&snapshot.snapshot_path);
        if !src.exists() {
            anyhow::bail!("Snapshot file missing: {}", snapshot.snapshot_path);
        }

        let dest = Path::new(&snapshot.original_path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::copy(src, dest).await?;
        tracing::info!("Restored: {} from snapshot {}", snapshot.original_path, snapshot_id);

        Ok(snapshot.original_path.clone())
    }

    /// List recent snapshots.
    pub async fn list(&self, limit: usize) -> Result<Vec<Snapshot>> {
        let index = self.load_index().await?;
        let items: Vec<Snapshot> = index.snapshots.into_iter().rev().take(limit).collect();
        Ok(items)
    }

    async fn index_path(&self) -> PathBuf {
        self.snapshot_dir.join("index.json")
    }

    async fn load_index(&self) -> Result<SnapshotIndex> {
        let path = self.index_path().await;
        if path.exists() {
            let content = fs::read_to_string(&path).await?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(SnapshotIndex { snapshots: vec![] })
        }
    }

    async fn append_to_index(&self, snapshot: &Snapshot) -> Result<()> {
        let mut index = self.load_index().await?;
        index.snapshots.push(snapshot.clone());
        let content = serde_json::to_string_pretty(&index)?;
        fs::write(self.index_path().await, content).await?;
        Ok(())
    }

    async fn cleanup_old_snapshots(&self) -> Result<()> {
        let mut index = self.load_index().await?;
        while index.snapshots.len() > self.max_snapshots {
            let oldest = index.snapshots.remove(0);
            let path = Path::new(&oldest.snapshot_path);
            if path.exists() {
                let _ = fs::remove_file(path).await;
            }
        }
        let content = serde_json::to_string_pretty(&index)?;
        fs::write(self.index_path().await, content).await?;
        Ok(())
    }
}
