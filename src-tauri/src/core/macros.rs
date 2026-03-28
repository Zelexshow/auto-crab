use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionMacro {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps: Vec<MacroStep>,
    pub created_at: String,
    pub use_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroStep {
    pub action: String,
    #[serde(default)]
    pub target_window: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub delay_ms: u64,
    #[serde(default)]
    pub element_name: String,
}

pub struct MacroStore {
    dir: PathBuf,
}

impl MacroStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { dir: data_dir.join("macros") }
    }

    pub async fn save(&self, m: &ActionMacro) -> Result<()> {
        fs::create_dir_all(&self.dir).await?;
        let path = self.dir.join(format!("{}.json", m.id));
        let json = serde_json::to_string_pretty(m)?;
        fs::write(path, json).await?;
        Ok(())
    }

    pub async fn load(&self, id: &str) -> Result<ActionMacro> {
        let path = self.dir.join(format!("{}.json", id));
        let json = fs::read_to_string(path).await?;
        Ok(serde_json::from_str(&json)?)
    }

    pub async fn list(&self) -> Result<Vec<ActionMacro>> {
        let mut macros = Vec::new();
        if !self.dir.exists() { return Ok(macros); }
        let mut entries = fs::read_dir(&self.dir).await?;
        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(json) = fs::read_to_string(entry.path()).await {
                    if let Ok(m) = serde_json::from_str::<ActionMacro>(&json) {
                        macros.push(m);
                    }
                }
            }
        }
        macros.sort_by(|a, b| b.use_count.cmp(&a.use_count));
        Ok(macros)
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let path = self.dir.join(format!("{}.json", id));
        fs::remove_file(path).await?;
        Ok(())
    }

    pub async fn increment_use(&self, id: &str) -> Result<()> {
        if let Ok(mut m) = self.load(id).await {
            m.use_count += 1;
            self.save(&m).await?;
        }
        Ok(())
    }
}

pub fn create_wechat_reply_macro(contact: &str, message: &str) -> ActionMacro {
    ActionMacro {
        id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
        name: format!("微信回复{}", contact),
        description: format!("给微信联系人 {} 发送消息", contact),
        steps: vec![
            MacroStep {
                action: "focus_window".into(),
                target_window: "微信".into(),
                delay_ms: 500,
                ..Default::default()
            },
            MacroStep {
                action: "search_and_click".into(),
                element_name: contact.into(),
                delay_ms: 500,
                ..Default::default()
            },
            MacroStep {
                action: "click_input".into(),
                element_name: "输入框".into(),
                delay_ms: 300,
                ..Default::default()
            },
            MacroStep {
                action: "type".into(),
                text: message.into(),
                delay_ms: 300,
                ..Default::default()
            },
            MacroStep {
                action: "key_press".into(),
                key: "enter".into(),
                delay_ms: 500,
                ..Default::default()
            },
        ],
        created_at: Utc::now().to_rfc3339(),
        use_count: 0,
    }
}

impl Default for MacroStep {
    fn default() -> Self {
        Self {
            action: String::new(),
            target_window: String::new(),
            x: 0, y: 0,
            text: String::new(),
            key: String::new(),
            delay_ms: 300,
            element_name: String::new(),
        }
    }
}

pub async fn execute_macro(m: &ActionMacro) -> Result<String> {
    let mut results = Vec::new();

    for (i, step) in m.steps.iter().enumerate() {
        tokio::time::sleep(std::time::Duration::from_millis(step.delay_ms)).await;

        let result = match step.action.as_str() {
            "focus_window" => {
                tokio::task::spawn_blocking({
                    let title = step.target_window.clone();
                    move || crate::tools::ui_automation::focus_window_by_title(&title)
                }).await??
            }
            "type" => {
                crate::commands::do_keyboard_type_pub(&step.text).await?
            }
            "key_press" => {
                crate::commands::do_key_press_pub(&step.key).await?
            }
            "click" => {
                crate::commands::do_mouse_click_pub(step.x, step.y, "left").await?
            }
            "search_and_click" | "click_input" => {
                format!("需要 analyze_and_act 定位 '{}'", step.element_name)
            }
            _ => format!("未知操作: {}", step.action),
        };
        results.push(format!("步骤{}: {} → {}", i + 1, step.action, result));
    }

    Ok(results.join("\n"))
}
