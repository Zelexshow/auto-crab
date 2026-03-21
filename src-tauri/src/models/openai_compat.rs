use super::provider::*;
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::ReceiverStream;

/// OpenAI-compatible adapter. Works with:
/// - OpenAI (GPT-4o, o1, o3)
/// - DeepSeek (deepseek-chat, deepseek-coder)
/// - 通义千问 DashScope (qwen-max, qwen-plus) via compatible endpoint
/// - 智谱 GLM (glm-4) via compatible endpoint
/// - 月之暗面 Moonshot (moonshot-v1-128k)
/// - Any OpenAI-compatible API
pub struct OpenAICompatProvider {
    client: Client,
    config: OpenAICompatConfig,
}

#[derive(Debug, Clone)]
pub struct OpenAICompatConfig {
    pub provider_name: String,
    pub display_name: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_context: usize,
    pub supports_tools: bool,
}

impl OpenAICompatProvider {
    pub fn new(config: OpenAICompatConfig) -> Self {
        let mut builder = Client::builder()
            .timeout(std::time::Duration::from_secs(120));

        // Auto-detect system proxy (Clash etc.)
        if let Ok(proxy_url) = std::env::var("HTTPS_PROXY").or_else(|_| std::env::var("HTTP_PROXY")) {
            if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
                builder = builder.proxy(proxy);
                tracing::debug!("Using proxy: {}", proxy_url);
            }
        }

        let client = builder.build().expect("failed to build HTTP client");
        Self { client, config }
    }

    pub fn openai(api_key: &str, model: &str) -> Self {
        Self::new(OpenAICompatConfig {
            provider_name: "openai".into(),
            display_name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: api_key.into(),
            model: model.into(),
            max_context: 128000,
            supports_tools: true,
        })
    }

    pub fn deepseek(api_key: &str, model: &str) -> Self {
        Self::new(OpenAICompatConfig {
            provider_name: "deepseek".into(),
            display_name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            api_key: api_key.into(),
            model: model.into(),
            max_context: 64000,
            supports_tools: true,
        })
    }

    pub fn dashscope(api_key: &str, model: &str) -> Self {
        Self::new(OpenAICompatConfig {
            provider_name: "dashscope".into(),
            display_name: "通义千问".into(),
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
            api_key: api_key.into(),
            model: model.into(),
            max_context: 128000,
            supports_tools: true,
        })
    }

    pub fn zhipu(api_key: &str, model: &str) -> Self {
        Self::new(OpenAICompatConfig {
            provider_name: "zhipu".into(),
            display_name: "智谱 GLM".into(),
            base_url: "https://open.bigmodel.cn/api/paas/v4".into(),
            api_key: api_key.into(),
            model: model.into(),
            max_context: 128000,
            supports_tools: true,
        })
    }

    pub fn moonshot(api_key: &str, model: &str) -> Self {
        Self::new(OpenAICompatConfig {
            provider_name: "moonshot".into(),
            display_name: "月之暗面 Kimi".into(),
            base_url: "https://api.moonshot.cn/v1".into(),
            api_key: api_key.into(),
            model: model.into(),
            max_context: 128000,
            supports_tools: true,
        })
    }

    pub fn anthropic(api_key: &str, model: &str) -> Self {
        Self::new(OpenAICompatConfig {
            provider_name: "anthropic".into(),
            display_name: "Anthropic Claude".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            api_key: api_key.into(),
            model: model.into(),
            max_context: 200000,
            supports_tools: true,
        })
    }
}

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
}

#[derive(Serialize, Deserialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    choices: Vec<ApiChoice>,
    usage: Option<ApiUsage>,
    model: Option<String>,
}

#[derive(Deserialize)]
struct ApiChoice {
    message: Option<ApiResponseMessage>,
    delta: Option<ApiResponseMessage>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ApiResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ApiToolCall>>,
}

#[derive(Deserialize)]
struct ApiToolCall {
    id: Option<String>,
    function: Option<ApiFunction>,
}

