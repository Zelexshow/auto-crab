mod schema;

pub use schema::*;

use anyhow::Result;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tokio::fs;

const CONFIG_FILE: &str = "auto-crab.toml";
const DEFAULT_CONFIG: &str = include_str!("../../defaults/auto-crab.default.toml");

pub fn config_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .expect("failed to resolve app config dir")
}

pub fn config_path(app: &AppHandle) -> PathBuf {
    config_dir(app).join(CONFIG_FILE)
}

pub async fn ensure_config_dir(app: &AppHandle) -> Result<()> {
    let dir = config_dir(app);
    fs::create_dir_all(&dir).await?;

    let path = dir.join(CONFIG_FILE);
    if !path.exists() {
        fs::write(&path, DEFAULT_CONFIG).await?;
        tracing::info!("Created default config at {:?}", path);
    }
    Ok(())
}

pub async fn load_config(app: &AppHandle) -> Result<AppConfig> {
    let path = config_path(app);
    let content = fs::read_to_string(&path).await?;
    let config: AppConfig = toml::from_str(&content)?;
    config.validate()?;
    Ok(config)
}

pub async fn save_config(app: &AppHandle, config: &AppConfig) -> Result<()> {
    config.validate()?;
    let path = config_path(app);
    let content = toml::to_string_pretty(config)?;
    fs::write(&path, content).await?;
    tracing::info!("Config saved to {:?}", path);
    Ok(())
}
