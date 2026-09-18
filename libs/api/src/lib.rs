use ai::AiService;
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Datelike, Timelike, Utc};
use shared::{
    AgentRequest, AgentResult, AuthConfig, AutopilotConfig, AutopilotRunRecord, AutopilotState,
    CommandExecution, ExecutionMode, Provider,
};
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use tokio::process::Command;
use tokio::time::{sleep, timeout};
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
        Self::new(config_dir())
    }
}

impl AuthService {
    pub fn new(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }

    fn auth_path(&self) -> PathBuf {
        self.config_dir.join("auth.toml")
    }

    pub async fn login(&self, api_key: &str) -> Result<PathBuf> {
        fs::create_dir_all(&self.config_dir).await?;
        let auth = AuthConfig {
            api_key: Some(api_key.to_string()),
            ..Default::default()
        };
        let auth_path = self.auth_path();
        fs::write(&auth_path, toml::to_string_pretty(&auth)?).await?;
        Ok(auth_path)
    }

    pub async fn load(&self) -> Result<AuthConfig> {
        if !path_exists(&self.auth_path()).await? {
            return Ok(AuthConfig::default());
        }

        let content = fs::read_to_string(self.auth_path()).await?;
        Ok(toml::from_str(&content)?)
    }
}

pub struct AgentEngine {
    ai: AiService,
    auth: AuthService,
}

impl Default for AgentEngine {
    fn default() -> Self {
        Self::new(config_dir())
    }
}

impl AgentEngine {
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            ai: AiService::new_default(),
            auth: AuthService::new(config_dir),
        }
    }

    pub async fn run(&self, request: AgentRequest) -> Result<AgentResult> {
        let auth = self.auth.load().await.unwrap_or_default();
        let planned_commands = plan_commands(&request.prompt);
        let mut warnings = Vec::new();
        let mut commands = Vec::new();

        if planned_commands.is_empty() {
            warnings.push(
                "No executable action derived from the prompt; returning provider guidance only."
                    .to_string(),
            );
        }

        for command in planned_commands {
            commands.push(execute_command(&command).await?);
        }

        let provider_summary = match self.ai.generate_with_auth(request.provider, &request.prompt, &auth).await {
            Ok(summary) => Some(summary),
            Err(error) => {
                warnings.push(format!("Provider summary unavailable: {error}"));
                None
            }
        };

        let output = render_agent_output(&commands, provider_summary.as_deref(), &warnings);

        Ok(AgentResult {
            id: Uuid::new_v4(),
            output,
            created_at: Utc::now(),
            async_job: matches!(request.mode, ExecutionMode::Async),
            provider_summary,
            commands,
            warnings,
        })
    }
}

pub struct AutopilotService {
    config_dir: PathBuf,
    agent: AgentEngine,
}

impl Default for AutopilotService {
    fn default() -> Self {
        Self::new(config_dir())
    }
}

