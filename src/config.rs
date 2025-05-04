use rmcp::{ServiceExt, transport::TokioChildProcess};
use serde::Deserialize;
use std::{collections::HashMap, fs::File, io, sync::Arc};
use tokio::process::Command;

use crate::{
    ManagerConfig, Workspace,
    mcp::local::LocalMcp,
    models::{
        anthropic::Anthropic,
        auth::{Auth, AuthLocation},
        azure::Azure,
        gemini::Gemini,
        openai::OpenAI,
    },
};

const DEFAULT_PORT: u16 = 7000;
const DEFAULT_LISTENER: &str = "127.0.0.1";

#[derive(Debug, Deserialize)]
struct FileConfig {
    models: HashMap<String, Model>,
    mcps: Option<HashMap<String, Mcp>>,
    workspaces: HashMap<String, WorkspaceConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type")]
enum Model {
    Gemini {
        url: String,
        auth: AuthMethod,
    },
    OpenAI(BaseModel),
    Azure {
        url: String,
        auth: AuthMethod,
        #[serde(rename = "api-version")]
        api_version: String,
    },
    Anthropic {
        url: String,
        auth: AuthMethod,
        #[serde(rename = "anthropic-version")]
        anthropic_version: String,
        model: String,
    },
}

#[derive(Debug, Deserialize)]
struct BaseModel {
    url: String,
    auth: AuthMethod,
    model: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type", content = "config")]
enum AuthMethod {
    ApiKey(AuthConfig),
}

#[derive(Clone, Debug, Deserialize)]
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
    config: WorkspaceListener,
}

#[derive(Debug, Deserialize)]
struct WorkspaceListener {
    path: String,
    port: Option<u16>,
    address: Option<String>,
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

pub async fn get_config(file: &str) -> io::Result<ManagerConfig> {
    let file = File::open(file).expect("Couldn't open file");

    let file_config: FileConfig = serde_yaml::from_reader(file).expect("Invalid configuration");

    let mut config = ManagerConfig {
        ..Default::default()
    };

    for (name, model) in file_config.models {
        let auth = match model {
            Model::OpenAI(BaseModel { ref auth, .. })
            | Model::Gemini { ref auth, .. }
            | Model::Azure { ref auth, .. }
            | Model::Anthropic { ref auth, .. } => get_auth(auth.to_owned()),
        };

        config.models.insert(
            name,
            match model {
                Model::OpenAI(BaseModel { url, model, .. }) => {
                    Arc::new(OpenAI::new(url, auth, model))
                }
                Model::Gemini { url, .. } => Arc::new(Gemini::new(url, auth)),
                Model::Azure {
                    url, api_version, ..
                } => Arc::new(Azure::new(url, auth, api_version)),
                Model::Anthropic {
                    url,
                    anthropic_version,
                    model,
                    ..
                } => Arc::new(Anthropic::new(url, auth, model, anthropic_version)),
            },
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

                        LocalMcp {
                            command: ()
                                .serve(
                                    TokioChildProcess::new(&mut command)
                                        .expect("Couldn't start MCP server in tokio"),
                                )
                                .await
                                .expect("Couldn't start MCP server"),
                        }
                    }
                    _ => unimplemented!("MCP server not implemented"),
                }),
            );
        }
    }

    for (name, config_workspace) in file_config.workspaces {
        config.workspaces.insert(name.clone(), {
            let mut workspace = Workspace {
                name: name.clone(),
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

            let workspace = Arc::new(workspace);

            let port = if let Some(port) = config_workspace.config.port {
                port
            } else {
                DEFAULT_PORT
            };
            let path = if &config_workspace.config.path[0..1] != "/" {
                panic!(
                    "Invalid path '{}'. Paths start with '/'",
                    config_workspace.config.path
                )
            } else {
                config_workspace.config.path
            };

            let listener = if let Some(address) = config_workspace.config.address {
                format!("{address}:{port}")
            } else {
                format!("{DEFAULT_LISTENER}:{port}")
            };

            if config.listeners.contains_key(&listener) {
                config
                    .listeners
                    .get_mut(&listener)
                    .unwrap()
                    .insert(path, Arc::clone(&workspace));
            } else {
                config
                    .listeners
                    .insert(listener, HashMap::from([(path, Arc::clone(&workspace))]));
            }

            workspace
        });
    }

    Ok(config)
}

fn get_auth(auth: AuthMethod) -> Auth {
    match auth {
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
    }
}
