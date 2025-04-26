use async_trait::async_trait;
use rmcp::ServiceError;

pub(crate) mod local;

#[async_trait]
pub(crate) trait McpServer: Sync {
    async fn call(&self, tool: String, arguments: Vec<String>) -> Result<String, ServiceError>;
}