impl AutopilotService {
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            config_dir: config_dir.clone(),
            agent: AgentEngine::new(config_dir),
        }
    }

    fn config_path(&self) -> PathBuf {
        self.config_dir.join("autopilot.toml")
    }

    fn state_path(&self) -> PathBuf {
        self.config_dir.join("autopilot_state.toml")
    }

    pub async fn up(&self) -> Result<AutopilotState> {
        fs::create_dir_all(&self.config_dir).await?;
        let mut state = self.status().await?;
        state.running = true;
        state.last_started_at = Some(Utc::now());
        state.schedule_count = self.list_schedules().await?.schedules.len();
        self.persist_state(&state).await?;
        Ok(state)
    }

    pub async fn down(&self) -> Result<AutopilotState> {
        fs::create_dir_all(&self.config_dir).await?;
        let mut state = self.status().await?;
        state.running = false;
        state.schedule_count = self.list_schedules().await?.schedules.len();
        self.persist_state(&state).await?;
        Ok(state)
    }

    pub async fn status(&self) -> Result<AutopilotState> {
        let schedules = self.list_schedules().await?;
        if !path_exists(&self.state_path()).await? {
            return Ok(AutopilotState {
                running: false,
                last_started_at: None,
                last_tick_at: None,
                schedule_count: schedules.schedules.len(),
                recent_runs: Vec::new(),
            });
        }

        let content = fs::read_to_string(self.state_path()).await?;
        let mut state: AutopilotState = toml::from_str(&content)?;
        state.schedule_count = schedules.schedules.len();
        Ok(state)
    }

    pub async fn list_schedules(&self) -> Result<AutopilotConfig> {
        if !path_exists(&self.config_path()).await? {
            return Ok(AutopilotConfig::default());
        }

        let content = fs::read_to_string(self.config_path()).await?;
        let parsed: AutopilotConfig = toml::from_str(&content)?;
        Ok(parsed)
    }

    pub async fn run_loop(&self, poll_interval: Duration) -> Result<()> {
        loop {
            let state = self.status().await?;
            if !state.running {
                break;
            }

            self.run_pending_at(Utc::now()).await?;
            sleep(poll_interval).await;
        }

        Ok(())
    }

    pub async fn run_pending_at(&self, now: DateTime<Utc>) -> Result<Vec<AutopilotRunRecord>> {
        let schedules = self.list_schedules().await?;
        let mut state = self.status().await?;
        if !state.running {
            return Ok(Vec::new());
        }

        let mut runs = Vec::new();
        for schedule in schedules.schedules.into_iter().filter(|schedule| schedule.enabled) {
            if !cron_matches(&schedule.cron, now)? || already_ran_this_minute(&state, &schedule.name, now)
            {
                continue;
            }

            let request = AgentRequest {
                prompt: schedule
                    .command
                    .clone()
                    .map(|command| format!("run: {command}"))
                    .unwrap_or_else(|| schedule.prompt.clone()),
                provider: schedule.provider.unwrap_or(Provider::Anthropic),
                mode: ExecutionMode::Async,
            };
            let result = self.agent.run(request).await?;
            let summary = if let Some(command) = schedule.command.as_deref() {
                format!("executed {command}")
            } else {
                summarize_line(&result.output)
            };

            let run = AutopilotRunRecord {
                schedule_name: schedule.name,
                started_at: now,
                summary,
                success: result.commands.iter().all(|command| command.success)
                    && !result.commands.is_empty(),
            };
            state.recent_runs.push(run.clone());
            runs.push(run);
        }

        state.last_tick_at = Some(now);
        state.schedule_count = self.list_schedules().await?.schedules.len();
        if state.recent_runs.len() > 20 {
            let drain_count = state.recent_runs.len() - 20;
            state.recent_runs.drain(0..drain_count);
        }
        self.persist_state(&state).await?;
        Ok(runs)
    }

    async fn persist_state(&self, state: &AutopilotState) -> Result<()> {
        fs::write(self.state_path(), toml::to_string_pretty(state)?).await?;
        Ok(())
    }
}

fn plan_commands(prompt: &str) -> Vec<String> {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if let Some(command) = extract_prefixed_command(trimmed) {
        return vec![command];
    }

    let normalized = trimmed.to_lowercase();
    if normalized.contains("check system health") {
        return vec![
            "uptime".to_string(),
            "df -h".to_string(),
            "uname -a".to_string(),
        ];
    }

    if normalized == "git status" || normalized.contains("show git status") {
        return vec!["git status --short --branch".to_string()];
    }

    if normalized.contains("list files") || normalized.contains("show files") {
        return vec!["ls".to_string()];
    }

    let first_word = trimmed.split_whitespace().next().unwrap_or_default();
    if is_allowed_program(first_word) {
        return vec![trimmed.to_string()];
    }

    Vec::new()
}

