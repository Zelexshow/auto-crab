use anyhow::{bail, Result};
use serde::Serialize;
use std::process::Stdio;
use tokio::process::Command;

pub struct ShellExecutor {
    enabled: bool,
    allowed_commands: Vec<String>,
}

impl ShellExecutor {
    pub fn new(enabled: bool, allowed_commands: Vec<String>) -> Self {
        Self { enabled, allowed_commands }
    }

    fn validate_command(&self, command: &str) -> Result<()> {
        if !self.enabled {
            bail!("Shell execution is disabled in configuration");
        }

        if self.allowed_commands.is_empty() {
            return Ok(());
        }

        let first_word = command.split_whitespace().next().unwrap_or("");
        let base_cmd = first_word.rsplit(['/', '\\']).next().unwrap_or(first_word);
        let base_cmd = base_cmd.strip_suffix(".exe").unwrap_or(base_cmd);

        if self.allowed_commands.iter().any(|c| c == base_cmd) {
            Ok(())
        } else {
            bail!(
                "Command '{}' is not in the allowed list: {:?}",
                base_cmd,
                self.allowed_commands
            );
        }
    }

    pub async fn execute(&self, command: &str, working_dir: Option<&str>) -> Result<ShellOutput> {
        self.validate_command(command)?;

        let (shell, flag) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        let mut cmd = Command::new(shell);
        cmd.arg(flag).arg(command);

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        cmd.stdout(Stdio::piped())
           .stderr(Stdio::piped());

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            cmd.output(),
        ).await
            .map_err(|_| anyhow::anyhow!("Command timed out after 60 seconds"))??;

        Ok(ShellOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

#[derive(Debug, Serialize)]
pub struct ShellOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
