use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub models: ModelsConfig,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub remote: RemoteConfig,
    #[serde(default)]
    pub scheduled_tasks: ScheduledTasksConfig,
}

impl AppConfig {
    pub fn validate(&self) -> Result<()> {
        if self.agent.name.is_empty() {
            bail!("agent.name cannot be empty");
        }
        if self.agent.max_context_tokens == 0 {
            bail!("agent.max_context_tokens must be > 0");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub first_run: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            language: default_language(),
            theme: default_theme(),
            first_run: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelsConfig {
    pub primary: Option<ModelEntry>,
    pub fallback: Option<ModelEntry>,
    pub coding: Option<ModelEntry>,
    pub vision: Option<ModelEntry>,
    #[serde(default)]
    pub routing: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub api_key_ref: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_agent_name")]
    pub name: String,
    #[serde(default = "default_personality")]
    pub personality: String,
    #[serde(default = "default_max_context")]
    pub max_context_tokens: usize,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    #[serde(default)]
    pub long_term_memory: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: default_agent_name(),
            personality: default_personality(),
            max_context_tokens: default_max_context(),
            system_prompt: default_system_prompt(),
            long_term_memory: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub master_password_required: bool,
    #[serde(default = "default_lock_minutes")]
    pub auto_lock_minutes: u32,
    #[serde(default)]
    pub risk_overrides: HashMap<String, RiskLevel>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            master_password_required: true,
            auto_lock_minutes: default_lock_minutes(),
            risk_overrides: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Safe,
    Moderate,
    Dangerous,
    Forbidden,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default)]
    pub file_access: Vec<String>,
    #[serde(default = "default_true")]
    pub shell_enabled: bool,
    #[serde(default)]
    pub shell_allowed_commands: Vec<String>,
    #[serde(default = "default_true")]
    pub network_access: bool,
    #[serde(default)]
    pub network_allowed_domains: Vec<String>,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            file_access: vec![],
            shell_enabled: true,
            shell_allowed_commands: vec![
                "git".into(),
                "npm".into(),
                "pnpm".into(),
                "python".into(),
                "cargo".into(),
                "node".into(),
                "cmd".into(),
                "powershell".into(),
                "pwsh".into(),
                "echo".into(),
                "dir".into(),
                "ls".into(),
                "cat".into(),
                "mkdir".into(),
                "cp".into(),
                "mv".into(),
                "rm".into(),
                "type".into(),
                "where".into(),
                "whoami".into(),
                "start".into(),
                "tasklist".into(),
                "taskkill".into(),
                "wmic".into(),
            ],
            network_access: true,
            network_allowed_domains: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RemoteConfig {
    #[serde(default)]
    pub enabled: bool,
    pub feishu: Option<FeishuConfig>,
    pub wechat_work: Option<WechatWorkConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuConfig {
    pub app_id: String,
    pub app_secret_ref: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    #[serde(default)]
    pub allowed_user_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WechatWorkConfig {
    pub corp_id: String,
    pub agent_id: String,
    pub secret_ref: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub encoding_aes_key: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    #[serde(default)]
    pub allowed_user_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTasksConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub require_confirmation: bool,
    #[serde(default)]
    pub jobs: Vec<ScheduledJob>,
}

impl Default for ScheduledTasksConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            require_confirmation: true,
            jobs: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub name: String,
    pub cron: String,
    pub action: String,
    #[serde(default)]
    pub auto_execute: bool,
}

fn default_language() -> String {
    "zh-CN".into()
}
fn default_theme() -> String {
    "system".into()
}
fn default_agent_name() -> String {
    "小蟹".into()
}
fn default_personality() -> String {
    "professional".into()
}
fn default_max_context() -> usize {
    128000
}
fn default_system_prompt() -> String {
    let os_info = if cfg!(target_os = "windows") {
        "系统: Windows。Shell: cmd /C。路径可用 ~ 或反斜杠。"
    } else if cfg!(target_os = "macos") {
        "系统: macOS。Shell: sh -c。"
    } else {
        "系统: Linux。Shell: sh -c。"
    };
    format!(
        "你是 Auto Crab（小蟹），一个桌面AI助理。{}\n\n\
你有工具可用（文件操作、命令执行、截图分析、屏幕操控、网页抓取），\
但只在用户明确要求操作时才调用。普通聊天、问答、闲聊不需要使用任何工具。\n\
当用户要求执行操作时，直接调用工具完成，不要只描述步骤。\n\
请用中文回复。",
        os_info
    )
}
fn default_true() -> bool {
    true
}
fn default_lock_minutes() -> u32 {
    15
}
fn default_poll_interval() -> u64 {
    30
}
