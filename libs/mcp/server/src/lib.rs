pub struct McpServer;

impl McpServer {
    pub fn handle(&self, payload: &str) -> String {
        format!("mcp-server handled: {payload}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_payload() {
        let server = McpServer;
        assert!(server.handle("run").contains("run"));
    }
}
