mod commands;
mod config;
mod core;
mod mcp;
mod models;
mod plugins;
mod remote;
mod security;
mod tools;

use std::collections::HashMap;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Manager,
};
use tokio::sync::Mutex;
use tracing_subscriber::{fmt, EnvFilter};

const REMOTE_HISTORY_MAX_MESSAGES: usize = 20;

#[derive(Default)]
struct MonitorState {
    tasks: Mutex<HashMap<String, MonitorTask>>,
}

struct MonitorTask {
    description: String,
    interval_secs: u64,
    cancel: tokio::sync::watch::Sender<bool>,
}

impl MonitorState {
    async fn add(&self, id: String, task: MonitorTask) {
        let mut tasks = self.tasks.lock().await;
        if let Some(old) = tasks.remove(&id) {
            let _ = old.cancel.send(true);
        }
        tasks.insert(id, task);
    }

    async fn remove(&self, id: &str) -> bool {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.remove(id) {
            let _ = task.cancel.send(true);
            true
        } else {
            false
        }
    }

    async fn list(&self) -> Vec<(String, String, u64)> {
        let tasks = self.tasks.lock().await;
        tasks
            .iter()
            .map(|(id, t)| (id.clone(), t.description.clone(), t.interval_secs))
            .collect()
    }
}

#[derive(Default)]
struct RemoteConversationState {
    sessions: Mutex<HashMap<String, Vec<crate::models::provider::ChatMessage>>>,
    active_sessions: Mutex<HashMap<String, String>>,
}

impl RemoteConversationState {
    async fn get_history(&self, session_key: &str) -> Vec<crate::models::provider::ChatMessage> {
        let sessions = self.sessions.lock().await;
        sessions.get(session_key).cloned().unwrap_or_default()
    }

