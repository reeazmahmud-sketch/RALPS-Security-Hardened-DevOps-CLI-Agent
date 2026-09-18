use anyhow::Result;
use serde_json::Value;
use shared::{JsonRpcRequest, JsonRpcResponse};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct McpClient {
    next_id: AtomicU64,
}

impl Default for McpClient {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
        }
    }
}

impl McpClient {
    pub fn build_request(&self, method: &str, params: Option<Value>) -> Result<String> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Value::from(self.next_id.fetch_add(1, Ordering::Relaxed)),
            method: method.to_string(),
            params,
        };

        Ok(serde_json::to_string(&request)?)
    }

    pub fn parse_response(&self, payload: &str) -> Result<JsonRpcResponse> {
        Ok(serde_json::from_str(payload)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_jsonrpc_request() {
        let client = McpClient::default();
        let request = client.build_request("tools/list", None).unwrap();
        assert!(request.contains("tools/list"));
    }
}
