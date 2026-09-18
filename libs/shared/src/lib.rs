use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Anthropic,
    OpenAi,
    Gemini,
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
pub struct AgentResult {
    pub id: Uuid,
    pub output: String,
    pub created_at: DateTime<Utc>,
    pub async_job: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotSchedule {
    pub name: String,
    pub cron: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutopilotConfig {
    #[serde(default)]
    pub schedules: Vec<AutopilotSchedule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutopilotState {
    #[serde(default)]
    pub running: bool,
    pub last_started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub schedule_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_from_str_works() {
        assert_eq!(Provider::from_str("anthropic").unwrap(), Provider::Anthropic);
        assert!(Provider::from_str("unknown").is_err());
    }

    #[test]
    fn autopilot_config_defaults_empty() {
        let config = AutopilotConfig::default();
        assert!(config.schedules.is_empty());
    }
}
