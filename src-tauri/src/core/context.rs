use crate::models::provider::{ChatMessage, MessageRole};

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
