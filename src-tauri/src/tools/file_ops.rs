use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use tokio::fs;

pub struct FileOps {
    allowed_roots: Vec<PathBuf>,
}

impl FileOps {
    pub fn new(allowed_roots: Vec<PathBuf>) -> Self {
        Self { allowed_roots }
    }

    fn check_access(&self, path: &Path) -> Result<PathBuf> {
        let canonical = path
            .canonicalize()
            .or_else(|_| Ok::<PathBuf, std::io::Error>(path.to_path_buf()))?;

        if self.allowed_roots.is_empty() {
            return Ok(canonical);
        }

        for root in &self.allowed_roots {
            let root_canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
            if canonical.starts_with(&root_canonical) {
                return Ok(canonical);
            }
        }

        bail!(
            "Access denied: '{}' is outside allowed directories: {:?}",
            path.display(),
            self.allowed_roots
        );
    }

    pub fn expand_path(path: &str) -> PathBuf {
        let expanded = shellexpand::tilde(path).to_string();
        PathBuf::from(expanded)
    }

    pub async fn read_file(&self, path: &str) -> Result<String> {
        let path = self.check_access(&Self::expand_path(path))?;
        let content = fs::read_to_string(&path).await?;
        Ok(content)
    }

    pub async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        let path = self.check_access(&Self::expand_path(path))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, content).await?;
        Ok(())
    }

    pub async fn list_directory(&self, path: &str) -> Result<Vec<FileEntry>> {
        let path = self.check_access(&Self::expand_path(path))?;
        let mut entries = Vec::new();
        let mut dir = fs::read_dir(&path).await?;

        while let Some(entry) = dir.next_entry().await? {
            let metadata = entry.metadata().await?;
            entries.push(FileEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: entry.path().to_string_lossy().to_string(),
                is_dir: metadata.is_dir(),
                size: metadata.len(),
            });
        }

        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));

        Ok(entries)
    }

    pub async fn delete_file(&self, path: &str) -> Result<()> {
        let path = self.check_access(&Self::expand_path(path))?;
        fs::remove_file(&path).await?;
        Ok(())
    }
}

#[derive(Debug, serde::Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}
