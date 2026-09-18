pub struct McpClient;

impl McpClient {
    pub fn send_request(&self, request: &str) -> String {
        format!("mcp-client forwarded: {request}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_forwarded_request() {
        let client = McpClient;
        assert!(client.send_request("ping").contains("ping"));
    }
}
