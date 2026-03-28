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
    #[serde(default)]
    pub search: SearchConfig,
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
    /// User-defined custom instructions appended to the system prompt.
    #[serde(default)]
    pub custom_instructions: String,
    /// Named skills – loaded from skills/ directory at runtime, not serialized to TOML.
    #[serde(skip)]
    pub skills: Vec<UserSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSkill {
    pub name: String,
    pub content: String,
    /// Trigger keywords for auto-matching. If empty, derived from name + content headings.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// If true, always inject into system prompt regardless of matching.
    #[serde(default)]
    pub always_on: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: default_agent_name(),
            personality: default_personality(),
            max_context_tokens: default_max_context(),
            system_prompt: default_system_prompt(),
            long_term_memory: false,
            custom_instructions: String::new(),
            skills: Vec::new(),
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
    /// Optional reference to a named skill from AgentConfig.skills.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_ref: Option<String>,
}

/// Web search API configuration. When an API key is provided, uses the
/// corresponding service for reliable structured results. Falls back to
/// DuckDuckGo HTML scraping when no API is configured.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// "serpapi" | "brave" | "tavily" | "auto" (try API first, fallback to scraping)
    #[serde(default = "default_search_provider")]
    pub provider: String,
    /// SerpApi key (free 250/month at https://serpapi.com)
    #[serde(default)]
    pub serpapi_api_key: String,
    /// Brave Search API key (free 1000/month at https://brave.com/search/api/)
    #[serde(default)]
    pub brave_api_key: String,
    /// Tavily API key (free 1000 credits/month at https://tavily.com)
    #[serde(default)]
    pub tavily_api_key: String,
    /// Monthly quota limits
    #[serde(default = "default_serpapi_quota")]
    pub serpapi_monthly_quota: u32,
    #[serde(default = "default_brave_quota")]
    pub brave_monthly_quota: u32,
    #[serde(default = "default_tavily_quota")]
    pub tavily_monthly_quota: u32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            provider: default_search_provider(),
            serpapi_api_key: String::new(),
            brave_api_key: String::new(),
            tavily_api_key: String::new(),
            serpapi_monthly_quota: 250,
            brave_monthly_quota: 1000,
            tavily_monthly_quota: 1000,
        }
    }
}

fn default_serpapi_quota() -> u32 { 250 }
fn default_brave_quota() -> u32 { 1000 }
fn default_tavily_quota() -> u32 { 1000 }

fn default_search_provider() -> String {
    "auto".into()
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
你有工具可用（文件操作、命令执行、截图分析、屏幕操控、网页抓取、行情查询），\
但只在用户明确要求操作时才调用。普通聊天、问答、闲聊不需要使用任何工具。\n\
当用户要求执行操作时，直接调用工具完成，不要只描述步骤。\n\n\
【行情查询规则 - 极其重要】\n\
1. 查询任何金融资产价格（股票、指数、加密货币、黄金白银、原油、外汇）时，\
必须调用 get_market_price 工具获取实时数据，绝对不要用 search_web 查价格，不要编造或引用新闻中的过时数据。\n\
2. 严格只回答用户当前消息中提到的标的。不要把对话历史中提到的其他标的混入回答。\
用户问\"茅台\"就只回答茅台，问\"BTC\"就只回答BTC，不要自作主张添加未被询问的内容。\n\
3. 回答中必须包含工具返回的实际数据（价格、涨跌幅），不要用\"需查询\"代替。\n\n\
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
