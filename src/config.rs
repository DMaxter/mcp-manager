use serde::Deserialize;
use std::{collections::HashMap, fs::File, io, sync::Arc};
use tokio::process::Command;

use crate::{
    ManagerConfig, McpServer, Workspace,
    mcp::local::LocalMcp,
    models::{
        auth::{Auth, AuthLocation},
        openai::OpenAI,
    },
};

#[derive(Debug, Deserialize)]
struct FileConfig {
    models: HashMap<String, Model>,
    mcps: Option<HashMap<String, Mcp>>,
    workspaces: HashMap<String, WorkspaceConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ModelType {
    OpenAI,
}

#[derive(Debug, Deserialize)]
struct Model {
    url: String,
    auth: AuthMethod,
    model: String,
    r#type: ModelType,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type", content = "config")]
enum AuthMethod {
    ApiKey(AuthConfig),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "location")]
enum AuthConfig {
    #[serde(rename = "header")]
    Header {
        name: String,
        value: String,
        prefix: Option<String>,
    },
    #[serde(rename = "parameter")]
    Parameter { name: String, value: String },
}

#[derive(Debug, Deserialize)]
struct WorkspaceConfig {
    model: String,
    mcps: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Mcp {
    Local {
        command: String,
        args: Option<Vec<String>>,
        env: Option<HashMap<String, String>>,
    },
    Remote {
        host: String,
        port: u16,
    },
}

pub fn get_config(file: &str) -> io::Result<ManagerConfig> {
    let file = File::open(file).expect("Couldn't open file");

    let file_config: FileConfig = serde_yaml::from_reader(file).expect("Invalid configuration");

    let mut config = ManagerConfig {
        ..Default::default()
    };

    for (name, model) in file_config.models {
        let auth = match model.auth {
            AuthMethod::ApiKey(location) => match location {
                AuthConfig::Parameter { name, value } => {
                    Auth::ApiKey(AuthLocation::Params(name, value))
                }
                AuthConfig::Header {
                    name,
                    value,
                    prefix,
                } => Auth::ApiKey(AuthLocation::Header(
                    name,
                    if let Some(prefix) = prefix {
                        format!("{prefix} {value}")
                    } else {
                        value
                    },
                )),
            },
        };

        config.models.insert(
            name,
            Arc::new(match model.r#type {
                ModelType::OpenAI => OpenAI::new(model.url, auth, model.model),
            }),
        );
    }

    if let Some(config_mcps) = file_config.mcps {
        for (name, mcp) in config_mcps {
            config.mcps.insert(
                name,
                Arc::new(match mcp {
                    Mcp::Local { command, args, env } => {
                        let mut command = Command::new(command);

                        if let Some(args) = args {
                            command.args(args);
                        }

                        if let Some(env) = env {
                            command.envs(env);
                        }

                        LocalMcp { command }
                    }
                    _ => unimplemented!("MCP server not implemented"),
                }),
            );
        }
    }

    for (name, config_workspace) in file_config.workspaces {
        config.workspaces.insert(name.clone(), {
            let mut workspace = Workspace {
                model: Arc::clone(
                    if let Some(model) = config.models.get(&config_workspace.model) {
                        model
                    } else {
                        panic!(
                            "Undefined model {} in workspace {name}",
                            config_workspace.model
                        )
                    },
                ),
                mcps: Vec::new(),
            };

            if let Some(workspace_mcps) = config_workspace.mcps {
                for mcp in workspace_mcps {
                    if let Some(mcp) = config.mcps.get(&mcp) {
                        workspace.mcps.push(Arc::clone(mcp))
                    } else {
                        panic!("Undefined MCP {mcp} in workspace {name}")
                    }
                }
            }

            workspace
        });
    }

    Ok(config)
}
