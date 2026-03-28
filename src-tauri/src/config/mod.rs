mod schema;

pub use schema::*;

use anyhow::Result;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tokio::fs;

const CONFIG_FILE: &str = "auto-crab.toml";
const DEFAULT_CONFIG: &str = include_str!("../../defaults/auto-crab.default.toml");
const SKILLS_DIR: &str = "skills";

pub fn config_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .expect("failed to resolve app config dir")
}

pub fn config_path(app: &AppHandle) -> PathBuf {
    config_dir(app).join(CONFIG_FILE)
}

pub fn skills_dir(app: &AppHandle) -> PathBuf {
    config_dir(app).join(SKILLS_DIR)
}

pub async fn ensure_config_dir(app: &AppHandle) -> Result<()> {
    let dir = config_dir(app);
    fs::create_dir_all(&dir).await?;
    fs::create_dir_all(dir.join(SKILLS_DIR)).await?;

    let path = dir.join(CONFIG_FILE);
    if !path.exists() {
        fs::write(&path, DEFAULT_CONFIG).await?;
        tracing::info!("Created default config at {:?}", path);
    }

    migrate_skills_from_toml(&path, &dir.join(SKILLS_DIR)).await;

    Ok(())
}

/// One-time migration: if TOML still contains [[agent.skills]], move them to files.
async fn migrate_skills_from_toml(toml_path: &std::path::Path, skills_path: &std::path::Path) {
    let Ok(content) = fs::read_to_string(toml_path).await else { return };
    let Ok(mut val) = content.parse::<toml::Table>() else { return };

    let Some(agent) = val.get_mut("agent").and_then(|v| v.as_table_mut()) else { return };
    let Some(skills_val) = agent.remove("skills") else { return };
    let Some(skills_arr) = skills_val.as_array() else { return };

    let mut migrated = 0usize;
    for item in skills_arr {
        let Some(tbl) = item.as_table() else { continue };
        let Some(name) = tbl.get("name").and_then(|v| v.as_str()) else { continue };
        let Some(content) = tbl.get("content").and_then(|v| v.as_str()) else { continue };
        let file_name = sanitize_skill_filename(name);
        let dest = skills_path.join(&file_name);
        if !dest.exists() {
            let _ = fs::write(&dest, content).await;
            migrated += 1;
        }
    }

    if migrated > 0 {
        let new_toml = toml::to_string_pretty(&val).unwrap_or(content);
        let _ = fs::write(toml_path, new_toml).await;
        tracing::info!("Migrated {} skills from TOML to files", migrated);
    }
}

fn sanitize_skill_filename(name: &str) -> String {
    let safe: String = name.chars().map(|c| {
        if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c > '\x7f' {
            c
        } else {
            '_'
        }
    }).collect();
    if safe.is_empty() { "untitled.md".to_string() } else { format!("{}.md", safe) }
}

pub async fn load_config(app: &AppHandle) -> Result<AppConfig> {
    let path = config_path(app);
    let content = fs::read_to_string(&path).await?;
    let mut config: AppConfig = toml::from_str(&content)?;
    config.validate()?;

    config.agent.skills = load_skills_from_dir(&skills_dir(app)).await;

    Ok(config)
}

pub async fn save_config(app: &AppHandle, config: &AppConfig) -> Result<()> {
    config.validate()?;

    let mut config_to_save = config.clone();
    let skills_to_write = std::mem::take(&mut config_to_save.agent.skills);

    let path = config_path(app);
    let content = toml::to_string_pretty(&config_to_save)?;
    fs::write(&path, content).await?;

    let sdir = skills_dir(app);
    write_skills_to_dir(&sdir, &skills_to_write).await?;

    tracing::info!("Config saved to {:?}, {} skills to {:?}", path, skills_to_write.len(), sdir);
    Ok(())
}

// ─── Skills file I/O ───