    async fn append_turn(&self, session_key: &str, user: &str, assistant: &str) {
        let mut sessions = self.sessions.lock().await;
        let entry = sessions.entry(session_key.to_string()).or_default();
        entry.push(crate::models::provider::ChatMessage {
            role: crate::models::provider::MessageRole::User,
            content: user.to_string(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });
        entry.push(crate::models::provider::ChatMessage {
            role: crate::models::provider::MessageRole::Assistant,
            content: assistant.to_string(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });
        if entry.len() > REMOTE_HISTORY_MAX_MESSAGES {
            let overflow = entry.len() - REMOTE_HISTORY_MAX_MESSAGES;
            entry.drain(0..overflow);
        }
    }

    async fn clear(&self, session_key: &str) {
        let mut sessions = self.sessions.lock().await;
        sessions.remove(session_key);
    }

    async fn set_active_session(&self, user_id: &str, session_key: &str) {
        let mut active = self.active_sessions.lock().await;
        active.insert(user_id.to_string(), session_key.to_string());
    }

    async fn get_active_session(&self, user_id: &str) -> String {
        let active = self.active_sessions.lock().await;
        active.get(user_id).cloned().unwrap_or_default()
    }

    async fn list_sessions(&self, user_id: &str) -> Vec<String> {
        let sessions = self.sessions.lock().await;
        let active = self.active_sessions.lock().await;
        let prefix = format!("feishu:{}", user_id);
        let mut all: std::collections::HashSet<String> = sessions
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();
        if let Some(active_key) = active.get(user_id) {
            if active_key.starts_with(&prefix) {
                all.insert(active_key.clone());
            }
        }
        let mut list: Vec<String> = all.into_iter().collect();
        list.sort();
        list
    }
}

fn create_memory(cfg: &config::AppConfig) -> Option<std::sync::Arc<core::long_memory::LongTermMemory>> {
    if !cfg.agent.long_term_memory {
        return None;
    }

    let dashscope_key = cfg.models.vision.as_ref()
        .and_then(|m| m.api_key_ref.as_deref())
        .and_then(|r| crate::security::credentials::CredentialStore::resolve_ref(r).ok());

    dashscope_key.map(|key| {
        let data_dir = commands::app_data_dir();
        tracing::info!("[Memory] Long-term memory enabled (DashScope Embedding), dir: {}", data_dir.display());
        std::sync::Arc::new(core::long_memory::LongTermMemory::new(data_dir, key))
    })
}

/// Parse natural language for monitoring intent.
/// Returns (interval_secs, description) if the message looks like a monitor request.
/// Supports: crypto, A-shares, HK/US/JP stocks, gold, silver, commodities, forex.
fn parse_natural_monitor(text: &str) -> Option<(u64, String)> {
    let lower = text.to_lowercase();

    let has_monitor_intent = lower.contains("盯盘") || lower.contains("盯一下")
        || lower.contains("盯着") || lower.contains("帮我盯")
        || lower.contains("持续监控") || lower.contains("定时查")
        || lower.contains("每隔") || lower.contains("monitor");

    if !has_monitor_intent {
        return None;
    }

    let has_financial_asset =
        // Crypto
        lower.contains("btc") || lower.contains("eth") || lower.contains("sol")
        || lower.contains("bnb") || lower.contains("xrp") || lower.contains("doge")
        || lower.contains("比特币") || lower.contains("以太坊") || lower.contains("币价") || lower.contains("crypto")
        // Commodities
        || lower.contains("黄金") || lower.contains("白银") || lower.contains("gold") || lower.contains("silver")
        || lower.contains("原油") || lower.contains("crude") || lower.contains("wti")
        // Stocks
        || lower.contains("茅台") || lower.contains("平安") || lower.contains("宁德") || lower.contains("比亚迪")
        || lower.contains("腾讯") || lower.contains("阿里") || lower.contains("美团") || lower.contains("小米")
        || lower.contains("苹果") || lower.contains("特斯拉") || lower.contains("英伟达") || lower.contains("微软")
        // Indices
        || lower.contains("上证") || lower.contains("沪深") || lower.contains("创业板") || lower.contains("恒生")
        || lower.contains("恒指") || lower.contains("纳指") || lower.contains("纳斯达克") || lower.contains("道琼斯")
        || lower.contains("道指") || lower.contains("标普") || lower.contains("日经")
        // Markets
        || lower.contains("a股") || lower.contains("港股") || lower.contains("美股") || lower.contains("日股")
        // Forex
        || lower.contains("美元") || lower.contains("人民币") || lower.contains("汇率")
        // Stock codes
        || regex::Regex::new(r"(?i)(sh|sz)\d{6}|hk\d{5}|\b[A-Z]{2,5}\b").ok()
            .and_then(|re| re.find(&lower)).is_some();

    if !has_financial_asset {
        return None;
    }

    let interval = extract_interval(text);
    let desc = extract_monitor_description(text);

    Some((interval, desc))
}

/// Extract a concise description of what to monitor from the user's natural language.
fn extract_monitor_description(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut targets: Vec<&str> = Vec::new();

    // Detect all mentioned assets
    let checks: Vec<(&str, &str)> = vec![
        ("btc", "BTC"), ("eth", "ETH"), ("sol", "SOL"), ("bnb", "BNB"), ("doge", "DOGE"),
        ("比特币", "BTC"), ("以太坊", "ETH"),
        ("黄金", "黄金"), ("gold", "黄金"), ("白银", "白银"), ("silver", "白银"),
        ("原油", "原油"), ("crude", "原油"), ("wti", "原油"),
        ("茅台", "茅台"), ("平安", "中国平安"), ("宁德", "宁德时代"), ("比亚迪", "比亚迪"),
        ("腾讯", "腾讯"), ("阿里", "阿里巴巴"), ("美团", "美团"), ("小米", "小米"),
        ("苹果", "苹果/AAPL"), ("特斯拉", "特斯拉/TSLA"), ("英伟达", "英伟达/NVDA"), ("微软", "微软/MSFT"),
        ("上证", "上证指数"), ("沪深300", "沪深300"), ("创业板", "创业板"),
        ("恒生", "恒生指数"), ("恒指", "恒生指数"),
        ("纳指", "纳指"), ("纳斯达克", "纳指"), ("道指", "道指"), ("道琼斯", "道指"), ("标普", "标普500"),
        ("日经", "日经225"),
        ("美元指数", "美元指数"), ("人民币", "美元/人民币"),
    ];

    for (kw, label) in &checks {
        if lower.contains(kw) && !targets.contains(label) {
            targets.push(label);
        }
    }

    if targets.is_empty() {
        format!("盯盘 {}", text.chars().take(30).collect::<String>())
    } else {
        format!("盯盘 {}", targets.join("/"))
    }
}

/// Extract time interval from natural language. Defaults to 300s (5min).
fn extract_interval(text: &str) -> u64 {
    use regex::Regex;

    // "5min", "5m", "5分钟", "5分", "10s", "10秒"
    if let Ok(re) = Regex::new(r"(\d+)\s*(?:min|m|分钟|分)") {
        if let Some(cap) = re.captures(text) {
            if let Ok(n) = cap[1].parse::<u64>() {
                return (n * 60).max(30);
            }
        }
    }
    if let Ok(re) = Regex::new(r"(\d+)\s*(?:s|秒)") {
        if let Some(cap) = re.captures(text) {
            if let Ok(n) = cap[1].parse::<u64>() {
                return n.max(30);
            }
        }
    }
    if let Ok(re) = Regex::new(r"每\s*(\d+)\s*(?:分钟|分|min|m)") {
        if let Some(cap) = re.captures(text) {
            if let Ok(n) = cap[1].parse::<u64>() {
                return (n * 60).max(30);
            }
        }
    }
    if let Ok(re) = Regex::new(r"(\d+)\s*(?:h|小时|hour)") {
        if let Some(cap) = re.captures(text) {
            if let Ok(n) = cap[1].parse::<u64>() {
                return (n * 3600).max(30);
            }
        }
    }
    300 // default 5 minutes
}

fn extract_crypto_symbols(desc: &str) -> Vec<String> {
    let desc_upper = desc.to_uppercase();
    let known = ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "XRPUSDT", "DOGEUSDT", "ADAUSDT", "AVAXUSDT"];
    let mut found: Vec<String> = known.iter()
        .filter(|s| desc_upper.contains(&s[..s.len()-4]))
        .map(|s| s.to_string())
        .collect();
    if found.is_empty() { found.push("BTCUSDT".into()); }
    found
}

/// Extract market queries from a monitor description.
/// Converts keywords like "BTC", "茅台", "黄金" into queries for `fetch_market_price`.
fn extract_monitor_queries(desc: &str) -> Vec<String> {
    let lower = desc.to_lowercase();
    let mut queries: Vec<String> = Vec::new();

    let asset_keywords: Vec<(&str, &str)> = vec![
        // Crypto
        ("btc", "BTCUSDT"), ("eth", "ETHUSDT"), ("sol", "SOLUSDT"),
        ("bnb", "BNBUSDT"), ("xrp", "XRPUSDT"), ("doge", "DOGEUSDT"),
        ("比特币", "BTCUSDT"), ("以太坊", "ETHUSDT"),
        // Commodities
        ("黄金", "黄金"), ("gold", "黄金"), ("白银", "白银"), ("silver", "白银"),
        ("原油", "原油"), ("crude", "原油"), ("wti", "原油"),
        // A-shares
        ("茅台", "茅台"), ("平安", "平安"), ("宁德", "宁德"), ("比亚迪", "比亚迪a"),
        ("招行", "招行"), ("五粮液", "五粮液"), ("万科", "万科"),
        ("上证指数", "上证指数"), ("沪深300", "沪深300"), ("创业板", "创业板"),
        // HK
        ("腾讯", "腾讯"), ("阿里", "阿里"), ("美团", "美团"), ("小米", "小米"),
        ("恒生", "恒生指数"), ("恒指", "恒生指数"),
        // US
        ("苹果", "苹果"), ("特斯拉", "特斯拉"), ("英伟达", "英伟达"), ("微软", "微软"),
        ("纳指", "纳指"), ("纳斯达克", "纳指"), ("道指", "道指"), ("道琼斯", "道指"), ("标普", "标普"),
        // JP
        ("日经", "日经"),
        // Forex
        ("美元指数", "美元指数"), ("人民币", "美元人民币"),
    ];

    for (kw, query) in &asset_keywords {
        if lower.contains(kw) && !queries.contains(&query.to_string()) {
            queries.push(query.to_string());
        }
    }

    queries
}

/// Build the full system prompt with context-aware skill injection.
/// Only injects skills that match the user's message or are marked always_on.
pub fn build_full_system_prompt(cfg: &config::AppConfig, user_input: Option<&str>) -> String {
    let mut prompt = cfg.agent.system_prompt.clone();

    if !cfg.agent.custom_instructions.is_empty() {
        prompt.push_str(&format!("\n\n【用户自定义指令】\n{}", cfg.agent.custom_instructions));
    }

    let matched = match_skills(&cfg.agent.skills, user_input);
    if !matched.is_empty() {
        let skill_text: Vec<String> = matched.iter()
            .map(|s| format!("[{}] {}", s.name, s.content))
            .collect();
        prompt.push_str(&format!("\n\n【已激活技能】\n{}", skill_text.join("\n")));
    }

    prompt
}

/// Build prompt for scheduled tasks with explicit skill_ref.
pub fn build_scheduled_prompt(cfg: &config::AppConfig, skill_ref: Option<&str>) -> String {
    let mut prompt = cfg.agent.system_prompt.clone();

    if !cfg.agent.custom_instructions.is_empty() {
        prompt.push_str(&format!("\n\n【用户自定义指令】\n{}", cfg.agent.custom_instructions));
    }

    if let Some(ref_name) = skill_ref {
        if let Some(skill) = cfg.agent.skills.iter().find(|s| s.name == ref_name) {
            prompt.push_str(&format!("\n\n【已激活技能: {}】\n{}", skill.name, skill.content));
        }
    }

    let always_on: Vec<&config::UserSkill> = cfg.agent.skills.iter()
        .filter(|s| s.always_on && skill_ref.map_or(true, |r| s.name != r))
        .collect();
    if !always_on.is_empty() {
        let text: Vec<String> = always_on.iter()
            .map(|s| format!("[{}] {}", s.name, s.content))
            .collect();
        prompt.push_str(&format!("\n\n【常驻技能】\n{}", text.join("\n")));
    }

    prompt
}

/// Match skills against user input by keyword scanning.
fn match_skills<'a>(skills: &'a [config::UserSkill], user_input: Option<&str>) -> Vec<&'a config::UserSkill> {
    let mut matched: Vec<&config::UserSkill> = skills.iter()
        .filter(|s| s.always_on)
        .collect();

    if let Some(input) = user_input {
        let input_lower = input.to_lowercase();
        for skill in skills {
            if skill.always_on { continue; }
            let hit = skill.keywords.iter().any(|kw| {
                input_lower.contains(&kw.to_lowercase())
            });
            if hit {
                matched.push(skill);
            }
        }
    }

    matched
}

