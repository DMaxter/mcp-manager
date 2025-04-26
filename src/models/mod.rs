use async_trait::async_trait;
use reqwest::Error;
use rmcp::model::Tool;

pub mod auth;
pub mod gemini;
pub mod openai;

#[async_trait]
pub trait AIModel: Sync {
    async fn call(&self, prompt: String, tools: Vec<Tool>) -> Result<String, Error>;
}
