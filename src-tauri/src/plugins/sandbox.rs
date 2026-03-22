use super::manifest::{PluginManifest, PluginPermissions};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// WASM plugin sandbox.
///
/// NOTE: Full wasmtime integration requires adding `wasmtime` to Cargo.toml.
/// This module provides the sandbox architecture and permission enforcement.
/// The actual WASM execution will be wired up when wasmtime is added.
///
/// Architecture:
/// 1. Plugin WASM is loaded into an isolated wasmtime instance
/// 2. Host functions are selectively exposed based on PluginPermissions
/// 3. File/network/shell access is mediated through the sandbox
/// 4. Plugin cannot escape its allowed paths or domains
pub struct PluginSandbox {
    manifest: PluginManifest,
    wasm_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCallResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

impl PluginSandbox {
    pub fn new(manifest: PluginManifest, plugin_dir: PathBuf) -> Self {
        let wasm_path = plugin_dir.join(&manifest.wasm_file);
        Self {
            manifest,
            wasm_path,
        }
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    pub fn permissions(&self) -> &PluginPermissions {
        &self.manifest.permissions
    }

    /// Check if this plugin is allowed to perform an operation.
    pub fn check_permission(&self, operation: &str, target: &str) -> Result<()> {
        match operation {
            "file_read" => {
                if !self.manifest.permissions.file_read {
                    anyhow::bail!(
                        "Plugin '{}' does not have file_read permission",
                        self.manifest.name
                    );
                }
                self.check_path_allowed(target)?;
            }
            "file_write" => {
                if !self.manifest.permissions.file_write {
                    anyhow::bail!(
                        "Plugin '{}' does not have file_write permission",
                        self.manifest.name
                    );
                }
                self.check_path_allowed(target)?;
            }
            "network" => {
                if !self.manifest.permissions.network {
                    anyhow::bail!(
                        "Plugin '{}' does not have network permission",
                        self.manifest.name
                    );
                }
                self.check_domain_allowed(target)?;
            }
            "shell" => {
                if !self.manifest.permissions.shell {
                    anyhow::bail!(
                        "Plugin '{}' does not have shell permission",
                        self.manifest.name
                    );
                }
            }
            _ => {
                anyhow::bail!("Unknown permission type: {}", operation);
            }
        }
        Ok(())
    }

    fn check_path_allowed(&self, path: &str) -> Result<()> {
        if self.manifest.permissions.allowed_paths.is_empty() {
            return Ok(());
        }
        for allowed in &self.manifest.permissions.allowed_paths {
            if path.starts_with(allowed) {
                return Ok(());
            }
        }
        anyhow::bail!(
            "Plugin '{}': path '{}' is outside allowed paths: {:?}",
            self.manifest.name,
            path,
            self.manifest.permissions.allowed_paths
        );
    }

    fn check_domain_allowed(&self, domain: &str) -> Result<()> {
        if self.manifest.permissions.allowed_domains.is_empty() {
            return Ok(());
        }
        for allowed in &self.manifest.permissions.allowed_domains {
            if allowed.starts_with("*.") {
                let suffix = &allowed[1..];
                if domain.ends_with(suffix) || domain == &allowed[2..] {
                    return Ok(());
                }
            } else if domain == allowed {
                return Ok(());
            }
        }
        anyhow::bail!(
            "Plugin '{}': domain '{}' is not allowed: {:?}",
            self.manifest.name,
            domain,
            self.manifest.permissions.allowed_domains
        );
    }

    /// Placeholder for WASM execution.
    /// In production, this would use wasmtime to execute the plugin.
    pub async fn call(&self, function: &str, args: &str) -> PluginCallResult {
        tracing::info!(
            "Plugin '{}' call: {}({})",
            self.manifest.name,
            function,
            &args[..args.len().min(100)]
        );

        // TODO: Wire up wasmtime execution here.
        // For now, return a placeholder indicating the architecture is ready.
        PluginCallResult {
            success: false,
            output: String::new(),
            error: Some(format!(
                "WASM runtime not yet initialized. Plugin '{}' architecture is ready, \
                 add wasmtime dependency to enable execution.",
                self.manifest.name
            )),
        }
    }
}
