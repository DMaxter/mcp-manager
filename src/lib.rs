use std::{collections::HashMap, sync::Arc};

use mcp::McpServer;

use crate::models::AIModel;

pub mod config;
pub mod mcp;
pub mod models;

#[derive(Default)]
pub struct ManagerConfig {
    pub workspaces: HashMap<String, Workspace>,
    models: HashMap<String, Arc<dyn AIModel>>,
    mcps: HashMap<String, Arc<dyn McpServer>>,
}

pub struct Workspace {
    pub model: Arc<dyn AIModel>,
    mcps: Vec<Arc<dyn McpServer>>,
}
