use std::{str::FromStr, sync::Arc};

use async_trait::async_trait;
use reqwest::{
    Client, Error, Url,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use rmcp::model::{JsonObject, Tool as RcmpTool};
use serde::{Deserialize, Serialize};
use tracing::{Level, event};

use crate::{
    ManagerBody,
    models::{
        AIModel, Message, ModelDecision,
        auth::{Auth, AuthLocation},
    },
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Body {
    pub(crate) model: String,
    pub(crate) input: Vec<Message>,
    pub(crate) temperature: Option<f64>,
    pub(crate) top_p: Option<f64>,
    pub(crate) tools: Option<Vec<Tool>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Tool {
    pub(crate) r#type: ToolType,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) parameters: Arc<JsonObject>,
    pub(crate) strict: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ToolType {
    Function,
}

pub struct OpenAI {
    url: Url,
    client: Client,
    model: String,
}

impl OpenAI {
    pub fn new(url: String, auth: Auth, model: String) -> OpenAI {
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

        OpenAI { client, url, model }
    }
}

impl From<ManagerBody> for Body {
    fn from(value: ManagerBody) -> Self {
        Body {
            temperature: value.temperature,
            top_p: value.top_p,
            input: value.messages,
            tools: None,
            ..Default::default()
        }
    }
}

#[async_trait]
impl AIModel for OpenAI {
    async fn call(&self, body: ManagerBody, tools: Vec<RcmpTool>) -> Result<ModelDecision, Error> {
        let mut body: Body = body.into();

        body.model = self.model.clone();
        body.tools = Some(
            tools
                .into_iter()
                .map(|tool: RcmpTool| Tool {
                    r#type: ToolType::Function,
                    name: tool.name.into_owned(),
                    description: tool.description.into_owned(),
                    parameters: tool.input_schema,
                    strict: false, // FIXME: allow this to be changed on the configuration
                })
                .collect(),
        );

        let response = self
            .client
            .post(self.url.clone())
            .json(&body)
            .send()
            .await?
            .text()
            .await?;

        event!(Level::DEBUG, "Response: {response:?}");

        Ok(ModelDecision::TextMessage(String::new()))
    }
}
