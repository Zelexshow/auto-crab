use crate::models::provider::{ChatMessage, MessageRole};
use std::collections::HashMap;

pub struct ContextManager {
    messages: Vec<ChatMessage>,
    max_tokens: usize,
}

impl ContextManager {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_tokens,
        }
    }

    pub fn add_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
        self.trim_if_needed();
    }

    pub fn build_messages(&self, system_prompt: &str) -> Vec<ChatMessage> {
        let mut result = vec![ChatMessage {
            role: MessageRole::System,
            content: system_prompt.to_string(),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }];
        result.extend(self.messages.clone());
        result
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    fn estimate_tokens(&self) -> usize {
        self.messages.iter().map(|m| m.content.len() / 3).sum()
    }

    fn trim_if_needed(&mut self) {
        while self.estimate_tokens() > self.max_tokens && self.messages.len() > 2 {
            self.messages.remove(0);
        }
    }

    pub fn get_history(&self) -> &[ChatMessage] {
        &self.messages
    }
}

// ── Smart context management (static utility functions) ───────────

const MAX_TOOL_RESULT_SIZE: usize = 50_000;
const TRUNCATED_OLD_RESULT_SIZE: usize = 500;

#[derive(Debug, Clone)]
pub struct TrimEvent {
    pub round: usize,
    pub chars_before: usize,
    pub chars_after: usize,
    pub timestamp: std::time::Instant,
}

