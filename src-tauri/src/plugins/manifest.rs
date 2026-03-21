use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

/// Plugin manifest describing a WASM plugin's metadata and permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub wasm_file: String,

    #[serde(default)]
    pub permissions: PluginPermissions,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginPermissions {
    #[serde(default)]
    pub file_read: bool,
    #[serde(default)]
    pub file_write: bool,
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub shell: bool,
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub allowed_domains: Vec<String>,
}

impl PluginManifest {
    pub async fn load(manifest_path: &Path) -> Result<Self> {
        let content = fs::read_to_string(manifest_path).await?;
        let manifest: Self = toml::from_str(&content)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            anyhow::bail!("Plugin name cannot be empty");
        }
        if self.wasm_file.is_empty() {
            anyhow::bail!("Plugin wasm_file cannot be empty");
        }
        if self.permissions.shell {
            tracing::warn!(
                "Plugin '{}' requests shell access — this is a high-risk permission",
                self.name
            );
        }
        Ok(())
    }
}
