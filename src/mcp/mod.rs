use std::collections::HashSet;

use async_trait::async_trait;
use rmcp::{
    ServiceError,
    model::{JsonObject, Tool},
};
use serde::{Deserialize, Serialize};

pub(crate) mod local;

#[async_trait]
pub(crate) trait McpServer: Sync {
    async fn call(&self, call: ToolCall) -> Result<String, ServiceError>;
    async fn list_tools(&self) -> Result<Vec<Tool>, ServiceError>;
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