/// Dedup tool results: when the same tool is called with identical arguments,
/// keep only the latest result. Covers read_file, list_directory, search_web, etc.
pub fn dedup_tool_results(messages: &mut Vec<ChatMessage>) {
    let mut latest_calls: HashMap<String, usize> = HashMap::new();
    let mut indices_to_remove: Vec<usize> = Vec::new();

    // Build a dedup key from the assistant's tool_call that produced each tool result.
    // For each tool result, look back at the preceding assistant message to find the arguments.
    for (i, msg) in messages.iter().enumerate() {
        if msg.role != MessageRole::Tool {
            continue;
        }
        let tool_name = msg.name.as_deref().unwrap_or("");
        // Only dedup read-like tools (not writes/shell/actions)
        let is_dedup_candidate = matches!(
            tool_name,
            "read_file" | "list_directory" | "search_web" | "get_crypto_price"
            | "get_market_price" | "fetch_webpage" | "read_pdf" | "get_ui_tree"
        );
        if !is_dedup_candidate {
            continue;
        }

        // Build dedup key from tool name + arguments (from the preceding assistant message)
        let args_key = if i > 0 {
            let prev = &messages[i - 1];
            if prev.role == MessageRole::Assistant {
                if let Some(ref tcs) = prev.tool_calls {
                    let tool_call_id = msg.tool_call_id.as_deref().unwrap_or("");
                    tcs.iter()
                        .find(|tc| tc.id == tool_call_id)
                        .map(|tc| tc.arguments.clone())
                        .unwrap_or_default()
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let dedup_key = format!("{}:{}", tool_name, args_key);

        if let Some(&prev_idx) = latest_calls.get(&dedup_key) {
            indices_to_remove.push(prev_idx);
        }
        latest_calls.insert(dedup_key, i);
    }

    if indices_to_remove.is_empty() {
        return;
    }

    indices_to_remove.sort_unstable();
    indices_to_remove.dedup();

    // Also remove the corresponding assistant tool_call message preceding each removed tool result
    let mut all_removals: Vec<usize> = Vec::new();
    for &idx in &indices_to_remove {
        all_removals.push(idx);
        if idx > 0 {
            let prev = &messages[idx - 1];
            if prev.role == MessageRole::Assistant && prev.tool_calls.is_some() && prev.content.is_empty() {
                if let Some(ref tcs) = prev.tool_calls {
                    let tool_call_id = messages[idx].tool_call_id.as_deref().unwrap_or("");
                    if tcs.iter().any(|tc| tc.id == tool_call_id) {
                        all_removals.push(idx - 1);
                    }
                }
            }
        }
    }

    all_removals.sort_unstable();
    all_removals.dedup();

    let removed_count = all_removals.len();
    for idx in all_removals.into_iter().rev() {
        if idx < messages.len() {
            messages.remove(idx);
        }
    }

    if removed_count > 0 {
        tracing::info!("[Context] Dedup removed {} duplicate tool call messages", removed_count);
    }
}

/// Progressive compression: instead of hard-truncating all tool results uniformly,
/// apply graduated compression based on age.
pub fn compress_old_results(messages: &mut Vec<ChatMessage>, budget_chars: usize) {
    let total: usize = messages.iter().map(|m| m.content.len()).sum();
    if total <= budget_chars {
        return;
    }

    // Phase 1: Truncate oversized tool results (> MAX_TOOL_RESULT_SIZE)
    for msg in messages.iter_mut() {
        if msg.role == MessageRole::Tool && msg.content.len() > MAX_TOOL_RESULT_SIZE {
            let preview: String = msg.content.chars().take(MAX_TOOL_RESULT_SIZE).collect();
            let original_len = msg.content.len();
            msg.content = format!(
                "{}...\n[结果已截断: {} -> {} 字符]",
                preview, original_len, MAX_TOOL_RESULT_SIZE
            );
        }
    }

    let total: usize = messages.iter().map(|m| m.content.len()).sum();
    if total <= budget_chars {
        return;
    }

    // Phase 2: Progressively compress older tool results
    // Find all tool result indices
    let tool_indices: Vec<usize> = messages.iter().enumerate()
        .filter(|(_, m)| m.role == MessageRole::Tool)
        .map(|(i, _)| i)
        .collect();

    if tool_indices.is_empty() {
        return;
    }

    // Compress oldest half of tool results more aggressively
    let mid = tool_indices.len() / 2;
    for &idx in &tool_indices[..mid] {
        let msg = &mut messages[idx];
        if msg.content.len() > TRUNCATED_OLD_RESULT_SIZE {
            let tool_name = msg.name.as_deref().unwrap_or("tool");
            let preview: String = msg.content.chars().take(TRUNCATED_OLD_RESULT_SIZE).collect();
            let original_len = msg.content.len();
            msg.content = format!(
                "[{}] {}...\n[早期结果已压缩: {} -> {} 字符]",
                tool_name, preview, original_len, TRUNCATED_OLD_RESULT_SIZE
            );
        }
    }

    let total: usize = messages.iter().map(|m| m.content.len()).sum();
    if total <= budget_chars {
        return;
    }

    // Phase 3: Compress remaining old tool results
    for &idx in &tool_indices[mid..] {
        let msg = &mut messages[idx];
        if msg.content.len() > 2000 {
            let preview: String = msg.content.chars().take(1500).collect();
            let original_len = msg.content.len();
            msg.content = format!(
                "{}...\n[结果已截断: {} -> 1500 字符]",
                preview, original_len
            );
        }
    }
}

/// Detect autocompact thrashing: if we've trimmed 3+ times in quick succession
/// and the context keeps refilling, return true to signal the caller to stop.
pub fn detect_thrash(trim_history: &[TrimEvent]) -> bool {
    if trim_history.len() < 3 {
        return false;
    }

    let recent = &trim_history[trim_history.len() - 3..];
    let time_span = recent.last().unwrap().timestamp.duration_since(recent[0].timestamp);

    // If 3 trims happened within 60 seconds, that's thrashing
    if time_span.as_secs() < 60 {
        let avg_reduction: f64 = recent.iter()
            .map(|e| {
                if e.chars_before > 0 {
                    (e.chars_before - e.chars_after) as f64 / e.chars_before as f64
                } else {
                    0.0
                }
            })
            .sum::<f64>() / recent.len() as f64;

        // If each trim only reduced < 20% of context, we're thrashing
        if avg_reduction < 0.20 {
            return true;
        }
    }

    false
}

/// Combined smart trim: dedup, then compress, with thrash detection.
/// Returns an updated trim history.
pub fn smart_trim(
    messages: &mut Vec<ChatMessage>,
    budget_chars: usize,
    round: usize,
    mut trim_history: Vec<TrimEvent>,
) -> (Vec<TrimEvent>, bool) {
    let chars_before: usize = messages.iter().map(|m| m.content.len()).sum();
    if chars_before <= budget_chars {
        return (trim_history, false);
    }

    tracing::warn!(
        "[Context] Smart trim: {} chars exceeds {} budget",
        chars_before, budget_chars
    );

    dedup_tool_results(messages);
    compress_old_results(messages, budget_chars);

    let chars_after: usize = messages.iter().map(|m| m.content.len()).sum();

    trim_history.push(TrimEvent {
        round,
        chars_before,
        chars_after,
        timestamp: std::time::Instant::now(),
    });

    let thrashing = detect_thrash(&trim_history);
    if thrashing {
        tracing::error!(
            "[Context] Autocompact thrash detected: trimmed {} times but context keeps refilling",
            trim_history.len()
        );
    }

    (trim_history, thrashing)
}

/// Cap a single tool result to MAX_TOOL_RESULT_SIZE chars at the source.
pub fn cap_tool_result(result: &str) -> String {
    if result.len() <= MAX_TOOL_RESULT_SIZE {
        return result.to_string();
    }
    let preview: String = result.chars().take(MAX_TOOL_RESULT_SIZE).collect();
    format!(
        "{}...\n[输出已截断: 原始 {} 字符, 显示前 {} 字符]",
        preview,
        result.len(),
        MAX_TOOL_RESULT_SIZE
    )
}

fn extract_file_path(content: &str) -> Option<String> {
    // Tool results for read_file typically start with file content or contain a path reference.
    // We look for common path patterns in the first line.
    let first_line = content.lines().next().unwrap_or("");

    // Pattern: "文件内容 (path):" or just a path at the start
    if let Some(start) = first_line.find('(') {
        if let Some(end) = first_line.find(')') {
            if end > start {
                let candidate = &first_line[start + 1..end];
                if candidate.contains('/') || candidate.contains('\\') {
                    return Some(candidate.to_string());
                }
            }
        }
    }

    // Pattern: lines starting with a path-like string
    if first_line.contains('/') || first_line.contains('\\') {
        let trimmed = first_line.trim();
        if trimmed.len() < 300 {
            return Some(trimmed.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::provider::ChatMessage;

    fn make_tool_msg(name: &str, content: &str, tool_call_id: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::Tool,
            content: content.to_string(),
            name: Some(name.to_string()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
        }
    }

    #[test]
    fn test_cap_tool_result_short() {
        let result = "hello world";
        assert_eq!(cap_tool_result(result), result);
    }

    #[test]
    fn test_cap_tool_result_long() {
        let result = "x".repeat(60_000);
        let capped = cap_tool_result(&result);
        assert!(capped.len() < 55_000);
        assert!(capped.contains("[输出已截断"));
    }

    #[test]
    fn test_compress_old_results_within_budget() {
        let mut msgs = vec![
            make_tool_msg("read_file", "short content", "tc1"),
        ];
        compress_old_results(&mut msgs, 100_000);
        assert_eq!(msgs[0].content, "short content");
    }

    #[test]
    fn test_thrash_detection() {
        let now = std::time::Instant::now();
        let history = vec![
            TrimEvent { round: 1, chars_before: 120000, chars_after: 110000, timestamp: now },
            TrimEvent { round: 2, chars_before: 120000, chars_after: 110000, timestamp: now },
            TrimEvent { round: 3, chars_before: 120000, chars_after: 110000, timestamp: now },
        ];
        assert!(detect_thrash(&history));
    }

    #[test]
    fn test_no_thrash_with_good_reduction() {
        let now = std::time::Instant::now();
        let history = vec![
            TrimEvent { round: 1, chars_before: 120000, chars_after: 60000, timestamp: now },
            TrimEvent { round: 2, chars_before: 120000, chars_after: 60000, timestamp: now },
            TrimEvent { round: 3, chars_before: 120000, chars_after: 60000, timestamp: now },
        ];
        assert!(!detect_thrash(&history));
    }
}
