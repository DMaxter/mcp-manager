use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::Url;
use rmcp::model::Tool as RcmpTool;
use serde::Serialize;
use serde_json::{from_str, json};
use tracing::{Level, event, instrument};

use crate::{
    Error as ManagerError, ManagerBody, UsageTokens,
    auth::Auth,
    mcp::ToolCall as GeneralToolCall,
    models::{
        AIModel, Message as ManagerMessage, ModelDecision, Role, TextMessage,
        client::ModelClient,
        openai::{
            FinishReason, Function, Message, ResponseBody, Tool, ToolCall, ToolCallParams,
            ToolChoice, ToolType,
        },
    },
};

#[derive(Debug, Serialize)]
pub(crate) struct RequestBody {
    pub(crate) messages: Vec<Message>,
    pub(crate) temperature: Option<f64>,
    pub(crate) max_tokens: Option<isize>,
    pub(crate) top_p: Option<f64>,
    pub(crate) tools: Option<Vec<Tool>>,
    pub(crate) tool_choice: ToolChoice,
}

impl From<ManagerBody> for RequestBody {
    fn from(value: ManagerBody) -> Self {
        RequestBody {
            temperature: value.temperature,
            max_tokens: value.max_tokens,
            top_p: value.top_p,
            messages: value
                .messages
                .into_iter()
                .map(|message| match message {
                    ManagerMessage::TextMessage(message) => Message::Text(message),
                    ManagerMessage::ToolOutput {
                        call_id, output, ..
                    } => Message::ToolOutput {
                        role: Role::Tool,
                        tool_call_id: call_id,
                        content: output,
                    },
                    ManagerMessage::ToolCalls { role, tool_calls } => Message::ToolCalls {
                        role,
                        tool_calls: tool_calls
                            .into_iter()
                            .map(|call| ToolCall {
                                function: ToolCallParams {
                                    name: call.name,
                                    arguments: json!(call.arguments).to_string(),
                                },
                                r#type: ToolType::Function,
                                id: call.id,
                            })
                            .collect(),
                    },
                })
                .collect(),
            tool_choice: ToolChoice::Auto,
            tools: None,
        }
    }
}

pub struct Azure {
    url: Url,
    client: ModelClient,
}

impl Azure {
    pub async fn new(url: String, auth: Auth, api_version: String) -> Azure {
        let mut params = HashMap::new();

        params.insert(String::from("api-version"), api_version);

        let (client, url) = ModelClient::new(url, auth, None, Some(params)).await;

        Azure { client, url }
    }
}

#[async_trait]
impl AIModel for Azure {
    #[instrument(skip_all)]
    async fn call(
        &self,
        body: ManagerBody,
        tools: Vec<RcmpTool>,
    ) -> Result<(Vec<ModelDecision>, UsageTokens), ManagerError> {
        let mut body: RequestBody = body.into();

        body.tools = Some(
            tools
                .into_iter()
                .map(|tool: RcmpTool| {
                    let description = if let Some(description) = tool.description {
                        description.to_string()
                    } else {
                        event!(
                            Level::WARN,
                            "Tool \"{}\" doesn't have a description",
                            tool.name
                        );

                        String::new()
                    };

                    Tool {
                        r#type: ToolType::Function,
                        function: Function {
                            name: tool.name.to_string(),
                            description,
                            parameters: tool.input_schema,
                        },
                    }
                })
                .collect(),
        );

        let response: String = self.client.call(self.url.clone(), &body).await?;

        let mut response = from_str::<ResponseBody>(&response).unwrap_or_else(|error| {
            event!(Level::ERROR, "Couldn't deserialize response: {error}");

            panic!()
        });

        if response.choices.len() > 1 {
            event!(
                Level::WARN,
                "Model gave multiple choices, moving on with first one"
            )
        }

        let choice = response.choices.remove(0);

        Ok((
            vec![match choice.finish_reason {
                FinishReason::Stop => ModelDecision::TextMessage(match choice.message {
                    Message::Text(TextMessage { role: _, content }) => content,
                    _ => todo!("Unknown response needs to be handled: {response:#?}"),
                }),
                FinishReason::ToolCalls => ModelDecision::ToolCalls(match choice.message {
                    Message::ToolCalls {
                        role: _,
                        tool_calls,
                    } => tool_calls
                        .into_iter()
                        .map(|call| GeneralToolCall {
                            name: call.function.name,
                            id: call.id,
                            arguments: from_str(&call.function.arguments).unwrap(),
                        })
                        .collect(),
                    _ => todo!("Unknown response needs to be handled: {response:#?}"),
                }),
            }],
            response.usage,
        ))
    }
}
