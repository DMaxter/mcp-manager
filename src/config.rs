use serde::Deserialize;
use std::{fs::File, io};

use crate::models::{
    AIModel,
    auth::{Auth, AuthLocation},
    openai::OpenAI,
};

#[derive(Debug, Deserialize)]
struct Config {
    models: Vec<Model>,
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

pub fn get_config(file: &str) -> io::Result<Vec<impl AIModel>> {
    let file = File::open(file).expect("Couldn't open file");

    let config: Config = serde_yaml::from_reader(file).expect("Invalid configuration");

    let mut models = Vec::new();

    for model in config.models {
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

        models.push(match model.r#type {
            ModelType::OpenAI => OpenAI::new(model.url, auth, model.model),
        });
    }

    Ok(models)
}
