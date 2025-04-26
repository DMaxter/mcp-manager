use async_trait::async_trait;
use rmcp::{ServiceError, model::Tool};

pub(crate) mod local;

#[async_trait]
pub(crate) trait McpServer: Sync {
    async fn call(&self, tool: String, arguments: Vec<String>) -> Result<String, ServiceError>;
    async fn list_tools(&self) -> Result<Vec<Tool>, ServiceError>;
}
