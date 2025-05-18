use async_trait::async_trait;
use rand::distr::{Alphanumeric, SampleString};
use reqwest::Url;
use rmcp::model::{JsonObject, Tool as RcmpTool};
use serde::{Deserialize, Serialize};
use serde_json::{Value, from_str};
use tracing::{Level, event, instrument};

use crate::{
    Error as ManagerError, ManagerBody, UsageTokens as ManagerUsage,
    auth::Auth,
    mcp::ToolCall as GeneralToolCall,
    models::{
        AIModel, Message as ManagerMessage, ModelDecision, Role as ManagerRole, TextMessage,
        client::ModelClient,
    },
};

const ID_LEN: usize = 24;

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct RequestBody {
    pub(crate) contents: Vec<Message>,
    pub(crate) tools: Option<Vec<Tool>>,
}

impl From<ManagerBody> for RequestBody {
    fn from(value: ManagerBody) -> Self {
        let mut contents = Vec::new();

        let mut last_output: Option<&mut Message> = None;

        for message in value.messages.into_iter() {
            match message {
                ManagerMessage::TextMessage(TextMessage { role, content }) => {
                    last_output = None;

                    contents.push(Message {
                        role: match role {
                            ManagerRole::Assistant => Role::Model,
                            ManagerRole::User => Role::User,
                            _ => unreachable!("Role not possible for text message"),
                        },
                        parts: vec![Part::Text { text: content }],
                    });
                }
                ManagerMessage::ToolCalls { role, tool_calls } => {
                    last_output = None;

                    contents.push(Message {
                        role: match role {
                            ManagerRole::Assistant => Role::Model,
                            _ => unreachable!("Role not possible for tool call"),
                        },
                        parts: tool_calls
                            .into_iter()
                            .map(|call| Part::FunctionCall {
                                function_call: FunctionCall {
                                    name: call.name,
                                    args: call.arguments,
                                },
                            })
                            .collect(),
                    });
                }
                ManagerMessage::ToolOutput {
                    call_id, output, ..
                } => {
                    if let Some(last) = last_output {
                        last.parts.push(Part::FunctionOutput {
                            function_response: FunctionResponse {
                                name: call_id.clone(),
                                response: FunctionContent {
                                    name: call_id,
                                    content: output,
                                },
                            },
                        })
                    } else {
                        contents.push(Message {
                            role: Role::Function,
                            parts: vec![Part::FunctionOutput {
                                function_response: FunctionResponse {
                                    name: call_id.clone(),
                                    response: FunctionContent {
                                        name: call_id,
                                        content: output,
                                    },
                                },
                            }],
                        });
                    }

                    last_output = contents.last_mut();
                }
            };
        }

        RequestBody {
            contents,
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub(crate) function_declarations: Vec<FunctionDeclaration>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct FunctionDeclaration {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) parameters: JsonObject,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Message {
    role: Role,
    parts: Vec<Part>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Model,
    Function,
    User,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum Part {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: FunctionCall,
    },
    FunctionOutput {
        function_response: FunctionResponse,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseBody {
    candidates: Vec<Candidate>,
    usage_metadata: UsageTokens,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    content: Message,
    finish_reason: FinishReason,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum FinishReason {
    Stop,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageTokens {
    prompt_token_count: usize,
    candidates_token_count: usize,
    total_token_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum Modality {
    Text,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct FunctionCall {
    name: String,
    args: Option<JsonObject>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct FunctionResponse {
    name: String,
    response: FunctionContent,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct FunctionContent {
    name: String,
    content: String,
}

pub struct Gemini {
    url: Url,
    client: ModelClient,
}

impl Gemini {
    pub async fn new(url: String, auth: Auth) -> Gemini {
        let (client, url) = ModelClient::new(url, auth, None, None).await;

        Gemini { client, url }
    }
}

#[async_trait]
impl AIModel for Gemini {
    #[instrument(skip_all)]
    async fn call(
        &self,
        body: ManagerBody,
        tools: Vec<RcmpTool>,
    ) -> Result<(Vec<ModelDecision>, ManagerUsage), ManagerError> {
        let mut body: RequestBody = body.into();

        body.tools = Some(vec![Tool {
            function_declarations: tools
                .into_iter()
                .map(|tool: RcmpTool| {
                    let mut schema = JsonObject::clone(&tool.input_schema);
                    remove_keys(&mut schema);

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

                    FunctionDeclaration {
                        name: tool.name.to_string(),
                        description,
                        parameters: schema,
                    }
                })
                .collect(),
        }]);

        let response: String = self.client.call(self.url.clone(), &body).await?;

        let mut response = from_str::<ResponseBody>(&response).unwrap_or_else(|error| {
            event!(Level::ERROR, "Couldn't deserialize response: {error}");

            panic!()
        });

        if response.candidates.len() > 1 {
            event!(
                Level::WARN,
                "Model gave multiple choices, moving on with first one"
            )
        }

        let choice = response.candidates.remove(0);

        let mut result = Vec::new();

        match choice.finish_reason {
            FinishReason::Stop => {
                let mut last_call: Option<&mut ModelDecision> = None;
                for part in choice.content.parts.into_iter() {
                    match part {
                        Part::Text { text } => {
                            last_call = None;
                            result.push(ModelDecision::TextMessage(text));
                        }
                        Part::FunctionCall { function_call } => {
                            let id = Alphanumeric.sample_string(&mut rand::rng(), ID_LEN);

                            if let Some(last) = last_call
                                && let ModelDecision::ToolCalls(calls) = last
                            {
                                calls.push(GeneralToolCall {
                                    id,
                                    name: function_call.name,
                                    arguments: function_call.args,
                                });
                            } else {
                                result.push(ModelDecision::ToolCalls(vec![GeneralToolCall {
                                    id,
                                    name: function_call.name,
                                    arguments: function_call.args,
                                }]));
                            }

                            last_call = result.last_mut();
                        }
                        _ => unreachable!("Part not supported"),
                    }
                }
            }
        }

        Ok((
            result,
            ManagerUsage {
                completion_tokens: response.usage_metadata.candidates_token_count,
                prompt_tokens: response.usage_metadata.prompt_token_count,
                total_tokens: response.usage_metadata.total_token_count,
            },
        ))
    }
}

fn remove_keys(map: &mut JsonObject) {
    map.remove("$schema");
    map.remove("additionalProperties");

    for value in map.values_mut() {
        if let Value::Object(map) = value {
            remove_keys(map)
        }
    }
}
