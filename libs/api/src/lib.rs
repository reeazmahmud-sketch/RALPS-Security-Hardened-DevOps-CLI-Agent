use ai::AiService;
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Datelike, Timelike, Utc};
use reqwest::StatusCode;
use serde_json::{json, Value};
use shared::{
    AgentRequest, AgentResult, AuthConfig, AutopilotConfig, AutopilotRunRecord, AutopilotState,
    CommandExecution, ExecutionMode, PlanStepSafety, Provider, RemoteConfig, TaskIntent, TaskPlan,
    TaskPlanStep,
};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
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

fn remote_path(config_dir: &Path) -> PathBuf {
    config_dir.join("remote.toml")
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
        let remote = load_remote_config(self.auth.config_dir.clone()).await?;
        let plan = build_task_plan(&request.prompt);
        let mut warnings = Vec::new();
        let mut commands = Vec::new();

        if plan.steps.is_empty() {
            warnings.push(
                "No executable action derived from the prompt; returning provider guidance only."
                    .to_string(),
            );
        }

        warnings.extend(plan.warnings.iter().cloned());

        for command in plan
            .steps
            .iter()
            .filter_map(|step| step.command.as_deref())
            .map(ToOwned::to_owned)
        {
            commands.push(execute_command_with_remote(&command, &remote).await?);
        }

        let provider_summary = match self
            .ai
            .generate_with_auth(request.provider, &request.prompt, &auth)
            .await
        {
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
            plan,
            provider_summary,
            commands,
            warnings,
        })
    }

    pub fn plan_task(&self, prompt: &str) -> TaskPlan {
        build_task_plan(prompt)
    }

    pub async fn run_safe_command(&self, command: &str) -> Result<CommandExecution> {
        let remote = load_remote_config(self.auth.config_dir.clone()).await?;
        execute_command_with_remote(command, &remote).await
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

    fn pid_path(&self) -> PathBuf {
        self.config_dir.join("autopilot.pid")
    }

    pub async fn up(&self) -> Result<AutopilotState> {
        self.daemon_start(std::process::id()).await
    }

    pub async fn down(&self) -> Result<AutopilotState> {
        self.daemon_stop().await
    }

    pub async fn status(&self) -> Result<AutopilotState> {
        self.recover_stale_daemon().await?;
        let schedules = self.list_schedules().await?;
        let mut state = if !path_exists(&self.state_path()).await? {
            AutopilotState {
                running: false,
                daemon_pid: None,
                daemon_started_at: None,
                daemon_heartbeat_at: None,
                last_started_at: None,
                last_tick_at: None,
                schedule_count: schedules.schedules.len(),
                recent_runs: Vec::new(),
            }
        } else {
            let content = fs::read_to_string(self.state_path()).await?;
            toml::from_str(&content)?
        };

        state.schedule_count = schedules.schedules.len();
        let remote = load_remote_config(self.config_dir.clone()).await?;
        if remote.enabled {
            state = fetch_remote_status(&remote).await?;
            state.schedule_count = schedules.schedules.len();
        }
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
            if let Some(pid) = state.daemon_pid {
                self.daemon_heartbeat(pid).await?;
            }
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
        for schedule in schedules
            .schedules
            .into_iter()
            .filter(|schedule| schedule.enabled)
        {
            if !cron_matches(&schedule.cron, now)?
                || already_ran_this_minute(&state, &schedule.name, now)
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

    pub async fn daemon_start(&self, pid: u32) -> Result<AutopilotState> {
        fs::create_dir_all(&self.config_dir).await?;
        self.recover_stale_daemon().await?;
        if let Ok(existing_pid) = self.read_pid().await {
            if is_process_alive(existing_pid) {
                return Err(anyhow!(
                    "autopilot daemon already running with pid {existing_pid}"
                ));
            }
        }

        let mut state = self.status().await.unwrap_or_default();
        let now = Utc::now();
        state.running = true;
        state.daemon_pid = Some(pid);
        state.daemon_started_at = Some(now);
        state.daemon_heartbeat_at = Some(now);
        state.last_started_at = Some(now);
        state.schedule_count = self.list_schedules().await?.schedules.len();
        self.persist_state(&state).await?;
        fs::write(self.pid_path(), pid.to_string()).await?;
        Ok(state)
    }

    pub async fn daemon_stop(&self) -> Result<AutopilotState> {
        fs::create_dir_all(&self.config_dir).await?;
        let mut state = self.status().await.unwrap_or_default();
        state.running = false;
        state.daemon_pid = None;
        state.daemon_started_at = None;
        state.daemon_heartbeat_at = None;
        state.schedule_count = self.list_schedules().await?.schedules.len();
        self.persist_state(&state).await?;
        if path_exists(&self.pid_path()).await? {
            fs::remove_file(self.pid_path()).await?;
        }
        Ok(state)
    }

    pub async fn daemon_heartbeat(&self, pid: u32) -> Result<()> {
        let mut state = self.status().await?;
        if state.daemon_pid != Some(pid) {
            return Err(anyhow!("daemon heartbeat rejected for non-owner pid"));
        }
        state.daemon_heartbeat_at = Some(Utc::now());
        self.persist_state(&state).await
    }

    pub async fn read_pid(&self) -> Result<u32> {
        let raw = fs::read_to_string(self.pid_path()).await?;
        raw.trim()
            .parse::<u32>()
            .context("autopilot pid file is invalid")
    }

    async fn recover_stale_daemon(&self) -> Result<()> {
        if !path_exists(&self.pid_path()).await? {
            return Ok(());
        }
        let pid = self.read_pid().await?;
        if is_process_alive(pid) {
            return Ok(());
        }

        let mut state = if path_exists(&self.state_path()).await? {
            let content = fs::read_to_string(self.state_path()).await?;
            toml::from_str::<AutopilotState>(&content)?
        } else {
            AutopilotState::default()
        };
        state.running = false;
        state.daemon_pid = None;
        state.daemon_started_at = None;
        state.daemon_heartbeat_at = None;
        self.persist_state(&state).await?;
        fs::remove_file(self.pid_path()).await?;
        Ok(())
    }

    async fn persist_state(&self, state: &AutopilotState) -> Result<()> {
        fs::write(self.state_path(), toml::to_string_pretty(state)?).await?;
        Ok(())
    }
}

fn build_task_plan(prompt: &str) -> TaskPlan {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return TaskPlan {
            intent: TaskIntent::GuidanceOnly,
            steps: Vec::new(),
            warnings: vec!["Prompt was empty.".to_string()],
        };
    }

    if let Some(command) = extract_prefixed_command(trimmed) {
        return TaskPlan {
            intent: TaskIntent::DirectCommand,
            steps: vec![TaskPlanStep {
                description: "Execute explicit command".to_string(),
                command: Some(command),
                safety: PlanStepSafety::Safe,
            }],
            warnings: Vec::new(),
        };
    }

    let normalized = trimmed.to_lowercase();
    if normalized.contains("check system health") || normalized.contains("system health") {
        return TaskPlan {
            intent: TaskIntent::SystemHealth,
            steps: vec![
                plan_step("Read host uptime", "uptime"),
                plan_step("Check disk pressure", "df -h"),
                plan_step("Capture host kernel details", "uname -a"),
            ],
            warnings: Vec::new(),
        };
    }

    if normalized.contains("diagnostic") || normalized.contains("diagnostics") {
        return TaskPlan {
            intent: TaskIntent::Diagnostics,
            steps: vec![
                plan_step("Read host uptime", "uptime"),
                plan_step(
                    "List top processes by CPU and memory",
                    "ps -eo pid,comm,%cpu,%mem",
                ),
                plan_step("Capture filesystem usage", "df -h"),
            ],
            warnings: Vec::new(),
        };
    }

    if normalized == "git status" || normalized.contains("show git status") {
        return TaskPlan {
            intent: TaskIntent::RepoStatus,
            steps: vec![
                plan_step(
                    "Inspect git branch and worktree status",
                    "git status --short --branch",
                ),
                plan_step("Show current HEAD revision", "git rev-parse --short HEAD"),
            ],
            warnings: Vec::new(),
        };
    }

    if normalized.contains("list files") || normalized.contains("show files") {
        return TaskPlan {
            intent: TaskIntent::FileInspection,
            steps: vec![
                plan_step("Print current working directory", "pwd"),
                plan_step("List files in current directory", "ls"),
            ],
            warnings: Vec::new(),
        };
    }

    let first_word = trimmed.split_whitespace().next().unwrap_or_default();
    if is_allowed_program(first_word) {
        return TaskPlan {
            intent: TaskIntent::DirectCommand,
            steps: vec![TaskPlanStep {
                description: "Execute inferred command".to_string(),
                command: Some(trimmed.to_string()),
                safety: PlanStepSafety::ReviewRequired,
            }],
            warnings: vec![
                "Command inferred from prompt text; review before execution.".to_string(),
            ],
        };
    }

    TaskPlan {
        intent: TaskIntent::GuidanceOnly,
        steps: Vec::new(),
        warnings: vec![
            "No safe command template matched the prompt.".to_string(),
            "Use `run: <command>` for explicit execution.".to_string(),
        ],
    }
}

fn plan_step(description: &str, command: &str) -> TaskPlanStep {
    TaskPlanStep {
        description: description.to_string(),
        command: Some(command.to_string()),
        safety: PlanStepSafety::Safe,
    }
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

async fn execute_command_with_remote(
    command: &str,
    remote: &RemoteConfig,
) -> Result<CommandExecution> {
    if remote.enabled {
        return execute_remote_command(command, remote).await;
    }
    execute_command_local(command).await
}

async fn execute_command_local(command: &str) -> Result<CommandExecution> {
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

async fn execute_remote_command(command: &str, remote: &RemoteConfig) -> Result<CommandExecution> {
    let endpoint = remote
        .execute_endpoint
        .as_deref()
        .ok_or_else(|| anyhow!("remote mode enabled but execute_endpoint is not configured"))?;
    validate_command(command)?;

    let client = reqwest::Client::new();
    let attempts = remote.retries.saturating_add(1) as usize;
    let mut last_error = None;

    for _attempt in 0..attempts {
        let mut request = client
            .post(endpoint)
            .header("content-type", "application/json")
            .json(&json!({ "command": command }));
        if let Some(token) = remote.auth_token.as_deref() {
            request = request.bearer_auth(token);
        }

        match timeout(Duration::from_secs(remote.timeout_secs), request.send()).await {
            Ok(Ok(response)) => {
                return parse_remote_execution_response(response, command).await;
            }
            Ok(Err(error)) => {
                last_error = Some(anyhow!("remote execute request failed: {error}"));
            }
            Err(_) => {
                last_error = Some(anyhow!(
                    "remote execute timed out after {}s",
                    remote.timeout_secs
                ));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("remote execute failed")))
}

async fn parse_remote_execution_response(
    response: reqwest::Response,
    command: &str,
) -> Result<CommandExecution> {
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .context("remote execution returned invalid JSON")?;

    if status != StatusCode::OK {
        let message = body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("remote execution error");
        return Err(anyhow!("{message}"));
    }

    Ok(CommandExecution {
        command: command.to_string(),
        exit_code: body.get("exit_code").and_then(Value::as_i64).unwrap_or(-1) as i32,
        stdout: body
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        stderr: body
            .get("stderr")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        success: body
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        executed_at: Utc::now(),
    })
}

fn validate_command(command: &str) -> Result<()> {
    if command
        .chars()
        .any(|ch| matches!(ch, '|' | '&' | ';' | '>' | '<' | '`'))
    {
        return Err(anyhow!(
            "unsafe shell operator rejected in command: {command}"
        ));
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

async fn load_remote_config(config_dir: PathBuf) -> Result<RemoteConfig> {
    let path = remote_path(&config_dir);
    if !path_exists(&path).await? {
        return Ok(RemoteConfig::default());
    }
    let content = fs::read_to_string(path).await?;
    let parsed: RemoteConfig = toml::from_str(&content)?;
    Ok(parsed)
}

async fn fetch_remote_status(remote: &RemoteConfig) -> Result<AutopilotState> {
    let endpoint = remote
        .status_endpoint
        .as_deref()
        .ok_or_else(|| anyhow!("remote mode enabled but status_endpoint is not configured"))?;
    let client = reqwest::Client::new();
    let attempts = remote.retries.saturating_add(1) as usize;
    let mut last_error = None;

    for _attempt in 0..attempts {
        let mut request = client.get(endpoint);
        if let Some(token) = remote.auth_token.as_deref() {
            request = request.bearer_auth(token);
        }

        match timeout(Duration::from_secs(remote.timeout_secs), request.send()).await {
            Ok(Ok(response)) => {
                let status = response.status();
                let body: Value = response
                    .json()
                    .await
                    .context("remote status returned invalid JSON")?;
                if status != StatusCode::OK {
                    let message = body
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("remote status request failed");
                    return Err(anyhow!("{message}"));
                }
                let parsed: AutopilotState = serde_json::from_value(body)
                    .context("remote status payload does not match autopilot state schema")?;
                return Ok(parsed);
            }
            Ok(Err(error)) => {
                last_error = Some(anyhow!("remote status request failed: {error}"));
            }
            Err(_) => {
                last_error = Some(anyhow!(
                    "remote status timed out after {}s",
                    remote.timeout_secs
                ));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("remote status request failed")))
}

fn is_process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
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
            lines.push(format!(
                "  stdout: {}",
                command.stdout.trim().replace('\n', " | ")
            ));
        }
        if !command.stderr.trim().is_empty() {
            lines.push(format!(
                "  stderr: {}",
                command.stderr.trim().replace('\n', " | ")
            ));
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
        let parsed: u32 = candidate
            .parse()
            .with_context(|| format!("invalid cron value: {candidate}"))?;
        if parsed == value {
            return Ok(true);
        }
    }

    Ok(false)
}

fn already_ran_this_minute(
    state: &AutopilotState,
    schedule_name: &str,
    now: DateTime<Utc>,
) -> bool {
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

    #[test]
    fn planner_builds_multi_step_health_plan() {
        let plan = build_task_plan("check system health");
        assert_eq!(plan.intent, TaskIntent::SystemHealth);
        assert!(plan.steps.len() >= 3);
        assert!(plan.warnings.is_empty());
    }

    #[test]
    fn planner_guidance_only_for_unmatched_prompt() {
        let plan = build_task_plan("write me a poem about stars");
        assert_eq!(plan.intent, TaskIntent::GuidanceOnly);
        assert!(plan.steps.is_empty());
        assert!(!plan.warnings.is_empty());
    }

    #[tokio::test]
    async fn stale_pid_is_recovered() {
        let config_dir = temp_config_dir("stale-pid");
        fs::create_dir_all(&config_dir).await.unwrap();
        fs::write(config_dir.join("autopilot.pid"), "999999")
            .await
            .unwrap();
        fs::write(
            config_dir.join("autopilot_state.toml"),
            toml::to_string_pretty(&AutopilotState {
                running: true,
                daemon_pid: Some(999999),
                daemon_started_at: Some(Utc::now()),
                daemon_heartbeat_at: Some(Utc::now()),
                last_started_at: Some(Utc::now()),
                last_tick_at: Some(Utc::now()),
                schedule_count: 0,
                recent_runs: Vec::new(),
            })
            .unwrap(),
        )
        .await
        .unwrap();

        let state = AutopilotService::new(config_dir).status().await.unwrap();
        assert!(!state.running);
        assert!(state.daemon_pid.is_none());
    }
}
