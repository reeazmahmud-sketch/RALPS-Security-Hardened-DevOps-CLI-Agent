# RALPS - Security-Hardened DevOps CLI Agent

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Rust Edition 2021](https://img.shields.io/badge/Rust-2021-orange)](https://www.rust-lang.org/)
[![Status: Active Development](https://img.shields.io/badge/Status-Active%20Development-brightgreen)]()

RALPS is a security-hardened DevOps AI agent that runs in your terminal. It generates infrastructure code, debugs Kubernetes, configures CI/CD, and automates deployments — **without giving the LLM keys to production**.

## 🎯 Features

### Infrastructure & Deployment
- 🏗️ **Infrastructure as Code (IaC)** - Automate infrastructure provisioning
- 🐳 **Container Orchestration** - Kubernetes & Docker integration
- 🔄 **CI/CD Pipeline Automation** - Complete pipeline configuration
- 🌍 **Multi-Environment Management** - dev, staging, production
- 🎨 **Blue-Green & Canary Deployments** - Advanced deployment strategies

### Monitoring & Observability
- 📊 **Real-Time Metrics & Dashboards** - Live performance tracking
- 📝 **Log Aggregation & Analysis** - Centralized logging
- ⚠️ **Alert Management** - Intelligent alerting system
- 🔍 **Distributed Tracing** - Request flow tracking

### Automation & Orchestration
- ⚡ **Workflow Automation** - Event-driven workflows
- ⏱️ **Task Scheduling** - Cron-based task execution
- 🔙 **Automated Rollbacks** - Instant recovery
- 🩹 **Self-Healing** - Automatic issue remediation

## 🏗️ Architecture

RALPS is built as a modular Rust workspace with clear separation of concerns:

- **cli/** - Main binary crate with CLI commands
- **tui/** - Terminal UI layer (ratatui-based)
- **libs/ai/** - LLM provider abstraction (Anthropic, OpenAI, Gemini)
- **libs/api/** - API client and context management
- **libs/shared/** - Shared types and models
- **libs/mcp/** - Model Context Protocol implementation

## 🚀 Quick Start

### Prerequisites
- macOS 10.15+
- Rust 1.70+ (install via `rustup`)

### Installation

```bash
git clone https://github.com/reeazmahmud-sketch/ralps.git
cd ralps

# Build for development
cargo build

# Build optimized release
cargo build --release
```

### First Run

```bash
# Login with API key
ralps auth login --api-key $RALPS_API_KEY

# Start interactive agent
ralps agent run --interactive

# Or run async mode
ralps agent run --async --prompt "Check system health"
```

## ⚙️ Configuration

Configuration files are stored in `~/.ralps/`:

```toml
# config.toml - Main configuration
[default]
provider = "anthropic"
model = "claude-3-opus-20240229"

# autopilot.toml - Schedules and channels
[[schedules]]
name = "health-check"
cron = "*/5 * * * *"
prompt = "Check system health"
```

## 📝 CLI Commands

```bash
# Interactive agent mode
ralps agent run --interactive

# Async execution
ralps agent run --async --prompt "Deploy to production"

# Autopilot system
ralps up                          # Start autopilot
ralps down                        # Stop autopilot
ralps autopilot status            # Check status
ralps autopilot schedule list     # List schedules
```

## 🔒 Security

- ✅ No production keys sent to LLM
- ✅ Credentials stored locally and readonly
- ✅ Complete audit logging
- ✅ Tool approval workflow
- ✅ Sandboxed execution environment

## 🔌 Future RAAL Integration

RALPS is designed as a standalone, modular system with clear integration points for future connection to RAAL (Reeaz Agentic Autonomy Life):

- **Event Bus** - Subscribe to agent events
- **API Gateway** - Call RALPS from external agents
- **Checkpoint Storage** - Share session state
- **Message Routing** - Route messages between agents

Integration will be opt-in and non-breaking. See `docs/INTEGRATION.md` (coming soon).

## 🧪 Testing

```bash
cargo test --workspace
cargo test --workspace --lib
cargo clippy --all-targets
cargo fmt --check
```

## 📚 Documentation

- [Architecture](docs/ARCHITECTURE.md) - System design details
- [Configuration](docs/CONFIG.md) - Config file reference
- [Integration](docs/INTEGRATION.md) - Future RAAL integration (coming soon)

## 📄 License

Apache License 2.0 - see [LICENSE](LICENSE)

## 🛣️ Roadmap

- [x] Workspace setup
- [x] Core agent execution engine
- [x] CLI commands
- [x] LLM integrations
- [x] TUI implementation
- [x] Autopilot system
- [ ] MCP support
- [ ] Docker support
- [ ] macOS distribution
- [ ] RAAL integration interface

---

**Version:** 0.1.1  
**Last Updated:** September 15, 2026  
**License:** Apache 2.0
