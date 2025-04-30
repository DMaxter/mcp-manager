use async_trait::async_trait;
use reqwest::Error;
use rmcp::model::Tool;
use serde::{Deserialize, Serialize};

use crate::{ManagerBody, mcp::ToolCall};

pub mod auth;
pub mod azure;
pub mod gemini;
pub mod openai;

#[async_trait]
pub trait AIModel: Sync {
    async fn call(&self, body: ManagerBody, tools: Vec<Tool>) -> Result<Vec<ModelDecision>, Error>;
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Message {
    TextMessage(TextMessage),
    ToolCalls {
        role: Role,
        tool_calls: Vec<ToolCall>,
    },
    ToolOutput {
        r#type: ToolOutputType,
        call_id: String,
        output: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename = "snake_case")]
pub enum ToolOutputType {
    FunctionCallOutput,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextMessage {
    pub(crate) role: Role,
    pub(crate) content: String,
}

pub enum ModelDecision {
    TextMessage(String),
    ToolCalls(Vec<ToolCall>),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Assistant,
    System,
    Tool,
    User,
}
