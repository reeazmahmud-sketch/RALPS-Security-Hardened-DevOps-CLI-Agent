# RALPS - Security-Hardened DevOps CLI Agent

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Rust Edition 2021](https://img.shields.io/badge/Rust-2021-orange)](https://www.rust-lang.org/)
[![Status: Prototype](https://img.shields.io/badge/Status-Prototype-yellow)]()

RALPS is a security-hardened DevOps CLI prototype for local operational workflows. It keeps credentials local, can execute a bounded set of approved commands, and can optionally request AI summaries from supported providers.

## Current Capabilities

- `ralps auth login --api-key ...` stores a local fallback API key in `~/.ralps/auth.toml`
- `ralps agent run` builds a structured task plan (intent, steps, safety) before executing safe bounded commands
- AI summaries can be requested from Anthropic, OpenAI, and Gemini when credentials are configured
- `ralps up` / `ralps down` now manage a background autopilot daemon with PID and heartbeat tracking
- The TUI reads real autopilot status, schedules, and recent run history
- MCP crates expose a centralized tool registry with task planning, autopilot operations, and safe command execution tools
- Optional remote integrations can forward MCP calls and command/status operations to configured endpoints

## Workspace Layout

- **cli/** - CLI entrypoint and command routing
- **tui/** - Terminal UI for autopilot status and schedule visibility
- **libs/ai/** - Provider HTTP integrations and auth resolution
- **libs/api/** - Config loading, command execution, and autopilot scheduling
- **libs/shared/** - Shared models for agent, auth, autopilot, and MCP data
- **libs/mcp/** - Local JSON-RPC client, server, and proxy components

## Quick Start

### Prerequisites

- macOS or Linux
- Rust 1.70+ (`rustup` recommended)

### Installation

```bash
git clone https://github.com/reeazmahmud-sketch/ralps.git
cd ralps
cargo build
```

### First Run

```bash
ralps auth login --api-key "$ANTHROPIC_API_KEY"
ralps agent run --prompt "run: git status"
ralps agent run --prompt "check system health"
```

### Autopilot

```toml
# ~/.ralps/autopilot.toml
[[schedules]]
name = "health-check"
cron = "*/5 * * * *"
prompt = "check system health"
command = "echo healthy"
provider = "anthropic"
enabled = true
```

```bash
ralps up
ralps down
ralps autopilot status
ralps autopilot schedule list
```

## Authentication

RALPS resolves provider credentials in this order:

1. Provider-specific environment variable
2. Provider-specific key in `~/.ralps/auth.toml`
3. Generic `api_key` in `~/.ralps/auth.toml`

Supported environment variables:

- `ANTHROPIC_API_KEY`
- `OPENAI_API_KEY`
- `GEMINI_API_KEY`

## Command Safety

RALPS only executes allowlisted programs and rejects shell operators such as `|`, `&`, `;`, `<`, `>`, and backticks.

Current allowlist includes common local inspection and DevOps commands such as:

- `git`
- `cargo`
- `kubectl`
- `docker`
- `terraform`
- `ls`, `pwd`, `cat`, `grep`, `rg`, `find`
- `uptime`, `uname`, `df`, `ps`, `date`, `whoami`

## MCP Support

Current MCP support includes:

- JSON-RPC request parsing
- `initialize`
- `tools/list`
- `tools/call`
- centralized tool registry for `echo`, `health-check`, `task-plan`, `autopilot-status`, `autopilot-schedule-list`, `autopilot-recent-runs`, and `safe-command`

## Testing

```bash
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Configuration](docs/CONFIG.md)
- [Integration](docs/INTEGRATION.md)

## Roadmap

- [x] Workspace setup
- [x] CLI commands
- [x] Provider-aware auth loading
- [x] Bounded local command execution
- [x] Local autopilot scheduler loop
- [x] Basic MCP JSON-RPC support
- [x] TUI status views
- [x] Richer task planning
- [x] Expanded MCP tool catalog
- [x] Background daemonization
- [x] Remote integration surfaces

**Version:** 0.1.1  
**Last Updated:** September 18, 2026
