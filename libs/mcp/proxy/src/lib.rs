use client::McpClient;
use server::McpServer;

pub struct McpProxy {
    client: McpClient,
    server: McpServer,
}

impl Default for McpProxy {
    fn default() -> Self {
        Self {
            client: McpClient,
            server: McpServer,
        }
    }
}

impl McpProxy {
    pub fn proxy(&self, payload: &str) -> String {
        let server_response = self.server.handle(payload);
        self.client.send_request(&server_response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxies_payload() {
        let proxy = McpProxy::default();
        let output = proxy.proxy("deploy");
        assert!(output.contains("deploy"));
    }
}
