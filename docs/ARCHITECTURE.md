# RALPS Architecture

RALPS is a Rust workspace with separate crates for the CLI, TUI, API orchestration, provider integrations, shared models, and MCP components.

## Runtime flow

1. `ralps` parses commands in the CLI crate.
2. The API crate loads local configuration from `~/.ralps/`.
3. The agent engine derives a structured task plan (intent, steps, safety) from the prompt.
4. Allowed local commands run with captured stdout/stderr and a timeout.
5. When provider credentials are configured, the AI crate requests a provider summary from Anthropic, OpenAI, or Gemini.
6. Autopilot daemon evaluates configured schedules, writes heartbeat/PID state, and records recent run history.

## Security posture

- API keys stay local in `~/.ralps/auth.toml` or provider environment variables.
- Command execution rejects shell operators and restricts execution to an allowlist of operational tools.
- Command output is captured and truncated to keep responses bounded.
- MCP support provides a centralized tool registry and typed tool argument validation for local-first operations.
- Optional remote integration surfaces can forward MCP and execution/status operations through authenticated HTTP endpoints.
