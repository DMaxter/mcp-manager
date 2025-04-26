use async_trait::async_trait;
use rmcp::{RoleClient, model::Tool, service::RunningService};
use tracing::instrument;

use crate::mcp::McpServer;

#[derive(Debug)]
pub(crate) struct LocalMcp {
    pub(crate) command: RunningService<RoleClient, ()>,
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

    #[instrument(skip(self))]
    async fn list_tools(&self) -> Result<Vec<Tool>, rmcp::ServiceError> {
        Ok(self.command.list_all_tools().await?)
    }
}
