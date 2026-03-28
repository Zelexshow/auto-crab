use serde::{Deserialize, Serialize};

/// Unified remote command protocol.
/// All remote channels (Feishu, WeChat Work) parse incoming messages
/// into this common format before dispatching to the Agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteCommand {
    pub source: RemoteSource,
    pub user_id: String,
    pub command_type: CommandType,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RemoteSource {
    Feishu,
    WechatWork,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandType {
    Chat,
    StatusQuery,
    TaskCreate,
    TaskCancel,
    ApproveAction,
    RejectAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteResponse {
    pub content: String,
    pub is_error: bool,
}

/// Security: Validate that the user is allowed to send remote commands
pub fn validate_remote_user(user_id: &str, allowed_ids: &[String], source: &RemoteSource) -> bool {
    if allowed_ids.is_empty() {
        tracing::warn!(
            "Remote control from {:?} has no user allowlist configured - rejecting all",
            source,
        );
        return false;
    }
    allowed_ids.iter().any(|id| id == user_id)
}

/// Convert Markdown to chat-app-friendly plain text.
/// Strips **bold**, *italic*, `code`, ~~strike~~, converts headings/lists/quotes.
pub fn markdown_to_plain(md: &str) -> String {
    let mut out = String::with_capacity(md.len());

    for line in md.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("### ") {
            out.push_str(&format!("【{}】", strip_inline_md(rest.trim())));
            out.push('\n');
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            out.push_str(&format!("【{}】", strip_inline_md(rest.trim())));
            out.push('\n');
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            out.push_str(&format!("【{}】", strip_inline_md(rest.trim())));
            out.push('\n');
            continue;
        }

        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            out.push('\n');
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("- ") {
            out.push_str(&format!("  • {}", strip_inline_md(rest)));
            out.push('\n');
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("* ") {
            out.push_str(&format!("  • {}", strip_inline_md(rest)));
            out.push('\n');
            continue;
        }

        // Ordered list: 1. item
        if trimmed.len() > 2 {
            let mut chars = trimmed.chars();
            if let Some(c) = chars.next() {
                if c.is_ascii_digit() {
                    let rest_str: String = chars.collect();
                    if let Some(content) = rest_str.strip_prefix(". ") {
                        out.push_str(&format!("  {}. {}", c, strip_inline_md(content)));
                        out.push('\n');
                        continue;
                    }
                }
            }
        }

        if let Some(rest) = trimmed.strip_prefix("> ") {
            out.push_str(&format!("  ｜{}", strip_inline_md(rest)));
            out.push('\n');
            continue;
        }

        out.push_str(&strip_inline_md(line));
        out.push('\n');
    }

    while out.ends_with("\n\n\n") {
        out.pop();
    }
    out.trim_end().to_string()
}

fn strip_inline_md(text: &str) -> String {
    let mut s = text.to_string();

    // ***bold+italic***
    while let Some(start) = s.find("***") {
        if let Some(end) = s[start + 3..].find("***") {
            s = format!("{}{}{}", &s[..start], &s[start + 3..start + 3 + end], &s[start + 6 + end..]);
        } else {
            break;
        }
    }

    // **bold**
    while let Some(start) = s.find("**") {
        if let Some(end) = s[start + 2..].find("**") {
            s = format!("{}{}{}", &s[..start], &s[start + 2..start + 2 + end], &s[start + 4 + end..]);
        } else {
            break;
        }
    }

    // *italic* (only clear inline patterns)
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] != ' ' && chars[i + 1] != '*' {
            if let Some(end) = s[i + 1..].find('*') {
                let inner = &s[i + 1..i + 1 + end];
                if !inner.contains(' ') || inner.len() < 30 {
                    result.push_str(inner);
                    i += 2 + end;
                    continue;
                }
            }
        }
        result.push(chars[i]);
        i += 1;
    }
    s = result;

    // `code`
    while let Some(start) = s.find('`') {
        if let Some(end) = s[start + 1..].find('`') {
            s = format!("{}{}{}", &s[..start], &s[start + 1..start + 1 + end], &s[start + 2 + end..]);
        } else {
            break;
        }
    }

    // ~~strike~~
    while let Some(start) = s.find("~~") {
        if let Some(end) = s[start + 2..].find("~~") {
            s = format!("{}{}{}", &s[..start], &s[start + 2..start + 2 + end], &s[start + 4 + end..]);
        } else {
            break;
        }
    }

    s
}

/// Parse a text message into a RemoteCommand
pub fn parse_command(text: &str, user_id: &str, source: RemoteSource) -> RemoteCommand {
    let text = text.trim();

    let (command_type, content) = if text.starts_with("/status") {
        (CommandType::StatusQuery, text[7..].trim().to_string())
    } else if text.starts_with("/task") {
        (CommandType::TaskCreate, text[5..].trim().to_string())
    } else if text.starts_with("/cancel") {
        (CommandType::TaskCancel, text[7..].trim().to_string())
    } else if text.starts_with("/approve") {
        (CommandType::ApproveAction, text[8..].trim().to_string())
    } else if text.starts_with("/reject") {
        (CommandType::RejectAction, text[7..].trim().to_string())
    } else {
        (CommandType::Chat, text.to_string())
    };

    RemoteCommand {
        source,
        user_id: user_id.to_string(),
        command_type,
        content,
        timestamp: chrono::Utc::now(),
    }
}
