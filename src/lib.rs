#![feature(let_chains)]

use std::{collections::HashMap, sync::Arc};

use axum::{Extension, Json, extract::Path, response::IntoResponse};
use futures::future::try_join_all;
use mcp::McpServer;
use models::{
    Message, ModelDecision, Role, TextMessage, ToolOutputType, openai::Tool as OpenAITool,
};
use rmcp::model::Tool;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{Level, event, instrument};

use crate::models::AIModel;

pub mod config;
pub(crate) mod error;
pub mod mcp;
pub mod models;

pub use error::Error;

type HandlerConfig = Arc<RwLock<HashMap<String, Arc<Workspace>>>>;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ManagerBody {
    pub(crate) messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_tokens: Option<isize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) top_p: Option<f64>,
}

impl ManagerBody {
    pub fn append_message(&mut self, message: Message) {
        self.messages.push(message);
    }
}

#[derive(Default)]
pub struct ManagerConfig {
    pub listeners: HashMap<String, HashMap<String, Arc<Workspace>>>,
    pub workspaces: HashMap<String, Arc<Workspace>>,
    models: HashMap<String, Arc<dyn AIModel + Send>>,
    mcps: HashMap<String, Arc<dyn McpServer + Send>>,
}

pub struct Workspace {
    name: String,
    pub model: Arc<dyn AIModel + Send>,
    mcps: Vec<Arc<dyn McpServer + Send>>,
}

#[instrument(skip(config, body))]
pub async fn workspace_handler(
    Extension(config): Extension<HandlerConfig>,
    Path(mut path): Path<String>,
    Json(mut body): Json<ManagerBody>,
) -> Result<impl IntoResponse, Error> {
    path.insert(0, '/');

    if let Some(workspace) = config.read().await.get(&path) {
        event!(Level::INFO, "Listing tools in {}", workspace.name);

        let tools_fut: Vec<_> = workspace.mcps.iter().map(|mcp| mcp.list_tools()).collect();

        let tools = try_join_all(tools_fut)
            .await
            .expect("Couldn't get all tools");

        let mcp_calls = workspace
            .mcps
            .iter()
            .zip(tools.iter())
            .flat_map(|(mcp, tools)| {
                tools
                    .iter()
                    .map(|tool| (tool.name.clone().into_owned(), Arc::clone(&mcp)))
                    .collect::<Vec<(String, Arc<dyn McpServer + Send>)>>()
            })
            .collect::<HashMap<String, Arc<dyn McpServer + Send>>>();

        let tools: Vec<Tool> = tools.into_iter().flatten().collect();

        loop {
            let response = workspace
                .model
                .call(body.clone(), tools.clone())
                .await
                .unwrap();

            let mut tool_call = false;

            for decision in response.into_iter() {
                match decision {
                    ModelDecision::ToolCalls(calls) => {
                        tool_call = true;

                        body.append_message(Message::ToolCalls {
                            role: Role::Assistant,
                            tool_calls: calls.clone(),
                        });

                        for call in calls {
                            let call_id = call.id.clone();

                            let mcp_server = mcp_calls
                                .get(&call.name)
                                .ok_or(String::from("Function doesn't exist"));

                            let response = if let Ok(mcp_server) = mcp_server {
                                mcp_server.call(call).await.map_err(|_| Error {
                                    status: 500,
                                    message: String::from("Internal server error"),
                                })?
                            } else {
                                mcp_server.err().unwrap()
                            };

                            body.append_message(Message::ToolOutput {
                                r#type: ToolOutputType::FunctionCallOutput,
                                output: response,
                                call_id,
                            });
                        }
                    }
                    ModelDecision::TextMessage(message) => {
                        body.append_message(Message::TextMessage(TextMessage {
                            role: Role::Assistant,
                            content: message,
                        }))
                    }
                };
            }

            // If LLM doesn't want to call anything, just return all the messages
            if !tool_call {
                break;
            }
        }

        Ok(Json(body))
    } else {
        Err(error_path().await)
    }
}

#[instrument]
pub async fn error_method() -> Result<(), Error> {
    Err(Error {
        status: 406,
        message: String::from("Method not allowed"),
    })
}

#[instrument]
pub async fn error_path() -> Error {
    Error {
        status: 404,
        message: String::from("Path not found"),
    }
}
