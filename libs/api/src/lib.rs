use ai::AiService;
use anyhow::Result;
use chrono::Utc;
use shared::{
    AgentRequest, AgentResult, AuthConfig, AutopilotConfig, AutopilotState, ExecutionMode,
};
use std::env;
use std::path::PathBuf;
use tokio::fs;
use uuid::Uuid;

fn config_dir() -> PathBuf {
    env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".ralps")
}

pub struct AuthService {
    config_dir: PathBuf,
}

impl Default for AuthService {
    fn default() -> Self {
        Self {
            config_dir: config_dir(),
        }
    }
}

impl AuthService {
    pub async fn login(&self, api_key: &str) -> Result<PathBuf> {
        fs::create_dir_all(&self.config_dir).await?;
        let auth = AuthConfig {
            api_key: api_key.to_string(),
        };
        let auth_path = self.config_dir.join("auth.toml");
        fs::write(&auth_path, toml::to_string_pretty(&auth)?).await?;
        Ok(auth_path)
    }
}

pub struct AgentEngine {
    ai: AiService,
}

impl Default for AgentEngine {
    fn default() -> Self {
        Self {
            ai: AiService::new_default(),
        }
    }
}

impl AgentEngine {
    pub async fn run(&self, request: AgentRequest) -> Result<AgentResult> {
        let generated = self.ai.generate(request.provider, &request.prompt).await?;
        let async_job = matches!(request.mode, ExecutionMode::Async);

        let output = if async_job {
            format!("Queued async execution. Result preview: {generated}")
        } else {
            generated
        };

        Ok(AgentResult {
            id: Uuid::new_v4(),
            output,
            created_at: Utc::now(),
            async_job,
        })
    }
}

pub struct AutopilotService {
    config_dir: PathBuf,
}

impl Default for AutopilotService {
    fn default() -> Self {
        Self {
            config_dir: config_dir(),
        }
    }
}

impl AutopilotService {
    fn config_path(&self) -> PathBuf {
        self.config_dir.join("autopilot.toml")
    }

    fn state_path(&self) -> PathBuf {
        self.config_dir.join("autopilot_state.toml")
    }

    pub async fn up(&self) -> Result<AutopilotState> {
        fs::create_dir_all(&self.config_dir).await?;
        let schedules = self.list_schedules().await?;
        let state = AutopilotState {
            running: true,
            last_started_at: Some(Utc::now()),
            schedule_count: schedules.schedules.len(),
        };

        fs::write(self.state_path(), toml::to_string_pretty(&state)?).await?;
        Ok(state)
    }

    pub async fn down(&self) -> Result<AutopilotState> {
        fs::create_dir_all(&self.config_dir).await?;
        let schedules = self.list_schedules().await?;
        let state = AutopilotState {
            running: false,
            last_started_at: None,
            schedule_count: schedules.schedules.len(),
        };

        fs::write(self.state_path(), toml::to_string_pretty(&state)?).await?;
        Ok(state)
    }

    pub async fn status(&self) -> Result<AutopilotState> {
        let schedules = self.list_schedules().await?;
        if !self.state_path().exists() {
            return Ok(AutopilotState {
                running: false,
                last_started_at: None,
                schedule_count: schedules.schedules.len(),
            });
        }

        let content = fs::read_to_string(self.state_path()).await?;
        let mut state: AutopilotState = toml::from_str(&content)?;
        state.schedule_count = schedules.schedules.len();
        Ok(state)
    }

    pub async fn list_schedules(&self) -> Result<AutopilotConfig> {
        if !self.config_path().exists() {
            return Ok(AutopilotConfig::default());
        }

        let content = fs::read_to_string(self.config_path()).await?;
        let parsed: AutopilotConfig = toml::from_str(&content)?;
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn agent_engine_returns_output() {
        let engine = AgentEngine::default();
        let result = engine
            .run(AgentRequest {
                prompt: "check cluster".to_string(),
                provider: shared::Provider::Anthropic,
                mode: ExecutionMode::Interactive,
            })
            .await
            .unwrap();

        assert!(!result.output.is_empty());
        assert!(!result.async_job);
    }
}
