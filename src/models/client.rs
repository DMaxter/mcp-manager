use std::{collections::HashMap, fmt::Debug, str::FromStr};

use axum::http::{HeaderName, HeaderValue};
use oauth2::basic::BasicClient as AuthClient;
use reqwest::{Client as HttpClient, Error as HttpError, Url, header::HeaderMap};
use serde::Serialize;
use tracing::{Level, event, instrument};

use crate::models::auth::{Auth, AuthLocation};

#[derive(Debug)]
pub(crate) enum ModelClient {
    ClientCredentials { http: HttpClient, auth: AuthClient },
    ApiKey(SimpleClient),
    NoAuth(SimpleClient),
}

#[derive(Debug)]
pub(crate) struct SimpleClient {
    pub(crate) client: HttpClient,
}

impl ModelClient {
    pub fn new(
        url: String,
        auth: Auth,
        headers: Option<HeaderMap>,
        parameters: Option<HashMap<String, String>>,
    ) -> (ModelClient, Url) {
        match auth {
            Auth::ApiKey(location) => match location {
                AuthLocation::Params(key, value) => {
                    let params = if let Some(mut params) = parameters {
                        params.insert(key, value);

                        params
                    } else {
                        let mut params = HashMap::new();
                        params.insert(key, value);

                        params
                    };

                    let (client, url) = create_http_client(url, headers, Some(params));

                    (ModelClient::ApiKey(SimpleClient { client }), url)
                }
                AuthLocation::Header(header, value) => {
                    let headers = if let Some(mut headers) = headers {
                        headers.insert(
                            HeaderName::from_str(&header).unwrap(),
                            HeaderValue::from_str(&value).unwrap(),
                        );

                        headers
                    } else {
                        let mut headers = HeaderMap::new();

                        headers.insert(
                            HeaderName::from_str(&header).unwrap(),
                            HeaderValue::from_str(&value).unwrap(),
                        );

                        headers
                    };

                    let (client, url) = create_http_client(url, Some(headers), parameters);

                    (ModelClient::ApiKey(SimpleClient { client }), url)
                }
            },
            Auth::OAuth2 {
                url,
                client_id,
                client_secret,
                scope,
            } => {
                todo!()
            }
            Auth::NoAuth => {
                let (client, url) = create_http_client(url, headers, parameters);

                (ModelClient::NoAuth(SimpleClient { client }), url)
            }
        }
    }

    #[instrument(skip_all)]
    pub async fn call<T: Debug + Serialize + ?Sized>(
        &self,
        url: Url,
        body: &T,
    ) -> Result<String, HttpError> {
        event!(Level::DEBUG, "Request: {body:#?}");

        let response: String = match self {
            ModelClient::ApiKey(http) | ModelClient::NoAuth(http) => {
                http.client
                    .post(url)
                    .json(&body)
                    .send()
                    .await?
                    .text()
                    .await?
            }
            _ => unimplemented!("HTTP Client not implemented"),
        };

        event!(Level::DEBUG, "Response: {response:?}");

        Ok(response)
    }
}

fn create_http_client(
    url: String,
    headers: Option<HeaderMap>,
    parameters: Option<HashMap<String, String>>,
) -> (HttpClient, Url) {
    let url = if let Some(params) = parameters {
        Url::parse_with_params(&url, params.iter()).expect(&format!(
            "Invalid URL \"{url}\" with parameters \"{params:?}\""
        ))
    } else {
        Url::parse(&url).expect(&format!("Invalid URL \"{url}\""))
    };

    let client = if let Some(headers) = headers {
        HttpClient::builder()
            .default_headers(headers)
            .build()
            .unwrap()
    } else {
        HttpClient::new()
    };

    (client, url)
}
