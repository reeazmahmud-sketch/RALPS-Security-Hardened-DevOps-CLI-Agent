# RALPS Configuration

RALPS stores local configuration in `~/.ralps/`.

## `auth.toml`

```toml
api_key = "shared-fallback-key"
anthropic_api_key = "optional-provider-specific-key"
openai_api_key = "optional-provider-specific-key"
gemini_api_key = "optional-provider-specific-key"
```

Provider-specific environment variables override file-based values:

- `ANTHROPIC_API_KEY`
- `OPENAI_API_KEY`
- `GEMINI_API_KEY`

## `autopilot.toml`

```toml
[[schedules]]
name = "health-check"
cron = "*/5 * * * *"
prompt = "check system health"
command = "echo healthy"
provider = "anthropic"
enabled = true
```

Supported cron syntax is the common five-field format with `*`, comma-separated values, and `*/step` intervals.
