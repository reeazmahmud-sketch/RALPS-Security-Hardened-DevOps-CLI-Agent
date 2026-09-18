use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};
use shared::{AuthConfig, Provider};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;

#[async_trait]
pub trait ProviderClient: Send + Sync {
    async fn generate(&self, api_key: &str, prompt: &str) -> Result<String>;
}

pub struct AnthropicClient {
    http: Client,
}

pub struct OpenAiClient {
    http: Client,
}

pub struct GeminiClient {
    http: Client,
}

impl Default for AnthropicClient {
    fn default() -> Self {
        Self {
            http: Client::new(),
        }
    }
}

impl Default for OpenAiClient {
    fn default() -> Self {
        Self {
            http: Client::new(),
        }
    }
}

impl Default for GeminiClient {
    fn default() -> Self {
        Self {
            http: Client::new(),
        }
    }
}

#[async_trait]
impl ProviderClient for AnthropicClient {
    async fn generate(&self, api_key: &str, prompt: &str) -> Result<String> {
        let response = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&json!({
                "model": "claude-3-5-sonnet-latest",
                "max_tokens": 1024,
                "messages": [{"role": "user", "content": prompt}],
            }))
            .send()
            .await
            .context("failed to call Anthropic API")?;

        parse_text_response(response, |body| {
            body.get("content")
                .and_then(Value::as_array)
                .and_then(|parts| parts.first())
                .and_then(|part| part.get("text"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .await
    }
}

#[async_trait]
impl ProviderClient for OpenAiClient {
    async fn generate(&self, api_key: &str, prompt: &str) -> Result<String> {
        let response = self
            .http
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(api_key)
            .json(&json!({
                "model": "gpt-4o-mini",
                "messages": [{"role": "user", "content": prompt}],
            }))
            .send()
            .await
            .context("failed to call OpenAI API")?;

        parse_text_response(response, |body| {
            body.get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| choices.first())
                .and_then(|choice| choice.get("message"))
                .and_then(|message| message.get("content"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .await
    }
}

#[async_trait]
impl ProviderClient for GeminiClient {
    async fn generate(&self, api_key: &str, prompt: &str) -> Result<String> {
        let response = self
            .http
            .post(format!(
                "https://generativelanguage.googleapis.com/v1beta/models/gemini-1.5-flash:generateContent?key={api_key}"
            ))
            .json(&json!({
                "contents": [{"parts": [{"text": prompt}]}],
            }))
            .send()
            .await
            .context("failed to call Gemini API")?;

        parse_text_response(response, |body| {
            body.get("candidates")
                .and_then(Value::as_array)
                .and_then(|candidates| candidates.first())
                .and_then(|candidate| candidate.get("content"))
                .and_then(|content| content.get("parts"))
                .and_then(Value::as_array)
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|part| part.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .filter(|text| !text.is_empty())
        })
        .await
    }
}

pub struct AiService {
    clients: HashMap<Provider, Arc<dyn ProviderClient>>,
}

impl AiService {
    pub fn new_default() -> Self {
        let mut clients: HashMap<Provider, Arc<dyn ProviderClient>> = HashMap::new();
        clients.insert(Provider::Anthropic, Arc::new(AnthropicClient::default()));
        clients.insert(Provider::OpenAi, Arc::new(OpenAiClient::default()));
        clients.insert(Provider::Gemini, Arc::new(GeminiClient::default()));

        Self { clients }
    }

    pub async fn generate_with_auth(
        &self,
        provider: Provider,
        prompt: &str,
        auth: &AuthConfig,
    ) -> Result<String> {
        let client = self
            .clients
            .get(&provider)
            .ok_or_else(|| anyhow!("Provider not configured"))?;
        let api_key = resolve_api_key(provider, auth)?;
        client.generate(&api_key, prompt).await
    }
}

fn resolve_api_key(provider: Provider, auth: &AuthConfig) -> Result<String> {
    if let Ok(value) = env::var(provider.env_var()) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    auth.key_for(provider)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            anyhow!(
                "Missing API key for {:?}. Set {} or ~/.ralps/auth.toml",
                provider,
                provider.env_var()
            )
        })
}

async fn parse_text_response(
    response: reqwest::Response,
    extractor: impl Fn(&Value) -> Option<String>,
) -> Result<String> {
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("provider returned non-JSON response")?;

    if !status.is_success() {
        let message = body
            .get("error")
            .and_then(|error| error.get("message").or(Some(error)))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| body.to_string());
        bail!("provider request failed: {message}");
    }

    extractor(&body).ok_or_else(|| anyhow!("provider response missing text content"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_key_resolves_for_provider() {
        let auth = AuthConfig {
            api_key: Some("shared-key".to_string()),
            ..Default::default()
        };

        assert_eq!(
            resolve_api_key(Provider::OpenAi, &auth).unwrap(),
            "shared-key"
        );
    }
}