/// Pre-fetch real market data and news for scheduled job actions.
/// If the action mentions investment/market keywords, fetches real-time prices
/// and news, then injects them into the prompt so the LLM has actual data.
async fn enrich_scheduled_action(action: &str) -> String {
    let enrich_start = std::time::Instant::now();
    let lower = action.to_lowercase();

    let is_investment = lower.contains("投资") || lower.contains("行情") || lower.contains("盯盘")
        || lower.contains("market") || lower.contains("报告") || lower.contains("stock")
        || lower.contains("金融") || lower.contains("交易");

    let is_learning = lower.contains("学习") || lower.contains("科技") || lower.contains("技术动态")
        || lower.contains("tech") || lower.contains("职业") || lower.contains("创业")
        || lower.contains("startup");

    if !is_investment && !is_learning {
        return action.to_string();
    }

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let mut sections: Vec<String> = Vec::new();

    if is_investment {
        let assets: Vec<(&str, &str)> = vec![
            ("上证指数", "上证指数"), ("沪深300", "沪深300"), ("创业板", "创业板"),
            ("恒生指数", "恒生指数"),
            ("纳指", "纳指"), ("道指", "道指"), ("标普", "标普"),
            ("日经", "日经"),
            ("BTCUSDT", "BTCUSDT"), ("ETHUSDT", "ETHUSDT"),
            ("黄金", "黄金"), ("白银", "白银"), ("原油", "原油"),
            ("美元人民币", "美元人民币"),
        ];

        // Parallel fetch with automatic retry on failure
        let futures: Vec<_> = assets.iter().map(|(label, query)| {
            let label = label.to_string();
            let query = query.to_string();
            async move {
                for attempt in 0..2 {
                    if attempt > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    }
                    match commands::fetch_market_price_pub(&query).await {
                        Ok(info) => {
                            let brief: String = info.lines().take(4).collect::<Vec<_>>().join(" | ");
                            return format!("• {}: {}", label, brief);
                        }
                        Err(e) => {
                            if attempt == 0 {
                                tracing::warn!("Price fetch failed for '{}' (attempt 1): {}, retrying...", label, e);
                            } else {
                                tracing::warn!("Price fetch failed for '{}' (attempt 2): {}", label, e);
                            }
                        }
                    }
                }
                format!("• {}: 暂无数据", label)
            }
        }).collect();

        let price_data: Vec<String> = futures::future::join_all(futures).await;
        sections.push(format!("【实时行情数据 ({})】\n{}", now, price_data.join("\n")));

        // All search queries in parallel: market news + macro + sentiment
        let all_queries: Vec<(&str, &str)> = vec![
            // Market news (existing)
            ("news", "stock market China A-share news today"),
            ("news", "US tech stocks NASDAQ latest"),
            ("news", "Bitcoin crypto market news"),
            // Macro economy & policy (NEW)
            ("macro", "Federal Reserve PBOC interest rate monetary policy today"),
            ("macro", "China PMI CPI GDP economic data latest 2026"),
            ("macro", "geopolitical trade tariff sanctions impact market"),
            // Market sentiment (NEW)
            ("sentiment", "VIX fear greed index market sentiment today"),
            ("sentiment", "China A-share northbound capital flow margin trading"),
        ];

        let search_futures: Vec<_> = all_queries.iter().map(|(cat, query)| {
            let cat = cat.to_string();
            let query = query.to_string();
            async move {
                match commands::search_web_pub(&query).await {
                    Ok(results) => {
                        let brief: String = results.chars().take(600).collect();
                        Some((cat, format!("[{}]\n{}", query, brief)))
                    }
                    Err(e) => {
                        tracing::warn!("Scheduled search failed for '{}': {}", query, e);
                        None
                    }
                }
            }
        }).collect();

        let search_results = futures::future::join_all(search_futures).await;
        let mut news_data = Vec::new();
        let mut macro_data = Vec::new();
        let mut sentiment_data = Vec::new();
        for item in search_results.into_iter().flatten() {
            match item.0.as_str() {
                "news" => news_data.push(item.1),
                "macro" => macro_data.push(item.1),
                "sentiment" => sentiment_data.push(item.1),
                _ => {}
            }
        }
        if !macro_data.is_empty() {
            sections.push(format!("【宏观经济与政策动态】\n{}", macro_data.join("\n\n")));
        }
        if !news_data.is_empty() {
            sections.push(format!("【市场新闻动态】\n{}", news_data.join("\n\n")));
        }
        if !sentiment_data.is_empty() {
            sections.push(format!("【市场情绪信号】\n{}", sentiment_data.join("\n\n")));
        }
    }

    if is_learning {
        let tech_queries = vec![
            "AI large language model news today",
            "Web3 crypto blockchain latest",
            "robotics automation technology news",
            "tech company Apple Google Tesla news",
        ];
        let tech_futures: Vec<_> = tech_queries.iter().map(|tq| {
            let tq = tq.to_string();
            async move {
                match commands::search_web_pub(&tq).await {
                    Ok(results) => {
                        let brief: String = results.chars().take(500).collect();
                        Some(format!("[{}]\n{}", tq, brief))
                    }
                    Err(e) => {
                        tracing::warn!("Scheduled search failed for '{}': {}", tq, e);
                        None
                    }
                }
            }
        }).collect();

        let tech_results = futures::future::join_all(tech_futures).await;
        let tech_data: Vec<String> = tech_results.into_iter().flatten().collect();
        if !tech_data.is_empty() {
            sections.push(format!("【科技圈最新动态 ({})】\n{}", now, tech_data.join("\n\n")));
        }
    }

    let enrich_elapsed = enrich_start.elapsed();
    tracing::info!("[Enrich] Data pre-fetch completed in {:.1}s ({} sections)", enrich_elapsed.as_secs_f64(), sections.len());
    commands::record_perf_event("enrich", &format!("{} sections", sections.len()), enrich_elapsed.as_millis() as u64);

    if sections.is_empty() {
        return action.to_string();
    }

    format!(
        "{}\n\n以下是预先获取的实时数据，请基于这些数据完成分析，不要编造数据：\n\n{}",
        action,
        sections.join("\n\n")
    )
}

