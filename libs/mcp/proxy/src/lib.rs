use anyhow::{anyhow, Context, Result};
use client::McpClient;
use serde_json::Value;
use server::McpServer;
use shared::RemoteConfig;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

pub struct McpProxy {
    client: McpClient,
    transport: Transport,
}

enum Transport {
    Local(McpServer),
    Remote(RemoteTransport),
}

struct RemoteTransport {
    endpoint: String,
    token: Option<String>,
    timeout_secs: u64,
    retries: u8,
}

impl Default for McpProxy {
    fn default() -> Self {
        let remote = load_remote_config().unwrap_or_default();
        let transport = if remote.enabled {
            Transport::Remote(RemoteTransport {
                endpoint: remote.mcp_endpoint.unwrap_or_default(),
                token: remote.auth_token,
                timeout_secs: remote.timeout_secs,
                retries: remote.retries,
            })
        } else {
            Transport::Local(McpServer::default())
        };

        Self {
            client: McpClient::default(),
            transport,
        }
    }
}

impl McpProxy {
    pub fn proxy(&self, payload: &str) -> Result<String> {
        let response = match &self.transport {
            Transport::Local(server) => server.handle(payload)?,
            Transport::Remote(remote) => remote.send(payload)?,
        };
        let parsed = self.client.parse_response(&response)?;
        if let Some(error) = parsed.error {
            return Err(anyhow!(error.message));
        }

        Ok(response)
    }

    pub fn call(&self, method: &str, params: Option<Value>) -> Result<String> {
        let request = self.client.build_request(method, params)?;
        self.proxy(&request)
    }
}

impl RemoteTransport {
    fn send(&self, payload: &str) -> Result<String> {
        if self.endpoint.trim().is_empty() {
            return Err(anyhow!(
                "remote MCP mode enabled but mcp_endpoint is not configured"
            ));
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(self.timeout_secs))
            .build()
            .context("failed to build remote MCP client")?;

        let attempts = self.retries.saturating_add(1) as usize;
        let mut last_error = None;
        for _attempt in 0..attempts {
            let mut request = client
                .post(&self.endpoint)
                .header("content-type", "application/json")
                .body(payload.to_string());
            if let Some(token) = self.token.as_deref() {
                request = request.bearer_auth(token);
            }

            match request.send() {
                Ok(response) => {
                    if response.status().is_success() {
                        return response
                            .text()
                            .context("failed to read remote MCP response body");
                    }
                    last_error = Some(anyhow!(
                        "remote MCP request failed with status {}",
                        response.status()
                    ));
                }
                Err(error) => {
                    last_error = Some(anyhow!("remote MCP request failed: {error}"));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("remote MCP request failed")))
    }
}

fn load_remote_config() -> Result<RemoteConfig> {
    if let Ok(value) = env::var("RALPS_REMOTE_ENABLED") {
        if value.eq_ignore_ascii_case("true") {
            return Ok(RemoteConfig {
                enabled: true,
                mcp_endpoint: env::var("RALPS_REMOTE_MCP_ENDPOINT").ok(),
                execute_endpoint: env::var("RALPS_REMOTE_EXECUTE_ENDPOINT").ok(),
                status_endpoint: env::var("RALPS_REMOTE_STATUS_ENDPOINT").ok(),
                auth_token: env::var("RALPS_REMOTE_AUTH_TOKEN").ok(),
                timeout_secs: env::var("RALPS_REMOTE_TIMEOUT_SECS")
                    .ok()
                    .and_then(|raw| raw.parse::<u64>().ok())
                    .unwrap_or(10),
                retries: env::var("RALPS_REMOTE_RETRIES")
                    .ok()
                    .and_then(|raw| raw.parse::<u8>().ok())
                    .unwrap_or(2),
            });
        }
    }

    let path = env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".ralps")
        .join("remote.toml");
    if !path.exists() {
        return Ok(RemoteConfig::default());
    }
    let content = fs::read_to_string(path)?;
    let parsed: RemoteConfig = toml::from_str(&content)?;
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn proxies_tool_call() {
        let proxy = McpProxy {
            client: McpClient::default(),
            transport: Transport::Local(McpServer::default()),
        };
        let output = proxy
            .call(
                "tools/call",
                Some(json!({"name": "echo", "arguments": {"text": "deploy"}})),
            )
            .unwrap();
        assert!(output.contains("deploy"));
    }

    #[test]
    fn fails_closed_for_invalid_remote_config() {
        let proxy = McpProxy {
            client: McpClient::default(),
            transport: Transport::Remote(RemoteTransport {
                endpoint: String::new(),
                token: None,
                timeout_secs: 1,
                retries: 0,
            }),
        };
        let error = proxy.proxy(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
        assert!(error.is_err());
    }
}