pub async fn load_skills_from_dir(dir: &std::path::Path) -> Vec<UserSkill> {
    let mut skills = Vec::new();
    let Ok(mut entries) = fs::read_dir(dir).await else { return skills };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") { continue; }
        let name = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .to_string();
        if let Ok(raw) = fs::read_to_string(&path).await {
            let skill = parse_skill_file(&name, &raw);
            skills.push(skill);
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// Parse a skill .md file. Supports optional YAML-like frontmatter:
/// ```
/// ---
/// keywords: 投资, 股票, A股, 行情
/// always_on: false
/// ---
/// (skill content here)
/// ```
fn parse_skill_file(name: &str, raw: &str) -> UserSkill {
    let trimmed = raw.trim();
    if !trimmed.starts_with("---") {
        return UserSkill {
            name: name.to_string(),
            content: raw.to_string(),
            keywords: auto_extract_keywords(name, raw),
            always_on: false,
        };
    }

    let after_first = &trimmed[3..];
    let Some(end_pos) = after_first.find("---") else {
        return UserSkill {
            name: name.to_string(),
            content: raw.to_string(),
            keywords: auto_extract_keywords(name, raw),
            always_on: false,
        };
    };

    let frontmatter = &after_first[..end_pos];
    let body = after_first[end_pos + 3..].trim_start().to_string();

    let mut keywords = Vec::new();
    let mut always_on = false;

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("keywords:") {
            keywords = val.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        } else if let Some(val) = line.strip_prefix("always_on:") {
            always_on = val.trim() == "true";
        }
    }

    if keywords.is_empty() {
        keywords = auto_extract_keywords(name, &body);
    }

    UserSkill { name: name.to_string(), content: body, keywords, always_on }
}

/// Auto-extract keywords from skill name and content headings.
fn auto_extract_keywords(name: &str, content: &str) -> Vec<String> {
    let mut kw = vec![name.to_string()];

    let keyword_patterns: &[(&str, &[&str])] = &[
        ("投资", &["投资", "股票", "A股", "港股", "美股", "行情", "持仓", "分析师", "盘", "策略"]),
        ("科技", &["科技", "AI", "技术", "大模型", "机器人", "芯片", "Web3"]),
        ("加密", &["加密", "币", "BTC", "ETH", "crypto"]),
        ("代码", &["代码", "编程", "开发", "bug", "review", "重构"]),
        ("风格", &["风格", "简洁", "格式", "回复"]),
        ("日报", &["日报", "早报", "周报", "报告", "总结", "复盘"]),
        ("学习", &["学习", "计划", "职业", "成长"]),
    ];

    let combined = format!("{} {}", name, content.chars().take(200).collect::<String>());
    for (_, patterns) in keyword_patterns {
        for &pat in *patterns {
            if combined.contains(pat) && !kw.contains(&pat.to_string()) {
                kw.push(pat.to_string());
            }
        }
    }
    kw
}

/// Serialize a skill to .md with frontmatter.
fn serialize_skill_file(skill: &UserSkill) -> String {
    let has_meta = !skill.keywords.is_empty() || skill.always_on;
    if !has_meta {
        return skill.content.clone();
    }
    let mut out = String::from("---\n");
    if !skill.keywords.is_empty() {
        out.push_str(&format!("keywords: {}\n", skill.keywords.join(", ")));
    }
    if skill.always_on {
        out.push_str("always_on: true\n");
    }
    out.push_str("---\n");
    out.push_str(&skill.content);
    out
}

async fn write_skills_to_dir(dir: &std::path::Path, skills: &[UserSkill]) -> Result<()> {
    fs::create_dir_all(dir).await?;

    let mut existing = std::collections::HashSet::new();
    if let Ok(mut entries) = fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("md") {
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    existing.insert(name.to_string());
                }
            }
        }
    }

    let mut written = std::collections::HashSet::new();
    for skill in skills {
        let filename = sanitize_skill_filename(&skill.name);
        let file_content = serialize_skill_file(skill);
        fs::write(dir.join(&filename), &file_content).await?;
        written.insert(filename);
    }

    for old_file in existing.difference(&written) {
        let _ = fs::remove_file(dir.join(old_file)).await;
    }

    Ok(())
}

pub async fn save_single_skill(app: &AppHandle, skill: &UserSkill) -> Result<()> {
    let dir = skills_dir(app);
    fs::create_dir_all(&dir).await?;
    let filename = sanitize_skill_filename(&skill.name);
    let file_content = serialize_skill_file(skill);
    fs::write(dir.join(&filename), &file_content).await?;
    Ok(())
}

pub async fn delete_single_skill(app: &AppHandle, name: &str) -> Result<()> {
    let dir = skills_dir(app);
    let filename = sanitize_skill_filename(name);
    let path = dir.join(&filename);
    if path.exists() {
        fs::remove_file(&path).await?;
    }
    Ok(())
}

pub async fn rename_skill(app: &AppHandle, old_name: &str, new_name: &str) -> Result<()> {
    let dir = skills_dir(app);
    let old_file = dir.join(sanitize_skill_filename(old_name));
    let new_file = dir.join(sanitize_skill_filename(new_name));
    if old_file.exists() {
        let content = fs::read_to_string(&old_file).await?;
        fs::write(&new_file, content).await?;
        fs::remove_file(&old_file).await?;
    }
    Ok(())
}
