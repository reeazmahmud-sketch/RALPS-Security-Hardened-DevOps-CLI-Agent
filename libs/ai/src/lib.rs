use anyhow::{anyhow, Result};
use async_trait::async_trait;
use shared::Provider;
use std::collections::HashMap;
use std::sync::Arc;

#[async_trait]
pub trait ProviderClient: Send + Sync {
    async fn generate(&self, prompt: &str) -> Result<String>;
}

pub struct AnthropicClient;
pub struct OpenAiClient;
pub struct GeminiClient;

#[async_trait]
impl ProviderClient for AnthropicClient {
    async fn generate(&self, prompt: &str) -> Result<String> {
        Ok(format!("[anthropic] {prompt}"))
    }
}

#[async_trait]
impl ProviderClient for OpenAiClient {
    async fn generate(&self, prompt: &str) -> Result<String> {
        Ok(format!("[openai] {prompt}"))
    }
}

#[async_trait]
impl ProviderClient for GeminiClient {
    async fn generate(&self, prompt: &str) -> Result<String> {
        Ok(format!("[gemini] {prompt}"))
    }
}

pub struct AiService {
    clients: HashMap<Provider, Arc<dyn ProviderClient>>,
}

impl AiService {
    pub fn new_default() -> Self {
        let mut clients: HashMap<Provider, Arc<dyn ProviderClient>> = HashMap::new();
        clients.insert(Provider::Anthropic, Arc::new(AnthropicClient));
        clients.insert(Provider::OpenAi, Arc::new(OpenAiClient));
        clients.insert(Provider::Gemini, Arc::new(GeminiClient));

        Self { clients }
    }

    pub async fn generate(&self, provider: Provider, prompt: &str) -> Result<String> {
        let client = self
            .clients
            .get(&provider)
            .ok_or_else(|| anyhow!("Provider not configured"))?;

        client.generate(prompt).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn generates_for_configured_provider() {
        let service = AiService::new_default();
        let output = service.generate(Provider::OpenAi, "ping").await.unwrap();
        assert!(output.contains("ping"));
    }
}
