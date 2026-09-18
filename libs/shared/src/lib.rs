use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    OpenAi,
    Gemini,
}

impl Provider {
    pub fn env_var(self) -> &'static str {
        match self {
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::OpenAi => "OPENAI_API_KEY",
            Self::Gemini => "GEMINI_API_KEY",
        }
    }
}

impl FromStr for Provider {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_lowercase().as_str() {
            "anthropic" => Ok(Self::Anthropic),
            "openai" => Ok(Self::OpenAi),
            "gemini" => Ok(Self::Gemini),
            _ => Err(format!("Unsupported provider: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Interactive,
    Async,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub prompt: String,
    pub provider: Provider,
    pub mode: ExecutionMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandExecution {
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub executed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub id: Uuid,
    pub output: String,
    pub created_at: DateTime<Utc>,
    pub async_job: bool,
    #[serde(default)]
    pub provider_summary: Option<String>,
    #[serde(default)]
    pub commands: Vec<CommandExecution>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    pub api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
}

impl AuthConfig {
    pub fn key_for(&self, provider: Provider) -> Option<&str> {
        match provider {
            Provider::Anthropic => self
                .anthropic_api_key
                .as_deref()
                .or(self.api_key.as_deref()),
            Provider::OpenAi => self.openai_api_key.as_deref().or(self.api_key.as_deref()),
            Provider::Gemini => self.gemini_api_key.as_deref().or(self.api_key.as_deref()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotSchedule {
    pub name: String,
    pub cron: String,
    pub prompt: String,
    #[serde(default)]
    pub provider: Option<Provider>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutopilotConfig {
    #[serde(default)]
    pub schedules: Vec<AutopilotSchedule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotRunRecord {
    pub schedule_name: String,
    pub started_at: DateTime<Utc>,
    pub summary: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutopilotState {
    #[serde(default)]
    pub running: bool,
    pub last_started_at: Option<DateTime<Utc>>,
    pub last_tick_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub schedule_count: usize,
    #[serde(default)]
    pub recent_runs: Vec<AutopilotRunRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_from_str_works() {
        assert_eq!(
            Provider::from_str("anthropic").unwrap(),
            Provider::Anthropic
        );
        assert!(Provider::from_str("unknown").is_err());
    }

    #[test]
    fn auth_config_falls_back_to_generic_key() {
        let config = AuthConfig {
            api_key: Some("generic".to_string()),
            ..Default::default()
        };

        assert_eq!(config.key_for(Provider::Gemini), Some("generic"));
    }

    #[test]
    fn autopilot_config_defaults_empty() {
        let config = AutopilotConfig::default();
        assert!(config.schedules.is_empty());
    }
}
