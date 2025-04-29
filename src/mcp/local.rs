use async_trait::async_trait;
use rmcp::{
    RoleClient, ServiceError,
    model::{CallToolRequestParam, RawContent, Tool},
    service::RunningService,
};
use tracing::{Level, event, instrument};

use crate::mcp::{McpServer, ToolCall};

#[derive(Debug)]
pub(crate) struct LocalMcp {
    pub(crate) command: RunningService<RoleClient, ()>,
}

#[async_trait]
impl McpServer for LocalMcp {
    #[instrument(skip(self))]
    async fn call(&self, call: ToolCall) -> Result<String, ServiceError> {
        let result = self
            .command
            .call_tool(CallToolRequestParam {
                name: call.name.into(),
                arguments: call.arguments,
            })
            .await?;

        if let Some(error) = result.is_error
            && error
        {
            event!(Level::ERROR, "{result:?}");
        } else {
            event!(Level::INFO, "{result:?}");
        }

        // FIXME: Handle multiple content responses
        if result.content.len() != 1 {
            event!(
                Level::ERROR,
                "Unknown MCP response needs to be handled: {:?}",
                result.content
            );
        }

        // FIXME: Handle annotations
        if result.content[0].annotations.is_some() {
            event!(Level::WARN, "Annotations not handled");
        }

        Ok(match result.content[0].clone().raw {
            RawContent::Text(text) => text.text,
            _ => unimplemented!(),
        })
    }

    #[instrument(skip(self))]
    async fn list_tools(&self) -> Result<Vec<Tool>, ServiceError> {
        Ok(self.command.list_all_tools().await?)
    }
}
