use rmcp::{
    ServiceExt,
    model::{ClientCapabilities, ClientInfo},
    transport::{SseClientTransport, StreamableHttpClientTransport, TokioChildProcess},
};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io,
    sync::Arc,
};
use tokio::process::Command;
use tracing::{Level, event, instrument};

use crate::{
    ManagerConfig, Workspace,
    auth::{Auth, AuthLocation},
    mcp::{McpServer, ToolFilter},
    models::{anthropic::Anthropic, azure::Azure, gemini::Gemini, openai::OpenAI},
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
        auth: Option<AuthMethod>,
    },
    OpenAI(BaseModel),
    Azure {
        url: String,
        auth: Option<AuthMethod>,
        #[serde(rename = "api-version")]
        api_version: String,
    },
    Anthropic {
        url: String,
        auth: Option<AuthMethod>,
        #[serde(rename = "anthropic-version")]
        anthropic_version: String,
        model: String,
    },
}

#[derive(Debug, Deserialize)]
struct BaseModel {
    url: String,
    auth: Option<AuthMethod>,
    model: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type", content = "config")]
enum AuthMethod {
    ApiKey(AuthConfig),
    OAuth2 {
        url: String,
        client_id: String,
        client_secret: String,
        scope: Option<String>,
    },
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
        filter: Option<ToolFilterConfig>,
    },
    Remote {
        url: String,
        filter: Option<ToolFilterConfig>,
        auth: Option<AuthMethod>,
        sse: Option<bool>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum ToolFilterConfig {
    Include { include: HashSet<String> },
    Exclude { exclude: HashSet<String> },
}

#[instrument]
pub async fn get_config(file: &str) -> io::Result<ManagerConfig> {
    let file = File::open(file).expect("Couldn't open file");

    let file_config: FileConfig = serde_yaml::from_reader(file).expect("Invalid configuration");

    let mut config = ManagerConfig {
        ..Default::default()
    };

    for (name, model) in file_config.models {
        event!(Level::DEBUG, "Parsing model \"{name}\"");

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
                    Arc::new(OpenAI::new(url, auth, model).await)
                }
                Model::Gemini { url, .. } => Arc::new(Gemini::new(url, auth).await),
                Model::Azure {
                    url, api_version, ..
                } => Arc::new(Azure::new(url, auth, api_version).await),
                Model::Anthropic {
                    url,
                    anthropic_version,
                    model,
                    ..
                } => Arc::new(Anthropic::new(url, auth, model, anthropic_version).await),
            },
        );
    }

    let client_info = ClientInfo {
        protocol_version: Default::default(),
        capabilities: ClientCapabilities::default(),
        client_info: rmcp::model::Implementation {
            name: String::from("mcp-manager"),
            version: String::from("0.3.0"),
        },
    };

    if let Some(config_mcps) = file_config.mcps {
        for (name, mcp) in config_mcps {
            event!(Level::DEBUG, "Parsing MCP server \"{name}\"");

            config.mcps.insert(
                name,
                match mcp {
                    Mcp::Local {
                        command,
                        args,
                        env,
                        filter,
                    } => {
                        let mut command = Command::new(command);

                        if let Some(args) = args {
                            command.args(args);
                        }

                        if let Some(env) = env {
                            command.envs(env);
                        }

                        Arc::new(McpServer {
                            service: client_info
                                .clone()
                                .serve(
                                    TokioChildProcess::new(command)
                                        .expect("Couldn't start MCP server in tokio"),
                                )
                                .await
                                .expect("Couldn't start MCP server"),
                            filter: get_filter(filter),
                        })
                    }
                    Mcp::Remote {
                        url,
                        filter,
                        auth,
                        sse,
                    } => {
                        let _auth = get_auth(auth);

                        let client = if let Some(sse) = sse
                            && sse
                        {
                            client_info
                                .clone()
                                .serve(SseClientTransport::start(url).await.unwrap_or_else(
                                    |error| panic!("Couldn't connect to server: {error}"),
                                ))
                                .await
                                .unwrap_or_else(|error| {
                                    panic!("Error with MCP connection: {error}")
                                })
                        } else {
                            client_info
                                .clone()
                                .serve(StreamableHttpClientTransport::from_uri(url))
                                .await
                                .unwrap_or_else(|error| {
                                    panic!("Error with MCP connection: {error}")
                                })
                        };

                        Arc::new(McpServer {
                            filter: get_filter(filter),
                            service: client,
                        })
                    }
                },
            );
        }
    }

    for (name, config_workspace) in file_config.workspaces {
        event!(Level::DEBUG, "Parsing workspace \"{name}\"");

        config.workspaces.insert(name.clone(), {
            let mut workspace = Workspace {
                name: name.clone(),
                model: Arc::clone(
                    if let Some(model) = config.models.get(&config_workspace.model) {
                        model
                    } else {
                        panic!("Undefined model \"{}\"", config_workspace.model)
                    },
                ),
                mcps: Vec::new(),
            };

            if let Some(workspace_mcps) = config_workspace.mcps {
                for mcp in workspace_mcps {
                    if let Some(mcp) = config.mcps.get(&mcp) {
                        workspace.mcps.push(Arc::clone(mcp))
                    } else {
                        panic!("Undefined MCP server \"{mcp}\"")
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

            config.listeners.entry(listener.clone()).or_default();
            config
                .listeners
                .get_mut(&listener)
                .unwrap()
                .insert(path, Arc::clone(&workspace));

            workspace
        });
    }

    Ok(config)
}

fn get_auth(auth: Option<AuthMethod>) -> Auth {
    if let Some(auth) = auth {
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
            AuthMethod::OAuth2 {
                url,
                client_id,
                client_secret,
                scope,
            } => Auth::OAuth2 {
                url,
                client_id,
                client_secret,
                scope,
            },
        }
    } else {
        Auth::None
    }
}

fn get_filter(filter: Option<ToolFilterConfig>) -> ToolFilter {
    if let Some(filter) = filter {
        match filter {
            ToolFilterConfig::Include { include } => ToolFilter::Include(include),
            ToolFilterConfig::Exclude { exclude } => ToolFilter::Exclude(exclude),
        }
    } else {
        ToolFilter::Exclude(HashSet::new())
    }
}
