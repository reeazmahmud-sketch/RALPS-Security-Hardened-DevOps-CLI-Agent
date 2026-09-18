use anyhow::{anyhow, Result};
use api::{AgentEngine, AuthService, AutopilotService};
use clap::{Args, Parser, Subcommand};
use colored::Colorize;
use shared::{AgentRequest, ExecutionMode, Provider};
use std::io::{self, Read};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

#[derive(Parser, Debug)]
#[command(name = "ralps", about = "ralps - security-hardened devops CLI agent")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Auth(AuthArgs),
    Agent(AgentArgs),
    Up,
    Down,
    Autopilot(AutopilotArgs),
}

#[derive(Args, Debug)]
struct AuthArgs {
    #[command(subcommand)]
    command: AuthCommands,
}

#[derive(Subcommand, Debug)]
enum AuthCommands {
    Login {
        #[arg(long)]
        api_key: String,
    },
}

#[derive(Args, Debug)]
struct AgentArgs {
    #[command(subcommand)]
    command: AgentCommands,
}

#[derive(Subcommand, Debug)]
enum AgentCommands {
    Run {
        #[arg(long)]
        interactive: bool,
        #[arg(long = "async")]
        async_mode: bool,
        #[arg(long)]
        prompt: Option<String>,
        #[arg(long, default_value = "anthropic")]
        provider: String,
    },
}

#[derive(Args, Debug)]
struct AutopilotArgs {
    #[command(subcommand)]
    command: AutopilotCommands,
}

#[derive(Subcommand, Debug)]
enum AutopilotCommands {
    Status,
    Schedule(ScheduleArgs),
    Daemon(DaemonArgs),
}

#[derive(Args, Debug)]
struct ScheduleArgs {
    #[command(subcommand)]
    command: ScheduleCommands,
}

#[derive(Subcommand, Debug)]
enum ScheduleCommands {
    List,
}

#[derive(Args, Debug)]
struct DaemonArgs {
    #[command(subcommand)]
    command: DaemonCommands,
}

#[derive(Subcommand, Debug)]
enum DaemonCommands {
    Start,
    Stop,
    Status,
    #[command(hide = true)]
    Run,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{} {error}", "error:".red().bold());
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Auth(args) => match args.command {
            AuthCommands::Login { api_key } => {
                let path = AuthService::default().login(&api_key).await?;
                println!(
                    "{} Saved credentials to {}",
                    "ok:".green().bold(),
                    path.display()
                );
            }
        },
        Commands::Agent(args) => match args.command {
            AgentCommands::Run {
                interactive,
                async_mode,
                prompt,
                provider,
            } => {
                let provider = provider.parse::<Provider>().map_err(anyhow::Error::msg)?;
                let mode = resolve_mode(interactive, async_mode)?;
                let prompt = resolve_prompt(mode, prompt)?;

                let result = AgentEngine::default()
                    .run(AgentRequest {
                        prompt,
                        provider,
                        mode,
                    })
                    .await?;

                println!("{} {}", "job:".cyan().bold(), result.id);
                println!("{} {:?}", "intent:".cyan().bold(), result.plan.intent);
                if !result.plan.steps.is_empty() {
                    println!("{}", "plan:".cyan().bold());
                    for (index, step) in result.plan.steps.iter().enumerate() {
                        println!(
                            "  {}. {} [{}]{}",
                            index + 1,
                            step.description,
                            match step.safety {
                                shared::PlanStepSafety::Safe => "safe",
                                shared::PlanStepSafety::ReviewRequired => "review",
                            },
                            step.command
                                .as_deref()
                                .map(|command| format!(" -> {command}"))
                                .unwrap_or_default()
                        );
                    }
                }
                println!(
                    "{} {}",
                    "mode:".cyan().bold(),
                    if result.async_job {
                        "async"
                    } else {
                        "interactive"
                    }
                );
                println!("{}\n{}", "result:".green().bold(), result.output);
            }
        },
        Commands::Up => {
            start_daemon().await?;
        }
        Commands::Down => {
            stop_daemon().await?;
        }
        Commands::Autopilot(args) => match args.command {
            AutopilotCommands::Status => {
                let state = AutopilotService::default().status().await?;
                println!(
                    "{} running={} daemon_pid={} heartbeat={} schedules={} last_tick={}",
                    "autopilot:".blue().bold(),
                    state.running,
                    state
                        .daemon_pid
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    state
                        .daemon_heartbeat_at
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_else(|| "never".to_string()),
                    state.schedule_count,
                    state
                        .last_tick_at
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_else(|| "never".to_string())
                );
                if !state.recent_runs.is_empty() {
                    println!("recent runs:");
                    for run in state.recent_runs.iter().rev().take(5) {
                        println!(
                            "- {} [{}] {}",
                            run.schedule_name,
                            if run.success { "ok" } else { "failed" },
                            run.summary
                        );
                    }
                }
            }
            AutopilotCommands::Schedule(schedule) => match schedule.command {
                ScheduleCommands::List => {
                    let schedules = AutopilotService::default().list_schedules().await?;
                    if schedules.schedules.is_empty() {
                        println!("No schedules configured.");
                    } else {
                        for schedule in schedules.schedules {
                            println!(
                                "- {} ({}) enabled={} provider={} target={}",
                                schedule.name,
                                schedule.cron,
                                schedule.enabled,
                                schedule
                                    .provider
                                    .map(|provider| format!("{:?}", provider))
                                    .unwrap_or_else(|| "Anthropic".to_string()),
                                schedule.command.unwrap_or(schedule.prompt)
                            );
                        }
                    }
                }
            },
            AutopilotCommands::Daemon(args) => match args.command {
                DaemonCommands::Start => start_daemon().await?,
                DaemonCommands::Stop => stop_daemon().await?,
                DaemonCommands::Status => {
                    let state = AutopilotService::default().status().await?;
                    println!(
                        "{} running={} pid={} heartbeat={}",
                        "daemon:".blue().bold(),
                        state.running,
                        state
                            .daemon_pid
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        state
                            .daemon_heartbeat_at
                            .map(|value| value.to_rfc3339())
                            .unwrap_or_else(|| "never".to_string())
                    );
                }
                DaemonCommands::Run => run_daemon().await?,
            },
        },
    }

    Ok(())
}

