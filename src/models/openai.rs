use std::sync::Arc;

use async_trait::async_trait;
use reqwest::{Error, Url};
use rmcp::model::{JsonObject, Tool as RmcpTool};
use serde::{Deserialize, Serialize};
use serde_json::{from_str, json};
use tracing::{Level, event};

use crate::{
    Error as ManagerError, ManagerBody,
    mcp::ToolCall as GeneralToolCall,
    models::{
        AIModel, Message as ManagerMessage, ModelDecision, Role, TextMessage, auth::Auth,
        client::ModelClient,
    },
};

#[derive(Debug, Default, Serialize)]
pub(crate) struct RequestBody {
    pub(crate) messages: Vec<Message>,
    pub(crate) temperature: Option<f64>,
    pub(crate) max_tokens: Option<isize>,
    pub(crate) top_p: Option<f64>,
    pub(crate) tools: Option<Vec<Tool>>,
    pub(crate) tool_choice: ToolChoice,
    pub(crate) model: String,
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
                    ManagerMessage::TextMessage(message) => Message::TextMessage(message),
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
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Tool {
    pub(crate) r#type: ToolType,
    pub(crate) function: Function,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ToolType {
    Function,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Function {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) parameters: Arc<JsonObject>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ToolChoice {
    #[default]
    Auto,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResponseBody {
    pub(crate) choices: Vec<Choice>,
    pub(crate) usage: UsageTokens,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct UsageTokens {
    pub(crate) completion_tokens: usize,
    pub(crate) prompt_tokens: usize,
    pub(crate) total_tokens: usize,
}

impl UsageTokens {
    pub(crate) fn add(&mut self, other: &UsageTokens) {
        self.completion_tokens += other.completion_tokens;
        self.prompt_tokens += other.prompt_tokens;
        self.total_tokens += other.total_tokens;
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Choice {
    pub(crate) finish_reason: FinishReason,
    pub(crate) message: Message,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FinishReason {
    ToolCalls,
    Stop,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum Message {
    TextMessage(TextMessage),
    ToolCalls {
        role: Role,
        tool_calls: Vec<ToolCall>,
    },
    ToolOutput {
        role: Role,
        tool_call_id: String,
        content: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ToolOutputResult {
    result: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ToolCall {
    pub(crate) function: ToolCallParams,
    pub(crate) r#type: ToolType,
    pub(crate) id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ToolCallParams {
    pub(crate) arguments: String,
    pub(crate) name: String,
}

pub struct OpenAI {
    url: Url,
    client: ModelClient,
    model: String,
}

impl OpenAI {
    pub async fn new(url: String, auth: Auth, model: String) -> OpenAI {
        let (client, url) = ModelClient::new(url, auth, None, None).await;

        OpenAI { client, url, model }
    }
}

#[async_trait]
impl AIModel for OpenAI {
    async fn call(
        &self,
        body: ManagerBody,
        tools: Vec<RmcpTool>,
    ) -> Result<(Vec<ModelDecision>, UsageTokens), ManagerError> {
        let mut body: RequestBody = body.into();

        body.model = self.model.clone();
        body.tools = Some(
            tools
                .into_iter()
                .map(|tool: RmcpTool| Tool {
                    r#type: ToolType::Function,
                    function: Function {
                        name: tool.name.into_owned(),
                        description: tool.description.into_owned(),
                        parameters: tool.input_schema,
                    },
                })
                .collect(),
        );

        let response = self.client.call(self.url.clone(), &body).await?;

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
                    Message::TextMessage(TextMessage { role: _, content }) => content,
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
