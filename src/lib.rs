use std::{collections::HashMap, sync::Arc};

use axum::{
    Extension, Json,
    body::Body,
    extract::Path,
    response::{IntoResponse, Response},
};
use futures::future::try_join_all;
use mcp::McpServer;
use serde::Serialize;
use serde_json::json;
use tokio::sync::RwLock;
use tracing::instrument;

use crate::models::AIModel;

pub mod config;
pub mod mcp;
pub mod models;

type HandlerConfig = Arc<RwLock<HashMap<String, Arc<Workspace>>>>;

#[derive(Default)]
pub struct ManagerConfig {
    pub listeners: HashMap<String, HashMap<String, Arc<Workspace>>>,
    pub workspaces: HashMap<String, Arc<Workspace>>,
    models: HashMap<String, Arc<dyn AIModel + Send>>,
    mcps: HashMap<String, Arc<dyn McpServer + Send>>,
}

pub struct Workspace {
    pub model: Arc<dyn AIModel + Send>,
    mcps: Vec<Arc<dyn McpServer + Send>>,
}

#[derive(Debug, Serialize)]
pub struct Error {
    status: u16,
    message: String,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        Response::builder()
            .status(self.status)
            .body(Body::from(json!(self).to_string()))
            .unwrap()
    }
}

#[instrument(skip(config))]
pub async fn workspace_handler(
    Extension(config): Extension<HandlerConfig>,
    Path(mut path): Path<String>,
    Json(prompt): Json<String>,
) -> Result<impl IntoResponse, Error> {
    path.insert(0, '/');

    if let Some(workspace) = config.read().await.get(&path) {
        let tools_fut: Vec<_> = workspace.mcps.iter().map(|mcp| mcp.list_tools()).collect();

        let tools = try_join_all(tools_fut)
            .await
            .expect("Couldn't get all tools")
            .into_iter()
            .flatten()
            .collect();

        Ok(Json(workspace.model.call(prompt, tools).await.unwrap()))
    } else {
        Err(error_path().await)
    }
}

#[instrument]
pub async fn error_method() -> Result<(), Error> {
    Err(Error {
        status: 406,
        message: String::from("Method not allowed"),
    })
}

#[instrument]
pub async fn error_path() -> Error {
    Error {
        status: 404,
        message: String::from("Path not found"),
    }
}
