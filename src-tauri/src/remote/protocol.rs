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
pub fn validate_remote_user(
    user_id: &str,
    allowed_ids: &[String],
    source: &RemoteSource,
) -> bool {
    if allowed_ids.is_empty() {
        tracing::warn!(
            "Remote control from {:?} has no user allowlist configured - rejecting all",
            source,
        );
        return false;
    }
    allowed_ids.iter().any(|id| id == user_id)
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
