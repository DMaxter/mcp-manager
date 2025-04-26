use std::{str::FromStr, sync::Arc};

use async_trait::async_trait;
use reqwest::{
    Client, Error, Url,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use rmcp::model::{JsonObject, Tool as RcmpTool};
use serde::{Deserialize, Serialize};

use crate::models::{
    AIModel,
    auth::{Auth, AuthLocation},
    openai::{Message, Role, ToolType},
};

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct OpenAIBody {
    pub(crate) model: String,
    pub(crate) messages: Vec<Message>,
    pub(crate) temperature: Option<f64>,
    pub(crate) max_tokens: Option<isize>,
    pub(crate) top_p: Option<f64>,
    pub(crate) tools: Option<Vec<Tool>>,
    pub(crate) tool_choice: ToolChoice,
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

pub struct Gemini {
    url: Url,
    client: Client,
    model: String,
}

impl Gemini {
    pub fn new(url: String, auth: Auth, model: String) -> Gemini {
        let (client, url) = match auth {
            Auth::ApiKey(location) => match location {
                AuthLocation::Params(key, value) => (
                    Client::new(),
                    Url::parse_with_params(&url, &[(key, value)]).expect("Invalid URL"),
                ),
                AuthLocation::Header(header, value) => {
                    let mut headers = HeaderMap::new();

                    headers.insert(
                        HeaderName::from_str(&header).unwrap(),
                        HeaderValue::from_str(&value).unwrap(),
                    );

                    (
                        Client::builder().default_headers(headers).build().unwrap(),
                        Url::parse(&url).expect("Invalid URL"),
                    )
                }
            },
            _ => panic!(
                "Invalid authentication method for OpenAI! Supported: API Key in Request Parameters"
            ),
        };

        Gemini { client, url, model }
    }
}

#[async_trait]
impl AIModel for Gemini {
    async fn call(&self, prompt: String, tools: Vec<RcmpTool>) -> Result<String, Error> {
        let body = OpenAIBody {
            model: self.model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: prompt,
            }],
            tools: Some(
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
            ),
            ..Default::default()
        };

        let response = self
            .client
            .post(self.url.clone())
            .json(&body)
            .send()
            .await?;

        Ok(response.text().await?)
    }
}
