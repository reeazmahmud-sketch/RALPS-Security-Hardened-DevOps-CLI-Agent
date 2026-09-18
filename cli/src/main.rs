use anyhow::{anyhow, Result};
use api::{AgentEngine, AuthService, AutopilotService};
use clap::{Args, Parser, Subcommand};
use colored::Colorize;
use shared::{AgentRequest, ExecutionMode, Provider};
use std::io::{self, Read};

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
                println!("{} {}", "result:".green().bold(), result.output);
            }
        },
        Commands::Up => {
            let state = AutopilotService::default().up().await?;
            println!(
                "{} running={} schedules={}",
                "autopilot:".green().bold(),
                state.running,
                state.schedule_count
            );
        }
        Commands::Down => {
            let state = AutopilotService::default().down().await?;
            println!(
                "{} running={} schedules={}",
                "autopilot:".yellow().bold(),
                state.running,
                state.schedule_count
            );
        }
        Commands::Autopilot(args) => match args.command {
            AutopilotCommands::Status => {
                let state = AutopilotService::default().status().await?;
                println!(
                    "{} running={} schedules={}",
                    "autopilot:".blue().bold(),
                    state.running,
                    state.schedule_count
                );
            }
            AutopilotCommands::Schedule(schedule) => match schedule.command {
                ScheduleCommands::List => {
                    let schedules = AutopilotService::default().list_schedules().await?;
                    if schedules.schedules.is_empty() {
                        println!("No schedules configured.");
                    } else {
                        for schedule in schedules.schedules {
                            println!(
                                "- {} ({}) -> {}",
                                schedule.name, schedule.cron, schedule.prompt
                            );
                        }
                    }
                }
            },
        },
    }

    Ok(())
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