async fn start_daemon() -> Result<()> {
    let exe = std::env::current_exe()?;
    let mut child = Command::new(exe)
        .args(["autopilot", "daemon", "run"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let pid = child
        .id()
        .ok_or_else(|| anyhow!("daemon pid unavailable"))?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let state = AutopilotService::default().status().await?;
    if !state.running {
        child.start_kill()?;
        return Err(anyhow!("autopilot daemon failed to start"));
    }
    println!(
        "{} running={} pid={}",
        "autopilot:".green().bold(),
        state.running,
        pid
    );
    Ok(())
}

async fn stop_daemon() -> Result<()> {
    let service = AutopilotService::default();
    let state = service.status().await?;
    if let Some(pid) = state.daemon_pid {
        let _ = Command::new("kill").arg(pid.to_string()).status().await;
    }
    let stopped = service.daemon_stop().await?;
    println!(
        "{} running={} schedules={}",
        "autopilot:".yellow().bold(),
        stopped.running,
        stopped.schedule_count
    );
    Ok(())
}

async fn run_daemon() -> Result<()> {
    let service = AutopilotService::default();
    service.daemon_start(std::process::id()).await?;

    let run_result = tokio::select! {
        result = service.run_loop(Duration::from_secs(30)) => result,
        _ = tokio::signal::ctrl_c() => Ok(()),
    };
    let _ = service.daemon_stop().await;
    run_result
}

fn resolve_mode(interactive: bool, async_mode: bool) -> Result<ExecutionMode> {
    if interactive && async_mode {
        return Err(anyhow!("Use only one mode: --interactive or --async"));
    }

    if async_mode {
        Ok(ExecutionMode::Async)
    } else {
        Ok(ExecutionMode::Interactive)
    }
}

fn resolve_prompt(mode: ExecutionMode, prompt: Option<String>) -> Result<String> {
    if let Some(prompt) = prompt {
        return Ok(prompt);
    }

    if matches!(mode, ExecutionMode::Interactive) {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        let trimmed = buffer.trim();
        if trimmed.is_empty() {
            Err(anyhow!(
                "No prompt provided. Pass --prompt or pipe input to stdin."
            ))
        } else {
            Ok(trimmed.to_string())
        }
    } else {
        Err(anyhow!("--prompt is required in async mode"))
    }
}
