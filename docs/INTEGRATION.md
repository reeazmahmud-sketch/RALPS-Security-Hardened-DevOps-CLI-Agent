# RALPS Integration

RALPS exposes local integration points through its MCP crates and file-based runtime state.

## MCP

The MCP server crate handles JSON-RPC 2.0 requests for:

- `initialize`
- `tools/list`
- `tools/call`

The proxy crate can construct a request, forward it to the server, and validate the response.

## Autopilot state

Autopilot writes runtime state to `~/.ralps/autopilot_state.toml`, including:

- whether autopilot is currently running
- the last scheduler tick
- the recent run history for configured schedules

## Future extension points

The current implementation is intentionally local-first. Future work can add remote transports, richer MCP tools, and more advanced task planning without changing the existing CLI surface.
