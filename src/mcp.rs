use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use rmcp::{
    RoleClient, ServiceError,
    model::{CallToolRequestParam, ClientInfo, JsonObject, RawContent, Tool},
    service::RunningService,
};
use tracing::{Level, event, instrument};

#[derive(Debug)]
pub(crate) struct McpServer {
    pub(crate) service: RunningService<RoleClient, ClientInfo>,
    pub(crate) filter: ToolFilter,
}

impl McpServer {
    #[instrument(skip(self))]
    pub(crate) async fn call(&self, call: ToolCall) -> Result<String, ServiceError> {
        let result = self
            .service
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
    pub(crate) async fn list_tools(&self) -> Result<Vec<Tool>, ServiceError> {
        Ok(self
            .service
            .list_all_tools()
            .await?
            .into_iter()
            .filter(|tool| {
                let name = tool.name.to_string();

                match &self.filter {
                    ToolFilter::Exclude(exclusions) => !exclusions.contains(&name),
                    ToolFilter::Include(inclusions) => inclusions.contains(&name),
                }
            })
            .collect())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolCall {
    pub(crate) name: String,
    pub(crate) id: String,
    pub(crate) arguments: Option<JsonObject>,
}

#[derive(Debug)]
pub(crate) enum ToolFilter {
    Include(HashSet<String>),
    Exclude(HashSet<String>),
}
