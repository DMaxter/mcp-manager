use std::{collections::HashMap, fmt::Debug, str::FromStr};

use axum::http::{HeaderName, HeaderValue};
use chrono::{DateTime, TimeDelta, Utc};
use oauth2::{
    Client as OAuthClient, ClientId, ClientSecret, EmptyExtraTokenFields, EndpointNotSet,
    EndpointSet, RevocationErrorResponseType, Scope, StandardErrorResponse, StandardRevocableToken,
    StandardTokenIntrospectionResponse, StandardTokenResponse, TokenResponse, TokenUrl,
    basic::{BasicClient, BasicErrorResponseType, BasicTokenType},
};
use reqwest::{Client as HttpClient, Error as HttpError, Url, header::HeaderMap};
use serde::Serialize;
use tracing::{Level, event, instrument};

use crate::models::auth::{Auth, AuthLocation};

type Token = StandardTokenResponse<EmptyExtraTokenFields, BasicTokenType>;
type AuthClient = OAuthClient<
    StandardErrorResponse<BasicErrorResponseType>,
    Token,
    StandardTokenIntrospectionResponse<EmptyExtraTokenFields, BasicTokenType>,
    StandardRevocableToken,
    StandardErrorResponse<RevocationErrorResponseType>,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointSet,
>;

#[derive(Debug)]
pub(crate) enum ModelClient {
    ClientCredentials {
        http: HttpClient,
        auth_params: AuthClient,
        auth_client: HttpClient,
        scope: Option<Scope>,
        token_data: TokenData,
    },
    ApiKey(SimpleClient),
    NoAuth(SimpleClient),
}

#[derive(Debug)]
pub(crate) struct TokenData {
    token: String,
    expiration: DateTime<Utc>,
}

#[derive(Debug)]
pub(crate) struct SimpleClient {
    pub(crate) client: HttpClient,
}

impl ModelClient {
    pub async fn new(
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
                url: auth_url,
                client_id,
                client_secret,
                scope,
            } => {
                let auth_params = BasicClient::new(ClientId::new(client_id))
                    .set_client_secret(ClientSecret::new(client_secret))
                    .set_token_uri(
                        TokenUrl::new(auth_url.clone())
                            .expect(&format!("Invalid auth url \"{auth_url}\"")),
                    );

                let auth_client = HttpClient::new();

                let mut client = auth_params.exchange_client_credentials();

                let client_scope: Option<Scope>;

                if let Some(scope) = scope {
                    client_scope = Some(Scope::new(scope));

                    client = client.add_scope(client_scope.clone().unwrap());
                } else {
                    client_scope = None;
                }

                let token = client
                    .request_async(&auth_client)
                    .await
                    .expect("Couldn't get token");

                let (http_client, url) = create_http_client(url, headers, parameters);
                (
                    ModelClient::ClientCredentials {
                        http: http_client,
                        auth_params,
                        auth_client,
                        scope: client_scope,
                        token_data: TokenData {
                            token: token.access_token().secret().to_owned(),
                            expiration: Utc::now()
                                .checked_add_signed(TimeDelta::seconds(
                                    token
                                        .expires_in()
                                        .expect("Token without expiration date")
                                        .as_secs()
                                        .try_into()
                                        .unwrap(),
                                ))
                                .expect("Date out of range"),
                        },
                    },
                    url,
                )
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
            ModelClient::ClientCredentials {
                http,
                auth_params,
                auth_client,
                scope,
                token_data,
            } => {
                todo!()
            }
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
