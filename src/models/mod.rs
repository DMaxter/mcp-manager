use async_trait::async_trait;
use reqwest::Error;
use serde::Serialize;

pub mod auth;
pub mod openai;

#[async_trait]
pub trait AIModel: Sync {
    async fn call(&self, prompt: String) -> Result<String, Error>;
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Role {
    Assistant,
    System,
    User,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct Body {
    model: String,
    messages: Vec<Message>,
    temperature: Option<f64>,
    max_tokens: Option<isize>,
    top_p: Option<f64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct Message {
    role: Role,
    content: String,
}
