use async_trait::async_trait;
use tokio::process::Command;

use crate::mcp::McpServer;

pub(crate) struct LocalMcp {
    pub(crate) command: Command,
}

#[async_trait]
impl McpServer for LocalMcp {
    async fn call(
        &self,
        tool: String,
        arguments: Vec<String>,
    ) -> Result<String, rmcp::ServiceError> {
        todo!()
    }
}