#[derive(Deserialize)]
struct ApiFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct ApiUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct StreamLine {
    choices: Option<Vec<ApiChoice>>,
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
impl ModelProvider for OpenAICompatProvider {
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        let messages: Vec<ApiMessage> = request.messages.iter().map(|m| ApiMessage {
            role: role_to_str(&m.role).into(),
            content: m.content.clone(),
        }).collect();

        let api_req = ApiRequest {
            model: self.config.model.clone(),
            messages,
            tools: None,
            temperature: Some(request.temperature),
            max_tokens: request.max_tokens,
            stream: false,
        };

        let mut req_builder = self.client
            .post(format!("{}/chat/completions", self.config.base_url))
            .json(&api_req);

        if self.config.provider_name == "anthropic" {
            req_builder = req_builder
                .header("x-api-key", &self.config.api_key)
                .header("anthropic-version", "2023-06-01");
        } else {
            req_builder = req_builder
                .header("Authorization", format!("Bearer {}", self.config.api_key));
        }

        let resp = req_builder.send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("[{}] API error {}: {}", self.config.provider_name, status, body);
        }

        let api_resp: ApiResponse = resp.json().await?;
        let choice = api_resp.choices.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("empty response from model"))?;

        let msg = choice.message.unwrap_or(ApiResponseMessage { content: None, tool_calls: None });

        let tool_calls = msg.tool_calls.map(|tcs| {
            tcs.into_iter().map(|tc| ToolCall {
                id: tc.id.unwrap_or_default(),
                name: tc.function.as_ref().and_then(|f| f.name.clone()).unwrap_or_default(),
                arguments: tc.function.as_ref().and_then(|f| f.arguments.clone()).unwrap_or_default(),
            }).collect()
        });

        Ok(ChatResponse {
            message: ChatMessage {
                role: MessageRole::Assistant,
                content: msg.content.unwrap_or_default(),
                name: None,
                tool_calls,
                tool_call_id: None,
            },
            usage: api_resp.usage.map(|u| TokenUsage {
                prompt_tokens: u.prompt_tokens.unwrap_or(0),
                completion_tokens: u.completion_tokens.unwrap_or(0),
                total_tokens: u.total_tokens.unwrap_or(0),
            }),
            model: api_resp.model.unwrap_or_else(|| self.config.model.clone()),
            finish_reason: choice.finish_reason,
        })
    }

    async fn chat_stream(&self, request: ChatRequest) -> anyhow::Result<ChatStream> {
        let messages: Vec<ApiMessage> = request.messages.iter().map(|m| ApiMessage {
            role: role_to_str(&m.role).into(),
            content: m.content.clone(),
        }).collect();

        let api_req = ApiRequest {
            model: self.config.model.clone(),
            messages,
            tools: None,
            temperature: Some(request.temperature),
            max_tokens: request.max_tokens,
            stream: true,
        };

        let mut req_builder = self.client
            .post(format!("{}/chat/completions", self.config.base_url))
            .json(&api_req);

        if self.config.provider_name == "anthropic" {
            req_builder = req_builder
                .header("x-api-key", &self.config.api_key)
                .header("anthropic-version", "2023-06-01");
        } else {
            req_builder = req_builder
                .header("Authorization", format!("Bearer {}", self.config.api_key));
        }

        let resp = req_builder.send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("[{}] API error {}: {}", self.config.provider_name, status, body);
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

                            if line.is_empty() || line == "data: [DONE]" {
                                continue;
                            }
                            if let Some(json_str) = line.strip_prefix("data: ") {
                                if let Ok(parsed) = serde_json::from_str::<StreamLine>(json_str) {
                                    if let Some(choices) = parsed.choices {
                                        if let Some(choice) = choices.into_iter().next() {
                                            if let Some(delta) = choice.delta {
                                                let chunk = ChatChunk {
                                                    delta: delta.content.unwrap_or_default(),
                                                    tool_calls: None,
                                                    finish_reason: choice.finish_reason,
                                                };
                                                if tx.send(Ok(chunk)).await.is_err() {
                                                    return;
                                                }
                                            }
                                        }
                                    }
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
            name: self.config.provider_name.clone(),
            display_name: self.config.display_name.clone(),
            supports_tools: self.config.supports_tools,
            supports_streaming: true,
            max_context_tokens: self.config.max_context,
            is_local: false,
        }
    }
}
