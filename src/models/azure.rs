use std::{str::FromStr, sync::Arc};

use async_trait::async_trait;
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Error, Url};
use rmcp::model::{JsonObject, Tool as RcmpTool};
use serde::{Deserialize, Serialize};
use serde_json::{from_str, json};
use tracing::{Level, event, instrument};

use crate::{
    ManagerBody,
    mcp::ToolCall as GeneralToolCall,
    models::{
        AIModel, Message as ManagerMessage, ModelDecision, Role, TextMessage,
        auth::{Auth, AuthLocation},
        openai::ToolType,
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
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Tool {
    pub(crate) r#type: ToolType,
    pub(crate) function: Function,
}

#[derive(Debug, Deserialize, Serialize)]
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
    created: usize,
    model: String,
    object: String,
    usage: UsageTokens,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UsageTokens {
    completion_tokens: usize,
    prompt_tokens: usize,
    total_tokens: usize,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Choice {
    pub(crate) finish_reason: FinishReason,
    pub(crate) index: usize,
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
    function: ToolCallParams,
    r#type: ToolType,
    id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ToolCallParams {
    pub(crate) arguments: String,
    pub(crate) name: String,
}

pub struct Azure {
    url: Url,
    client: Client,
}

impl Azure {
    pub fn new(url: String, auth: Auth, api_version: String) -> Azure {
        let (client, url) = match auth {
            Auth::ApiKey(location) => match location {
                AuthLocation::Params(key, value) => (
                    Client::new(),
                    Url::parse_with_params(
                        &url,
                        &[(key, value), (String::from("api-version"), api_version)],
                    )
                    .expect("Invalid URL"),
                ),
                AuthLocation::Header(header, value) => {
                    let mut headers = HeaderMap::new();

                    headers.insert(
                        HeaderName::from_str(&header).unwrap(),
                        HeaderValue::from_str(&value).unwrap(),
                    );

                    (
                        Client::builder().default_headers(headers).build().unwrap(),
                        Url::parse_with_params(&url, &[("api-version", api_version)])
                            .expect("Invalid URL"),
                    )
                }
            },
            _ => panic!("Invalid authentication method for Azure! Supported: API Key in headers"),
        };

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
    ) -> Result<Vec<ModelDecision>, Error> {
        let mut body: RequestBody = body.into();

        body.tools = Some(
            tools
                .into_iter()
                .map(|tool: RcmpTool| Tool {
                    r#type: ToolType::Function,
                    function: Function {
                        name: tool.name.into_owned(),
                        description: tool.description.into_owned(),
                        parameters: tool.input_schema,
                    },
                })
                .collect(),
        );

        event!(Level::DEBUG, "Request: {body:#?}");

        let response: String = self
            .client
            .post(self.url.clone())
            .json(&body)
            .send()
            .await?
            .text()
            .await?;

        event!(Level::DEBUG, "Response: {response:?}");

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

        Ok(vec![match choice.finish_reason {
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
        }])
    }
}
