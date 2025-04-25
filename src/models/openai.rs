use std::str::FromStr;

use reqwest::{
    Client, Error, Url,
    header::{HeaderMap, HeaderName, HeaderValue},
};

use crate::models::{
    AIModel, Body, Message, Role,
    auth::{Auth, AuthLocation},
};

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

impl AIModel for OpenAI {
    async fn call(&self, prompt: String) -> Result<String, Error> {
        let body = Body {
            model: self.model.clone(),
            messages: vec![Message {
                role: Role::User,
                content: prompt,
            }],
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