/// Route content to the correct subdirectory based on task name keywords.
pub fn resolve_vault_subdir(knowledge: &config::KnowledgeConfig, task_name: &str) -> String {
    let lower = task_name.to_lowercase();

    // Investment-related
    if lower.contains("投资") || lower.contains("行情") || lower.contains("盘")
        || lower.contains("invest") || lower.contains("理财") || lower.contains("简报") {
        return knowledge.routing.get("invest")
            .cloned().unwrap_or_else(|| "invest-explore".into());
    }
    // Entrepreneurship-related
    if lower.contains("创业") || lower.contains("boss") || lower.contains("startup")
        || lower.contains("副业") {
        return knowledge.routing.get("boss")
            .cloned().unwrap_or_else(|| "boss-explore".into());
    }
    // News / tech / topics
    if lower.contains("日报") || lower.contains("新闻") || lower.contains("科技")
        || lower.contains("选题") || lower.contains("news") || lower.contains("热点") {
        return knowledge.routing.get("news")
            .cloned().unwrap_or_else(|| "hot-news".into());
    }
    // Fallback
    knowledge.routing.get("default")
        .cloned().unwrap_or_else(|| "general".into())
}

/// Save content to the Obsidian vault as a dated markdown file,
/// automatically routed to the correct subdirectory.
pub fn save_to_vault(knowledge: &config::KnowledgeConfig, task_name: &str, content: &str) {
    use std::fs;
    let now = chrono::Local::now();
    let date_str = now.format("%Y-%m-%d").to_string();
    let time_str = now.format("%H%M").to_string();

    let subdir = resolve_vault_subdir(knowledge, task_name);
    let dir = std::path::Path::new(&knowledge.vault_path)
        .join(&subdir)
        .join(&date_str);
    if let Err(e) = fs::create_dir_all(&dir) {
        tracing::warn!("Failed to create vault dir {:?}: {}", dir, e);
        return;
    }

    let safe_name: String = task_name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c > '\x7f' { c } else { '_' })
        .collect();
    let file_path = dir.join(format!("{}-{}.md", time_str, safe_name));

    let tags: Vec<&str> = if subdir.contains("invest") {
        vec!["投资", "日报"]
    } else if subdir.contains("boss") {
        vec!["创业", "灵感"]
    } else if subdir.contains("news") {
        vec!["科技", "热点"]
    } else {
        vec!["auto-crab"]
    };

    let md = format!(
        "---\ntask: {}\ndate: {}\ntime: {}\ncategory: {}\ntags: [{}]\n---\n\n# {}\n\n{}\n",
        task_name, date_str, now.format("%H:%M"), subdir,
        tags.join(", "), task_name, content
    );

    match fs::write(&file_path, &md) {
        Ok(_) => tracing::info!("Saved report to vault: {:?} (category: {})", file_path, subdir),
        Err(e) => tracing::warn!("Failed to write vault file {:?}: {}", file_path, e),
    }
}

fn build_remote_reply(text: &str) -> String {
    let cmd = text.trim();
    if cmd.starts_with("/status") {
        format!(
            "🦀 Auto Crab 在线\n时间: {}\n状态: webhook 正常，远程通道已连通",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        )
    } else if cmd.starts_with("/task") {
        "已收到任务指令。当前版本正在接入任务执行链路，请先在桌面端查看执行结果。".into()
    } else if cmd.starts_with("/approve") || cmd.starts_with("/reject") {
        "已收到审批指令。当前版本正在接入审批回传链路，请先在桌面端审批弹窗操作。".into()
    } else {
        format!(
            "已收到你的消息：{}\n远程会话执行链路正在接入中，可先用 /status 验证连通性。",
            cmd
        )
    }
}

async fn run_remote_chat(
    cfg: &config::AppConfig,
    history: &[crate::models::provider::ChatMessage],
    user_input: &str,
    audit: Option<&std::sync::Arc<security::audit::AuditLogger>>,
) -> anyhow::Result<String> {
    // Disable planner for remote (Feishu) chats: planner splits tasks into
    // sequential LLM calls which multiplies latency on webhook-based channels.
    // The model handles analysis well in a single pass with skill prompts.
    run_remote_chat_inner(cfg, history, user_input, audit, false).await
}

async fn run_remote_chat_no_plan(
    cfg: &config::AppConfig,
    history: &[crate::models::provider::ChatMessage],
    user_input: &str,
    audit: Option<&std::sync::Arc<security::audit::AuditLogger>>,
) -> anyhow::Result<String> {
    run_remote_chat_inner(cfg, history, user_input, audit, false).await
}

async fn run_remote_chat_inner(
    cfg: &config::AppConfig,
    history: &[crate::models::provider::ChatMessage],
    user_input: &str,
    audit: Option<&std::sync::Arc<security::audit::AuditLogger>>,
    planning_enabled: bool,
) -> anyhow::Result<String> {
    use crate::core::engine::*;

    let chat_start = std::time::Instant::now();
    let preview: String = user_input.chars().take(50).collect();
    tracing::info!("[RemoteChat] Start (planning={}, history={}): {}", planning_enabled, history.len(), preview);

    let engine = AgentEngine::from_config(cfg)?;
    let full_prompt = build_full_system_prompt(cfg, Some(user_input));
    let messages = build_messages(&full_prompt, history, user_input);
    let sink = StringCollectorSink;
    let stream_id = uuid::Uuid::new_v4().to_string();

    let memory = create_memory(cfg);
    let agent_cfg = AgentConfig {
        max_rounds: 4,
        tools_enabled: cfg.tools.shell_enabled,
        audit: audit.cloned(),
        audit_source: security::audit::AuditSource::Feishu,
        memory,
        planning_enabled,
    };

    let result = engine.run(messages, &stream_id, &sink, &agent_cfg).await;
    let elapsed = chat_start.elapsed();
    tracing::info!("[RemoteChat] Completed in {:.1}s (result_len={})", elapsed.as_secs_f64(), result.len());
    commands::record_perf_event("remote_chat", &preview, elapsed.as_millis() as u64);
    Ok(result)
}

