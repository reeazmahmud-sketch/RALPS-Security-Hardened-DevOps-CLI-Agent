use anyhow::Result;
use api::{AgentEngine, AutopilotService};
use serde_json::{json, Value};
use shared::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

#[derive(Default)]
pub struct McpServer {
    tools: ToolRegistry,
}

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
            "tools/list" => success(request.id, json!({ "tools": self.tools.list() })),
            "tools/call" => match request.params {
                Some(params) => self.tools.call(request.id, params),
                None => error(request.id, -32602, "Missing tool call params"),
            },
            _ => error(request.id, -32601, "Method not found"),
        };

        Ok(serde_json::to_string(&response)?)
    }
}

#[derive(Default)]
struct ToolRegistry;

impl ToolRegistry {
    fn list(&self) -> Vec<Value> {
        vec![
            tool_def(
                "echo",
                "Echo provided text",
                json!({"type":"object","required":["text"],"properties":{"text":{"type":"string"}}}),
            ),
            tool_def(
                "health-check",
                "Return a health payload",
                json!({"type":"object","properties":{}}),
            ),
            tool_def(
                "task-plan",
                "Build a structured execution plan from prompt text",
                json!({"type":"object","required":["prompt"],"properties":{"prompt":{"type":"string"}}}),
            ),
            tool_def(
                "autopilot-status",
                "Read autopilot runtime status",
                json!({"type":"object","properties":{}}),
            ),
            tool_def(
                "autopilot-schedule-list",
                "List configured autopilot schedules",
                json!({"type":"object","properties":{}}),
            ),
            tool_def(
                "autopilot-recent-runs",
                "Read recent autopilot runs",
                json!({"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":20}}}),
            ),
            tool_def(
                "safe-command",
                "Execute an allowlisted command through RALPS safety controls",
                json!({"type":"object","required":["command"],"properties":{"command":{"type":"string"}}}),
            ),
        ]
    }

    fn call(&self, id: Value, params: Value) -> JsonRpcResponse {
        let name = match params.get("name").and_then(Value::as_str) {
            Some(name) => name,
            None => return error(id, -32602, "Tool name is required"),
        };
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        match name {
            "echo" => {
                let text = match required_string(&arguments, "text") {
                    Ok(value) => value,
                    Err(message) => return error(id, -32602, &message),
                };
                success(id, json!({"content":[{"type":"text","text":text}]}))
            }
            "health-check" => success(
                id,
                json!({"content":[{"type":"text","text":"ralps-mcp ok"}],"healthy":true}),
            ),
            "task-plan" => {
                let prompt = match required_string(&arguments, "prompt") {
                    Ok(value) => value,
                    Err(message) => return error(id, -32602, &message),
                };
                let plan = AgentEngine::default().plan_task(prompt);
                success(id, json!({"plan": plan}))
            }
            "autopilot-status" => match block_on(AutopilotService::default().status()) {
                Ok(state) => success(id, json!({"state": state})),
                Err(err) => error(id, -32000, &err.to_string()),
            },
            "autopilot-schedule-list" => {
                match block_on(AutopilotService::default().list_schedules()) {
                    Ok(schedules) => success(id, json!({"schedules": schedules.schedules})),
                    Err(err) => error(id, -32000, &err.to_string()),
                }
            }
            "autopilot-recent-runs" => {
                let limit = match optional_limit(&arguments, "limit", 20) {
                    Ok(value) => value,
                    Err(message) => return error(id, -32602, &message),
                };
                match block_on(AutopilotService::default().status()) {
                    Ok(state) => {
                        let runs: Vec<_> = state
                            .recent_runs
                            .iter()
                            .rev()
                            .take(limit)
                            .cloned()
                            .collect();
                        success(id, json!({"recent_runs": runs}))
                    }
                    Err(err) => error(id, -32000, &err.to_string()),
                }
            }
            "safe-command" => {
                let command = match required_string(&arguments, "command") {
                    Ok(value) => value,
                    Err(message) => return error(id, -32602, &message),
                };
                match block_on(AgentEngine::default().run_safe_command(command)) {
                    Ok(execution) => success(id, json!({"execution": execution})),
                    Err(err) => error(id, -32000, &err.to_string()),
                }
            }
            _ => error(id, -32602, "Unknown tool"),
        }
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(future))
    } else {
        tokio::runtime::Runtime::new()
            .expect("tokio runtime")
            .block_on(future)
    }
}

fn tool_def(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}

fn required_string<'a>(arguments: &'a Value, key: &str) -> std::result::Result<&'a str, String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("arguments.{key} must be a string"))
}

fn optional_limit(
    arguments: &Value,
    key: &str,
    fallback: usize,
) -> std::result::Result<usize, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(fallback);
    };
    let parsed = value
        .as_u64()
        .ok_or_else(|| format!("arguments.{key} must be an integer"))? as usize;
    if parsed == 0 || parsed > 20 {
        return Err(format!("arguments.{key} must be between 1 and 20"));
    }
    Ok(parsed)
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
        let server = McpServer::default();
        let response = server
            .handle(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
            .unwrap();
        assert!(response.contains("safe-command"));
        assert!(response.contains("task-plan"));
    }

    #[test]
    fn validates_tool_arguments() {
        let server = McpServer::default();
        let response = server
            .handle(
                r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"safe-command","arguments":{}}}"#,
            )
            .unwrap();
        assert!(response.contains("arguments.command must be a string"));
    }

    #[test]
    fn builds_plan_from_prompt() {
        let server = McpServer::default();
        let response = server
            .handle(
                r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"task-plan","arguments":{"prompt":"check system health"}}}"#,
            )
            .unwrap();
        assert!(response.contains("system_health"));
    }
}
