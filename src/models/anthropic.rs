use std::str::FromStr;

use async_trait::async_trait;
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Url;
use rmcp::model::Tool as RmcpTool;
use serde_json::from_str;
use tracing::{Level, event, instrument};

use crate::{
    Error as ManagerError, UsageTokens,
    models::{
        AIModel, ManagerBody, ModelDecision, TextMessage, ToolCall as GeneralToolCall,
        auth::Auth,
        client::ModelClient,
        openai::{FinishReason, Function, Message, RequestBody, ResponseBody, Tool, ToolType},
    },
};

pub struct Anthropic {
    url: Url,
    client: ModelClient,
    model: String,
}

impl Anthropic {
    pub async fn new(url: String, auth: Auth, model: String, version: String) -> Anthropic {
        let mut headers = HeaderMap::new();

        headers.insert(
            HeaderName::from_str("anthropic_version").unwrap(),
            HeaderValue::from_str(&version).unwrap(),
        );

        let (client, url) = ModelClient::new(url, auth, Some(headers), None).await;

        Anthropic { client, url, model }
    }
}

#[async_trait]
impl AIModel for Anthropic {
    #[instrument(skip_all)]
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
