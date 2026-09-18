use anyhow::Result;
use serde_json::{json, Value};
use shared::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

pub struct McpServer;

impl McpServer {
    pub fn handle(&self, payload: &str) -> Result<String> {
        let request: JsonRpcRequest = serde_json::from_str(payload)?;
        let response = match request.method.as_str() {
            "initialize" => success(
                request.id,
                json!({
                    "serverInfo": {"name": "ralps-mcp", "version": "0.1.1"},
                    "capabilities": {"tools": {"listChanged": false}},
                }),
            ),
            "tools/list" => success(
                request.id,
                json!({
                    "tools": [
                        {"name": "echo", "description": "Echo provided text"},
                        {"name": "health-check", "description": "Return a simple health payload"}
                    ]
                }),
            ),
            "tools/call" => match request.params {
                Some(params) => handle_tool_call(request.id, params),
                None => error(request.id, -32602, "Missing tool call params"),
            },
            _ => error(request.id, -32601, "Method not found"),
        };

        Ok(serde_json::to_string(&response)?)
    }
}

fn handle_tool_call(id: Value, params: Value) -> JsonRpcResponse {
    match params.get("name").and_then(Value::as_str) {
        Some("echo") => {
            let text = params
                .get("arguments")
                .and_then(|value| value.get("text"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            success(id, json!({"content": [{"type": "text", "text": text}]}))
        }
        Some("health-check") => success(
            id,
            json!({
                "content": [{"type": "text", "text": "ralps-mcp ok"}],
                "healthy": true,
            }),
        ),
        Some(_) => error(id, -32602, "Unknown tool"),
        None => error(id, -32602, "Tool name is required"),
    }
}

fn success(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
        error: None,
    }
}

fn error(id: Value, code: i64, message: &str) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
            data: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_tool_listing() {
        let server = McpServer;
        let response = server
            .handle(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
            .unwrap();
        assert!(response.contains("health-check"));
    }
}