async fn handle_remote_control_command(
    app: &tauri::AppHandle,
    cfg: &config::AppConfig,
    cmd: &remote::webhook_server::WebhookCommand,
) -> String {
    let session_key = format!("{}:{}", cmd.source, cmd.user_id);
    let conv_state = app.state::<RemoteConversationState>();
    let audit = app.try_state::<std::sync::Arc<security::audit::AuditLogger>>();
    let audit_ref = audit.as_deref();
    match cmd.command_type.as_str() {
        "status" => {
            let sub = cmd.text.trim();
            if sub == "models" {
                let mut lines = vec!["🦀 模型配置:".to_string()];
                if let Some(ref m) = cfg.models.primary {
                    lines.push(format!("  主模型: {} ({})", m.model, m.provider));
                }
                if let Some(ref m) = cfg.models.vision {
                    lines.push(format!("  视觉: {} ({})", m.model, m.provider));
                }
                if let Some(ref m) = cfg.models.coding {
                    lines.push(format!("  编码: {} ({})", m.model, m.provider));
                }
                if let Some(ref m) = cfg.models.fallback {
                    lines.push(format!("  回退: {} ({})", m.model, m.provider));
                }
                lines.join("\n")
            } else {
                format!(
                    "🦀 Auto Crab 在线\n时间: {}\n主模型: {}\n视觉: {}\n工具: {}\n会话: /sessions 查看\n\n/status models — 查看详细模型配置",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    cfg.models.primary.as_ref().map(|m| m.model.as_str()).unwrap_or("未配置"),
                    cfg.models.vision.as_ref().map(|m| m.model.as_str()).unwrap_or("未配置"),
                    if cfg.tools.shell_enabled { "Shell+文件+截图+鼠标键盘" } else { "受限" },
                )
            }
        }
        "chat" => {
            let text = cmd.text.trim();

            if text.eq_ignore_ascii_case("/reset") || text.eq_ignore_ascii_case("/clear")
                || text.eq_ignore_ascii_case("/新对话") || text.eq_ignore_ascii_case("/清空") {
                let active_key = conv_state.get_active_session(&cmd.user_id).await;
                let clear_key = if active_key.is_empty() { &session_key } else { &active_key };
                conv_state.clear(clear_key).await;
                return "✅ 已清空当前会话上下文，可以开始全新对话了。\n\n💡 提示：发送 /help 查看所有可用命令".to_string();
            }

            if text.eq_ignore_ascii_case("/new") {
                let new_name = chrono::Local::now().format("%m%d-%H%M").to_string();
                let new_key = format!("{}:{}:{}", cmd.source, cmd.user_id, new_name);
                conv_state.set_active_session(&cmd.user_id, &new_key).await;
                return format!("✅ 已创建并切换到新会话「{}」\n旧会话内容保留，可通过 /sessions 查看和切换。", new_name);
            }

            if text.eq_ignore_ascii_case("/help") || text.eq_ignore_ascii_case("/帮助") {
                return "🦀 Auto Crab 飞书命令：\n\n\
                    💬 对话管理\n\
                    /clear 或 /新对话 — 清空当前会话上下文\n\
                    /new — 新建一个独立会话（保留旧会话）\n\
                    /sessions — 查看所有会话\n\
                    /session <名称> — 切换到指定会话\n\n\
                    📊 系统\n\
                    /status — 查看系统状态\n\
                    /status models — 查看模型配置\n\n\
                    🔍 监控\n\
                    /monitor <秒数> <内容> — 启动定时监控\n\
                    /monitors — 查看活跃监控\n\
                    /monitor stop <ID> — 停止监控\n\n\
                    🔧 其他\n\
                    /task <描述> — 创建需审批的任务\n\
                    /undo — 撤回最近文件修改\n\n\
                    直接发送文字即可与 AI 对话。".to_string();
            }

            if text.eq_ignore_ascii_case("/undo") {
                let snapshots = crate::core::snapshots::SnapshotStore::new(commands::app_data_dir());
                match snapshots.list(1).await {
                    Ok(list) if !list.is_empty() => {
                        let snap = &list[0];
                        match snapshots.restore(&snap.id).await {
                            Ok(path) => return format!("已撤回文件修改: {}", path),
                            Err(e) => return format!("撤回失败: {}", e),
                        }
                    }
                    _ => return "没有可撤回的操作。".to_string(),
                }
            }

            if text.starts_with("/monitor stop") {
                let monitor_id = text.strip_prefix("/monitor stop").unwrap_or("").trim();
                let monitor = app.state::<MonitorState>();
                if monitor_id.is_empty() {
                    return "用法: /monitor stop <ID>".to_string();
                }
                if monitor.remove(monitor_id).await {
                    return format!("已停止监控: {}", monitor_id);
                } else {
                    return format!("未找到监控任务: {}", monitor_id);
                }
            }

            if text.eq_ignore_ascii_case("/monitors") {
                let monitor = app.state::<MonitorState>();
                let list = monitor.list().await;
                if list.is_empty() {
                    return "当前没有活跃的监控任务。\n用法: /monitor <间隔秒数> <监控内容描述>"
                        .to_string();
                }
                let lines: Vec<String> = list
                    .iter()
                    .map(|(id, desc, interval)| format!("  • [{}] 每{}秒 — {}", id, interval, desc))
                    .collect();
                return format!(
                    "活跃监控任务:\n{}\n\n/monitor stop <ID> 停止",
                    lines.join("\n")
                );
            }

            // Natural language monitor detection: "盯盘BTC 5min", "每5分钟看下ETH", etc.
            if let Some((interval, description)) = parse_natural_monitor(text) {
                let active_key = conv_state.get_active_session(&cmd.user_id).await;
                let final_key = if active_key.is_empty() { session_key.clone() } else { active_key };
                let history = conv_state.get_history(&final_key).await;

                let immediate = match run_remote_chat(cfg, &history, text, audit_ref).await {
                    Ok(answer) => {
                        conv_state.append_turn(&final_key, text, &answer).await;
                        answer
                    }
                    Err(_) => String::new(),
                };

                let monitor_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
                let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);

                let monitor = app.state::<MonitorState>();
                monitor.add(monitor_id.clone(), MonitorTask {
                    description: description.clone(),
                    interval_secs: interval,
                    cancel: cancel_tx,
                }).await;

                let cfg_clone = cfg.clone();
                let feishu_config = cfg.remote.feishu.clone();
                let user_id = cmd.user_id.clone();
                let mid = monitor_id.clone();
                let desc = description.clone();

                tokio::spawn(async move {
                    let mut price_history: Vec<(String, String)> = Vec::new();
                    let max_history = 5;
                    let mut round = 0u32;

                    loop {
                        tokio::select! {
                            _ = tokio::time::sleep(std::time::Duration::from_secs(interval)) => {},
                            _ = cancel_rx.changed() => { break; }
                        }
                        if *cancel_rx.borrow() { break; }
                        round += 1;

                        let queries = extract_monitor_queries(&desc);
                        let mut price_lines = Vec::new();
                        for q in &queries {
                            match commands::fetch_market_price_pub(q).await {
                                Ok(info) => price_lines.push(info),
                                Err(e) => price_lines.push(format!("{}: 获取失败 {}", q, e)),
                            }
                        }

                        let now = chrono::Local::now().format("%H:%M:%S").to_string();
                        let price_text = price_lines.join("\n---\n");

                        let hist_summary = price_history.iter().map(|(ts, a)| {
                            format!("[{}] {}", ts, a.chars().take(80).collect::<String>())
                        }).collect::<Vec<_>>().join("\n");

                        let analysis_prompt = format!(
                            "你是资深金融分析师。用户正在盯盘（第{}轮，每{}秒）。\n\n当前数据:\n{}\n\n{}\n\n请简洁分析：1.关键数据 2.与上轮对比变化 3.趋势判断 4.操作建议。限200字。",
                            round, interval, price_text,
                            if price_history.is_empty() { "首次分析".to_string() } else { format!("历史:\n{}", hist_summary) }
                        );

                        let analysis = match run_remote_chat(&cfg_clone, &[], &analysis_prompt, None).await {
                            Ok(t) => t,
                            Err(_) => format!("📊 第{}轮\n\n{}", round, price_text),
                        };

                        price_history.push((now, price_lines.first().cloned().unwrap_or_default()));
                        if price_history.len() > max_history { price_history.remove(0); }

                        let msg = format!("🔍 [{}] {} (第{}轮)\n\n{}", mid, desc, round, analysis);
                        if let Some(ref fc) = feishu_config {
                            let mut bot = crate::remote::feishu::FeishuBot::new(fc.clone());
                            let _ = bot.send_message(&user_id, &msg).await;
                        }
                    }
                });

                return if immediate.is_empty() {
                    format!("🔍 已启动盯盘监控\nID: {}\n间隔: {}秒\n标的: {}", monitor_id, interval, description)
                } else {
                    format!("{}\n\n---\n🔍 已自动启动盯盘（每{}秒更新）\nID: {}\n发送 /monitor stop {} 停止", immediate, interval, monitor_id, monitor_id)
                };
            }

            if text.starts_with("/monitor ") {
                let rest = text.strip_prefix("/monitor ").unwrap_or("").trim();
                let mut parts = rest.splitn(2, ' ');
                let interval_str = parts.next().unwrap_or("60");
                let description = parts.next().unwrap_or("监控屏幕变化").to_string();
                let interval: u64 = interval_str.parse().unwrap_or(60).max(10);

                let monitor_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
                let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);

                let task = MonitorTask {
                    description: description.clone(),
                    interval_secs: interval,
                    cancel: cancel_tx,
                };

                let monitor = app.state::<MonitorState>();
                monitor.add(monitor_id.clone(), task).await;

                let cfg_clone = cfg.clone();
                let feishu_config = cfg.remote.feishu.clone();
                let user_id = cmd.user_id.clone();
                let mid = monitor_id.clone();
                let desc = description.clone();

                tokio::spawn(async move {
                    let mut history: Vec<(String, String)> = Vec::new(); // (timestamp, analysis)
                    let max_history = 5;
                    let mut round = 0u32;

                    loop {
                        tokio::select! {
                            _ = tokio::time::sleep(std::time::Duration::from_secs(interval)) => {},
                            _ = cancel_rx.changed() => {
                                tracing::info!("Monitor {} cancelled", mid);
                                break;
                            }
                        }
                        if *cancel_rx.borrow() {
                            break;
                        }
                        round += 1;

                        tracing::info!("Monitor {} round {}: {}", mid, round, desc);

                        // Try unified market data API for financial assets
                        let queries = extract_monitor_queries(&desc);
                        if !queries.is_empty() {
                            let mut price_lines = Vec::new();
                            for q in &queries {
                                match commands::fetch_market_price_pub(q).await {
                                    Ok(info) => price_lines.push(info),
                                    Err(e) => price_lines.push(format!("{}: 获取失败 {}", q, e)),
                                }
                            }

                            let now = chrono::Local::now().format("%H:%M:%S").to_string();
                            let price_text = price_lines.join("\n---\n");

                            let history_summary = history.iter().map(|(ts, a)| {
                                format!("[{}] {}", ts, a.chars().take(80).collect::<String>())
                            }).collect::<Vec<_>>().join("\n");

                            let analysis = if history.is_empty() {
                                format!("📊 第{}轮监控\n\n{}", round, price_text)
                            } else {
                                format!("📊 第{}轮监控\n\n{}\n\n历史趋势:\n{}", round, price_text, history_summary)
                            };

                            history.push((now, price_lines.first().cloned().unwrap_or_default()));
                            if history.len() > max_history { history.remove(0); }

                            let msg = format!("🔍 [{}] {}\n\n{}", mid, desc, analysis);
                            if let Some(ref fc) = feishu_config {
                                let mut bot = crate::remote::feishu::FeishuBot::new(fc.clone());
                                let _ = bot.send_message(&user_id, &msg).await;
                            }
                            continue;
                        }

                        let tmp = format!(
                            "{}\\AppData\\Local\\Temp\\auto-crab-monitor-{}.png",
                            std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into()),
                            mid,
                        );
                        match commands::take_screenshot_sync(&tmp) {
                            Ok(_) => {
                                let history_context = if history.is_empty() {
                                    "这是首次监控分析，没有历史数据。".to_string()
                                } else {
                                    let entries: Vec<String> = history
                                        .iter()
                                        .map(|(ts, a)| {
                                            format!(
                                                "[{}] {}",
                                                ts,
                                                a.chars().take(150).collect::<String>()
                                            )
                                        })
                                        .collect();
                                    format!(
                                        "以下是最近{}次监控记录，请结合历史走势进行对比分析：\n{}",
                                        entries.len(),
                                        entries.join("\n")
                                    )
                                };

                                let monitor_prompt = format!(
"你是一位资深投资交易分析师。用户正在持续监控：「{desc}」（第{round}轮，每{interval}秒一次）。

{history_context}

请根据当前截图，只关注与监控主题相关的内容（忽略浏览器UI、任务栏等），输出：
1. 当前关键数据（价格/涨跌幅/成交量等）
2. 与上次对比的变化（价格变动方向、幅度）
3. K线形态和趋势判断
4. 操作建议（持有/加仓/减仓/观望，附理由）
5. 风险预警（如有异常波动或关键支撑位/阻力位突破）

如果不是交易界面，请简洁描述与监控主题相关的变化。
限300字以内。",
                                );
                                match commands::analyze_screenshot_with_prompt(
                                    &cfg_clone,
                                    &tmp,
                                    &monitor_prompt,
                                )
                                .await
                                {
                                    Ok(analysis) => {
                                        let now =
                                            chrono::Local::now().format("%H:%M:%S").to_string();
                                        let msg = format!(
                                            "🔍 [{}] {} (第{}轮)\n\n{}",
                                            mid, desc, round, analysis
                                        );

                                        history.push((now, analysis));
                                        if history.len() > max_history {
                                            history.remove(0);
                                        }

                                        if let Some(ref fc) = feishu_config {
                                            let mut bot =
                                                crate::remote::feishu::FeishuBot::new(fc.clone());
                                            let _ = bot.send_message(&user_id, &msg).await;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!("Monitor {} analysis failed: {}", mid, e);
                                    }
                                }
                            }
                            Err(e) => tracing::warn!("Monitor {} screenshot failed: {}", mid, e),
                        }
                    }
                });

                return format!(
                    "🔍 监控已启动\nID: {}\n间隔: {}秒\n内容: {}\n\n/monitor stop {} 停止\n/monitors 查看所有",
                    monitor_id, interval, description, monitor_id
                );
            }

            if text.eq_ignore_ascii_case("/sessions") {
                let list = conv_state.list_sessions(&cmd.user_id).await;
                if list.is_empty() {
                    return "当前没有活跃会话。发送任意消息开始默认会话。".to_string();
                }
                let active = conv_state.get_active_session(&cmd.user_id).await;
                let lines: Vec<String> = list
                    .iter()
                    .map(|s| {
                        let name = s.rsplit(':').next().unwrap_or(s);
                        if *s == active {
                            format!("  ▶ {} (当前)", name)
                        } else {
                            format!("  • {}", name)
                        }
                    })
                    .collect();
                return format!("活跃会话列表:\n{}", lines.join("\n"));
            }

            if text.starts_with("/session ") {
                let session_name = text.strip_prefix("/session ").unwrap_or("").trim();
                if session_name.is_empty() {
                    return "用法: /session <名称> — 切换到指定会话\n/sessions — 查看所有会话\n/reset — 清空当前会话".to_string();
                }
                let new_key = format!("{}:{}:{}", cmd.source, cmd.user_id, session_name);
                conv_state.set_active_session(&cmd.user_id, &new_key).await;
                return format!("已切换到会话「{}」", session_name);
            }

            let active_key = conv_state.get_active_session(&cmd.user_id).await;
            let final_key = if active_key.is_empty() {
                session_key.clone()
            } else {
                active_key
            };

            let history = conv_state.get_history(&final_key).await;
            match run_remote_chat(cfg, &history, text, audit_ref).await {
                Ok(answer) => {
                    conv_state.append_turn(&final_key, text, &answer).await;
                    answer
                }
                Err(e) => format!("远程对话失败: {}", e),
            }
        }
        "task_create" => {
            let state = app.state::<commands::ApprovalState>();
            let task_text = cmd.text.trim();
            if task_text.is_empty() {
                return "用法：/task <任务描述>".to_string();
            }
            let approval =
                commands::create_remote_task_approval(app, &state, &cmd.user_id, task_text);
            format!(
                "🟡 任务已进入审批队列\nID: {}\n任务: {}\n\n发送 /approve {} 执行\n发送 /reject {} 拒绝",
                approval.id, task_text, approval.id, approval.id
            )
        }
        "approve" => {
            let approval_id = cmd.text.split_whitespace().next().unwrap_or("");
            if approval_id.is_empty() {
                "用法：/approve <审批ID>".to_string()
            } else {
                let state = app.state::<commands::ApprovalState>();
                match commands::approve_operation_internal(&state, approval_id) {
                    Ok(approval) => {
                        if approval.operation == "remote_task" {
                            let task_text = approval
                                .details
                                .get("task_text")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            if task_text.is_empty() {
                                return format!("审批 {} 已通过，但任务内容为空", approval_id);
                            }

                            let history = conv_state.get_history(&session_key).await;
                            match run_remote_chat(cfg, &history, &task_text, audit_ref).await {
                                Ok(answer) => {
                                    conv_state
                                        .append_turn(&session_key, &task_text, &answer)
                                        .await;
                                    format!(
                                        "✅ 审批已通过并执行任务\nID: {}\n\n{}",
                                        approval_id, answer
                                    )
                                }
                                Err(e) => format!(
                                    "✅ 审批已通过，但任务执行失败\nID: {}\n错误: {}",
                                    approval_id, e
                                ),
                            }
                        } else {
                            format!("已处理审批：{}", approval_id)
                        }
                    }
                    Err(err) => format!("审批失败：{}", err),
                }
            }
        }
        "reject" => {
            let mut parts = cmd.text.splitn(2, ' ');
            let approval_id = parts.next().unwrap_or("").trim();
            let reject_reason = parts.next().unwrap_or("from_feishu").trim().to_string();
            if approval_id.is_empty() {
                "用法：/reject <审批ID>".to_string()
            } else {
                let state = app.state::<commands::ApprovalState>();
                match commands::reject_operation_internal(&state, approval_id) {
                    Ok(_) => format!("已拒绝审批：{}\n原因：{}", approval_id, reject_reason),
                    Err(err) => format!("拒绝失败：{}", err),
                }
            }
        }
        "task_cancel" => "已收到取消任务指令。当前版本将在后续接入任务队列后生效。".to_string(),
        _ => build_remote_reply(&cmd.text),
    }
}

