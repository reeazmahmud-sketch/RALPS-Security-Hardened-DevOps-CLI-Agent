# RALPS Integration

RALPS exposes local integration points through its MCP crates and file-based runtime state.

## MCP

The MCP server crate handles JSON-RPC 2.0 requests for:

- `initialize`
- `tools/list`
- `tools/call`

The MCP server exposes a centralized tool registry:

- `echo`
- `health-check`
- `task-plan`
- `autopilot-status`
- `autopilot-schedule-list`
- `autopilot-recent-runs`
- `safe-command`

The proxy crate can construct a request, forward it locally or remotely (when enabled), and validate the response.

## Autopilot state

Autopilot writes runtime state to `~/.ralps/autopilot_state.toml`, including:

- whether autopilot is currently running
- the last scheduler tick
- the recent run history for configured schedules

## Future extension points

The implementation is local-first by default. Optional `~/.ralps/remote.toml` settings can enable remote MCP forwarding and remote execution/status surfaces with auth, timeout, retry, and fail-closed behavior.