fn extract_prefixed_command(prompt: &str) -> Option<String> {
    for prefix in ["run:", "cmd:", "command:", "$ "] {
        if let Some(command) = prompt.strip_prefix(prefix) {
            let trimmed = command.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

async fn execute_command(command: &str) -> Result<CommandExecution> {
    validate_command(command)?;

    let args = split_command(command)?;
    let program = args
        .first()
        .ok_or_else(|| anyhow!("command cannot be empty"))?
        .to_string();
    let command_args = args[1..].to_vec();

    let output = timeout(
        Duration::from_secs(60),
        Command::new(&program).args(&command_args).output(),
    )
    .await
    .map_err(|_| anyhow!("command timed out after 60 seconds: {command}"))??;

    Ok(CommandExecution {
        command: command.to_string(),
        exit_code: output.status.code().unwrap_or(-1),
        stdout: truncate(&String::from_utf8_lossy(&output.stdout), 4000),
        stderr: truncate(&String::from_utf8_lossy(&output.stderr), 4000),
        success: output.status.success(),
        executed_at: Utc::now(),
    })
}

fn validate_command(command: &str) -> Result<()> {
    if command.chars().any(|ch| matches!(ch, '|' | '&' | ';' | '>' | '<' | '`')) {
        return Err(anyhow!("unsafe shell operator rejected in command: {command}"));
    }

    let args = split_command(command)?;
    let program = args
        .first()
        .ok_or_else(|| anyhow!("command cannot be empty"))?;

    if !is_allowed_program(program) {
        return Err(anyhow!("command is not allowed: {program}"));
    }

    Ok(())
}

fn split_command(command: &str) -> Result<Vec<String>> {
    let parts: Vec<String> = command.split_whitespace().map(ToOwned::to_owned).collect();
    if parts.is_empty() {
        return Err(anyhow!("command cannot be empty"));
    }
    Ok(parts)
}

fn is_allowed_program(program: &str) -> bool {
    matches!(
        program,
        "pwd"
            | "ls"
            | "echo"
            | "cat"
            | "head"
            | "tail"
            | "grep"
            | "rg"
            | "find"
            | "git"
            | "cargo"
            | "kubectl"
            | "docker"
            | "terraform"
            | "uptime"
            | "uname"
            | "whoami"
            | "date"
            | "df"
            | "free"
            | "ps"
    )
}

fn render_agent_output(
    commands: &[CommandExecution],
    provider_summary: Option<&str>,
    warnings: &[String],
) -> String {
    let mut sections = Vec::new();

    if !commands.is_empty() {
        sections.push(format_command_section(commands));
    }

    if let Some(summary) = provider_summary {
        sections.push(format!("Provider summary:\n{summary}"));
    }

    if !warnings.is_empty() {
        sections.push(format!("Warnings:\n- {}", warnings.join("\n- ")));
    }

    if sections.is_empty() {
        "No output produced.".to_string()
    } else {
        sections.join("\n\n")
    }
}

fn format_command_section(commands: &[CommandExecution]) -> String {
    let mut lines = vec!["Executed commands:".to_string()];
    for command in commands {
        lines.push(format!(
            "- {} [{}]",
            command.command,
            if command.success { "ok" } else { "failed" }
        ));
        if !command.stdout.trim().is_empty() {
            lines.push(format!("  stdout: {}", command.stdout.trim().replace('\n', " | ")));
        }
        if !command.stderr.trim().is_empty() {
            lines.push(format!("  stderr: {}", command.stderr.trim().replace('\n', " | ")));
        }
    }
    lines.join("\n")
}

fn truncate(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        value.to_string()
    } else {
        format!("{}...", &value[..max_len])
    }
}

fn summarize_line(value: &str) -> String {
    value
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("completed")
        .trim()
        .to_string()
}

fn cron_matches(expression: &str, now: DateTime<Utc>) -> Result<bool> {
    let fields: Vec<&str> = expression.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(anyhow!("cron expression must have 5 fields: {expression}"));
    }

    Ok(matches_field(fields[0], now.minute())?
        && matches_field(fields[1], now.hour())?
        && matches_field(fields[2], now.day())?
        && matches_field(fields[3], now.month())?
        && matches_field(fields[4], now.weekday().num_days_from_sunday())?)
}

fn matches_field(field: &str, value: u32) -> Result<bool> {
    if field == "*" {
        return Ok(true);
    }

    if let Some(step) = field.strip_prefix("*/") {
        let step: u32 = step.parse().context("invalid cron step")?;
        return Ok(step != 0 && value.is_multiple_of(step));
    }

    for candidate in field.split(',') {
        let parsed: u32 = candidate.parse().with_context(|| format!("invalid cron value: {candidate}"))?;
        if parsed == value {
            return Ok(true);
        }
    }

    Ok(false)
}

fn already_ran_this_minute(state: &AutopilotState, schedule_name: &str, now: DateTime<Utc>) -> bool {
    state.recent_runs.iter().any(|run| {
        run.schedule_name == schedule_name
            && run.started_at.year() == now.year()
            && run.started_at.month() == now.month()
            && run.started_at.day() == now.day()
            && run.started_at.hour() == now.hour()
            && run.started_at.minute() == now.minute()
    })
}

async fn path_exists(path: &Path) -> Result<bool> {
    Ok(fs::try_exists(path).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ralps-{name}-{}", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn agent_engine_executes_explicit_command() {
        let engine = AgentEngine::new(temp_config_dir("agent"));
        let result = engine
            .run(AgentRequest {
                prompt: "run: echo hello".to_string(),
                provider: shared::Provider::Anthropic,
                mode: ExecutionMode::Interactive,
            })
            .await
            .unwrap();

        assert_eq!(result.commands.len(), 1);
        assert!(result.commands[0].stdout.contains("hello"));
    }

    #[tokio::test]
    async fn autopilot_runs_due_schedule_once() {
        let config_dir = temp_config_dir("autopilot");
        fs::create_dir_all(&config_dir).await.unwrap();
        fs::write(
            config_dir.join("autopilot.toml"),
            r#"[[schedules]]
name = "health-check"
cron = "* * * * *"
prompt = "check system health"
command = "echo healthy"
"#,
        )
        .await
        .unwrap();

        let service = AutopilotService::new(config_dir.clone());
        service.up().await.unwrap();

        let now = Utc::now();
        let first = service.run_pending_at(now).await.unwrap();
        let second = service.run_pending_at(now).await.unwrap();

        assert_eq!(first.len(), 1);
        assert!(second.is_empty());
    }

    #[test]
    fn cron_step_matches() {
        let now = DateTime::parse_from_rfc3339("2026-09-18T12:10:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(cron_matches("*/5 * * * *", now).unwrap());
        assert!(!cron_matches("*/7 * * * *", now).unwrap());
    }
}
