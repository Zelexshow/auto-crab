use super::provider::*;
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::ReceiverStream;

pub struct OllamaProvider {
    client: Client,
    endpoint: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(endpoint: &str, model: &str) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            model: model.to_string(),
        }
    }
}

#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: Option<f32>,
    num_predict: Option<u32>,
}

#[derive(Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    message: Option<OllamaMessage>,
    done: Option<bool>,
    eval_count: Option<u32>,
    prompt_eval_count: Option<u32>,
}

fn role_to_str(role: &MessageRole) -> &str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

#[async_trait]
impl ModelProvider for OllamaProvider {
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        let messages: Vec<OllamaMessage> = request.messages.iter().map(|m| OllamaMessage {
            role: role_to_str(&m.role).into(),
            content: m.content.clone(),
        }).collect();

        let api_req = OllamaChatRequest {
            model: self.model.clone(),
            messages,
            stream: false,
            options: Some(OllamaOptions {
                temperature: Some(request.temperature),
                num_predict: request.max_tokens,
            }),
        };

        let resp = self.client
            .post(format!("{}/api/chat", self.endpoint))
            .json(&api_req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("[ollama] API error {}: {}", status, body);
        }

        let api_resp: OllamaChatResponse = resp.json().await?;
        let msg = api_resp.message.unwrap_or(OllamaMessage {
            role: "assistant".into(),
            content: String::new(),
        });

        Ok(ChatResponse {
            message: ChatMessage {
                role: MessageRole::Assistant,
                content: msg.content,
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            usage: Some(TokenUsage {
                prompt_tokens: api_resp.prompt_eval_count.unwrap_or(0),
                completion_tokens: api_resp.eval_count.unwrap_or(0),
                total_tokens: api_resp.prompt_eval_count.unwrap_or(0) + api_resp.eval_count.unwrap_or(0),
            }),
            model: self.model.clone(),
            finish_reason: Some("stop".into()),
        })
    }

    async fn chat_stream(&self, request: ChatRequest) -> anyhow::Result<ChatStream> {
        let messages: Vec<OllamaMessage> = request.messages.iter().map(|m| OllamaMessage {
            role: role_to_str(&m.role).into(),
            content: m.content.clone(),
        }).collect();

        let api_req = OllamaChatRequest {
            model: self.model.clone(),
            messages,
            stream: true,
            options: Some(OllamaOptions {
                temperature: Some(request.temperature),
                num_predict: request.max_tokens,
            }),
        };

        let resp = self.client
            .post(format!("{}/api/chat", self.endpoint))
            .json(&api_req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("[ollama] API error {}: {}", status, body);
        }

        let (tx, rx) = tokio::sync::mpsc::channel::<anyhow::Result<ChatChunk>>(64);
        let mut byte_stream = resp.bytes_stream();

        tokio::spawn(async move {
            let mut buffer = String::new();
            while let Some(chunk_result) = byte_stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(line_end) = buffer.find('\n') {
                            let line = buffer[..line_end].trim().to_string();
                            buffer = buffer[line_end + 1..].to_string();

                            if line.is_empty() { continue; }

                            if let Ok(parsed) = serde_json::from_str::<OllamaChatResponse>(&line) {
                                let content = parsed.message.map(|m| m.content).unwrap_or_default();
                                let done = parsed.done.unwrap_or(false);
                                let chunk = ChatChunk {
                                    delta: content,
                                    tool_calls: None,
                                    finish_reason: if done { Some("stop".into()) } else { None },
                                };
                                if tx.send(Ok(chunk)).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.into())).await;
                        return;
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "ollama".into(),
            display_name: format!("Ollama ({})", self.model),
            supports_tools: false,
            supports_streaming: true,
            max_context_tokens: 32768,
            is_local: true,
        }
    }
}
