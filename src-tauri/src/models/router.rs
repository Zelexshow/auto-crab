use super::provider::*;
use super::openai_compat::OpenAICompatProvider;
use super::ollama::OllamaProvider;
use crate::config::{AppConfig, ModelEntry};
use crate::security::credentials::CredentialStore;
use std::collections::HashMap;
use std::sync::Arc;

pub struct ModelRouter {
    providers: HashMap<String, Arc<dyn ModelProvider>>,
    routing: HashMap<String, String>,
    primary_key: Option<String>,
    fallback_key: Option<String>,
}

impl ModelRouter {
    pub fn from_config(config: &AppConfig) -> anyhow::Result<Self> {
        let mut providers: HashMap<String, Arc<dyn ModelProvider>> = HashMap::new();

        if let Some(ref entry) = config.models.primary {
            let key = format!("primary:{}", entry.provider);
            if let Ok(provider) = Self::build_provider(entry) {
                providers.insert(key, Arc::from(provider));
            }
        }

        if let Some(ref entry) = config.models.fallback {
            let key = format!("fallback:{}", entry.provider);
            if let Ok(provider) = Self::build_provider(entry) {
                providers.insert(key, Arc::from(provider));
            }
        }

        if let Some(ref entry) = config.models.coding {
            let key = format!("coding:{}", entry.provider);
            if let Ok(provider) = Self::build_provider(entry) {
                providers.insert(key, Arc::from(provider));
            }
        }

        let primary_key = config.models.primary.as_ref()
            .map(|e| format!("primary:{}", e.provider));
        let fallback_key = config.models.fallback.as_ref()
            .map(|e| format!("fallback:{}", e.provider));

        Ok(Self {
            providers,
            routing: config.models.routing.clone(),
            primary_key,
            fallback_key,
        })
    }

    fn build_provider(entry: &ModelEntry) -> anyhow::Result<Box<dyn ModelProvider>> {
        let api_key = entry.api_key_ref.as_deref()
            .map(CredentialStore::resolve_ref)
            .transpose()?
            .unwrap_or_default();

        match entry.provider.as_str() {
            "openai" => Ok(Box::new(OpenAICompatProvider::openai(&api_key, &entry.model))),
            "deepseek" => Ok(Box::new(OpenAICompatProvider::deepseek(&api_key, &entry.model))),
            "dashscope" => Ok(Box::new(OpenAICompatProvider::dashscope(&api_key, &entry.model))),
            "zhipu" => Ok(Box::new(OpenAICompatProvider::zhipu(&api_key, &entry.model))),
            "moonshot" => Ok(Box::new(OpenAICompatProvider::moonshot(&api_key, &entry.model))),
            "anthropic" => Ok(Box::new(OpenAICompatProvider::anthropic(&api_key, &entry.model))),
            "ollama" => {
                let endpoint = entry.endpoint.as_deref().unwrap_or("http://localhost:11434");
                Ok(Box::new(OllamaProvider::new(endpoint, &entry.model)))
            }
            other => anyhow::bail!("Unknown model provider: {}", other),
        }
    }

    pub fn get_for_task(&self, task_type: &str) -> Option<Arc<dyn ModelProvider>> {
        if let Some(model_name) = self.routing.get(task_type) {
            for (_, provider) in &self.providers {
                let info = provider.info();
                if info.name == *model_name || info.display_name.contains(model_name) {
                    return Some(Arc::clone(provider));
                }
            }
        }

        self.primary_key.as_ref()
            .and_then(|k| self.providers.get(k))
            .map(Arc::clone)
    }

    pub fn get_primary(&self) -> Option<Arc<dyn ModelProvider>> {
        self.primary_key.as_ref()
            .and_then(|k| self.providers.get(k))
            .map(Arc::clone)
    }

    pub fn get_fallback(&self) -> Option<Arc<dyn ModelProvider>> {
        self.fallback_key.as_ref()
            .and_then(|k| self.providers.get(k))
            .map(Arc::clone)
    }

    /// Try primary, fall back to fallback on error
    pub async fn chat_with_fallback(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        if let Some(primary) = self.get_primary() {
            match primary.chat(request.clone()).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    tracing::warn!("Primary model failed: {}, trying fallback...", e);
                }
            }
        }

        if let Some(fallback) = self.get_fallback() {
            return fallback.chat(request).await;
        }

        anyhow::bail!("No model provider available")
    }

    pub fn list_available(&self) -> Vec<ProviderInfo> {
        self.providers.values().map(|p| p.info()).collect()
    }
}