/// Run Auto-Crab as an MCP server on stdio.
/// Used when launched with `--mcp-server` flag.
pub async fn run_mcp_server() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("auto_crab=info")
        .with_target(false)
        .init();
    mcp::server::run_stdio_server().await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("auto_crab=info".parse().unwrap()),
        )
        .with_target(false)
        .init();

    tracing::info!("Auto Crab starting...");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(commands::ApprovalState::default())
        .manage(RemoteConversationState::default())
        .manage(MonitorState::default())
        .manage(std::sync::Arc::new(security::audit::AuditLogger::new(commands::app_data_dir())))
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::chat_send,
            commands::chat_stream_start,
            commands::list_models,
            commands::get_audit_log,
            commands::approve_operation,
            commands::reject_operation,
            commands::list_pending_approvals,
            commands::store_credential,
            commands::delete_credential,
            commands::check_credentials,
            commands::get_credential_preview,
            commands::get_risk_level,
            commands::save_conversation,
            commands::load_conversation,
            commands::list_conversations,
            commands::delete_conversation,
            commands::list_skills,
            commands::save_skill,
            commands::delete_skill,
            commands::rename_skill_cmd,
            commands::get_skills_dir,
            commands::get_search_usage_stats,
            commands::save_to_knowledge_base,
            commands::get_mcp_status,
            commands::get_perf_metrics,
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Initialize unified app data dir from Tauri (synchronous, before async tasks)
            if let Ok(data_dir) = app.path().app_data_dir() {
                commands::init_app_data_dir(data_dir);
            }

            // Initialize config + webhook server
            let handle_clone = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = config::ensure_config_dir(&handle_clone).await {
                    tracing::error!("Failed to initialize config directory: {}", e);
                }

                // Start webhook server if remote control is enabled
                match config::load_config(&handle_clone).await {
                    Ok(cfg) => {
                        // Initialize search API config globally
                        commands::update_search_config(&cfg.search);

                        // Load persisted perf events from disk
                        commands::load_perf_events();

                        // Initialize MCP client if enabled
                        if cfg.mcp.client_enabled && !cfg.mcp.servers.is_empty() {
                            let mcp_mgr = std::sync::Arc::new(mcp::client::McpClientManager::new());
                            let errors = mcp_mgr.connect_all(&cfg.mcp.servers).await;
                            if !errors.is_empty() {
                                for e in &errors {
                                    tracing::warn!("[MCP] {}", e);
                                }
                            }
                            let status = mcp_mgr.status().await;
                            let total_tools: usize = status.iter().map(|(_, c)| c).sum();
                            tracing::info!("[MCP] Client initialized: {} servers, {} tools total", status.len(), total_tools);
                            handle_clone.manage(mcp_mgr);
                        }

                        if cfg.remote.enabled {
                            let (tx, mut rx) = tokio::sync::mpsc::channel(32);
                            let server = remote::webhook_server::WebhookServer::new(&cfg, tx);
                            if let Err(e) = server.start().await {
                                tracing::error!("Failed to start webhook server: {}", e);
                            } else {
                                tracing::info!("Webhook server started on port 18790");
                            }
                            let mut feishu_bot = cfg
                                .remote
                                .feishu
                                .as_ref()
                                .map(|c| remote::feishu::FeishuBot::new(c.clone()));
                            let mut wechat_work_bot = cfg
                                .remote
                                .wechat_work
                                .as_ref()
                                .map(|c| remote::wechat_work::WechatWorkBot::new(c.clone()));
                            // Process incoming commands in background
                            let cfg_for_remote = cfg.clone();
                            tokio::spawn(async move {
                                while let Some(cmd) = rx.recv().await {
                                    tracing::info!(
                                        "Remote command from {}: {} [{}] -> {}",
                                        cmd.source,
                                        cmd.user_id,
                                        cmd.command_type,
                                        cmd.text
                                    );
                                    let reply = handle_remote_control_command(
                                        &handle_clone,
                                        &cfg_for_remote,
                                        &cmd,
                                    )
                                    .await;

                                    match cmd.source.as_str() {
                                        "feishu" => {
                                            if let Some(bot) = feishu_bot.as_mut() {
                                                if let Err(e) = bot.send_message(&cmd.user_id, &reply).await {
                                                    tracing::warn!("Failed to reply Feishu message to {}: {}", cmd.user_id, e);
                                                }
                                            }
                                        }
                                        "wechat_work" => {
                                            if let Some(bot) = wechat_work_bot.as_mut() {
                                                if let Err(e) = bot.send_message(&cmd.user_id, &reply).await {
                                                    tracing::warn!("Failed to reply WeChat Work message to {}: {}", cmd.user_id, e);
                                                }
                                            }
                                        }
                                        _ => {
                                            tracing::warn!("Unknown remote source: {}", cmd.source);
                                        }
                                    }
                                }
                            });
                        }

                        // Start task scheduler if enabled
                        if cfg.scheduled_tasks.enabled && !cfg.scheduled_tasks.jobs.is_empty() {
                            let mut scheduler = core::scheduler::TaskScheduler::new(
                                cfg.scheduled_tasks.jobs.clone(),
                                cfg.scheduled_tasks.require_confirmation,
                            );
                            let sched_cfg = cfg.clone();
                            let sched_feishu = cfg.remote.feishu.clone();
                            let sched_user = cfg.remote.feishu.as_ref()
                                .and_then(|f| f.allowed_user_ids.first().cloned())
                                .unwrap_or_default();

                            tokio::spawn(async move {
                                tracing::info!("TaskScheduler started with {} jobs", sched_cfg.scheduled_tasks.jobs.len());
                                loop {
                                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                                    let due = scheduler.check_due_jobs();
                                    for job in due {
                                        tracing::info!("Scheduled task triggered: {} (auto={})", job.name, job.auto_execute);
                                        if job.auto_execute {
                                            let sched_start = std::time::Instant::now();
                                            let enriched_action = enrich_scheduled_action(&job.action).await;
                                            let result = run_remote_chat_no_plan(&sched_cfg, &[], &enriched_action, None).await;
                                            let sched_elapsed = sched_start.elapsed();
                                            commands::record_perf_event("scheduled_task", &job.name, sched_elapsed.as_millis() as u64);
                                            let msg = match result {
                                                Ok(ref text) => format!("📋 {}\n\n{}", job.name, text),
                                                Err(ref e) => format!("📋 {} 执行失败\n{}", job.name, e),
                                            };

                                            // Save to Obsidian vault if configured
                                            if sched_cfg.knowledge.enabled && !sched_cfg.knowledge.vault_path.is_empty() {
                                                if let Ok(ref text) = result {
                                                    save_to_vault(&sched_cfg.knowledge, &job.name, text);
                                                }
                                            }

                                            if let Some(ref fc) = sched_feishu {
                                                let mut bot = remote::feishu::FeishuBot::new(fc.clone());
                                                let _ = bot.send_message(&sched_user, &msg).await;
                                            }
                                        } else {
                                            if let Some(ref fc) = sched_feishu {
                                                let mut bot = remote::feishu::FeishuBot::new(fc.clone());
                                                let msg = format!("📋 定时任务待确认: {}\n\n内容: {}\n\n回复 /approve 执行", job.name, job.action.chars().take(200).collect::<String>());
                                                let _ = bot.send_message(&sched_user, &msg).await;
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    }
                    Err(e) => tracing::warn!("Could not load config for webhook: {}", e),
                }
            });

            // System tray
            let show_item = MenuItemBuilder::with_id("show", "显示窗口").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "退出 Auto Crab").build(app)?;
            let tray_menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .menu(&tray_menu)
                .tooltip("Auto Crab - AI Desktop Assistant")
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            tracing::info!("System tray initialized");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Auto Crab");
}
