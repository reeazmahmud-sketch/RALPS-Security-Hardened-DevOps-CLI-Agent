use anyhow::{anyhow, Result};
use client::McpClient;
use serde_json::Value;
use server::McpServer;

pub struct McpProxy {
    client: McpClient,
    server: McpServer,
}

impl Default for McpProxy {
    fn default() -> Self {
        Self {
            client: McpClient::default(),
            server: McpServer,
        }
    }
}

impl McpProxy {
    pub fn proxy(&self, payload: &str) -> Result<String> {
        let response = self.server.handle(payload)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn proxies_tool_call() {
        let proxy = McpProxy::default();
        let output = proxy
            .call(
                "tools/call",
                Some(json!({"name": "echo", "arguments": {"text": "deploy"}})),
            )
            .unwrap();
        assert!(output.contains("deploy"));
    }
}
