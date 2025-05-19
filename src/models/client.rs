use std::{collections::HashMap, fmt::Debug, str::FromStr};

use axum::http::{HeaderName, HeaderValue};
use chrono::{DateTime, TimeDelta, Utc};
use oauth2::{
    Client as OAuthClient, ClientId, ClientSecret, EmptyExtraTokenFields, EndpointNotSet,
    EndpointSet, HttpClientError, RequestTokenError, RevocationErrorResponseType, Scope,
    StandardErrorResponse, StandardRevocableToken, StandardTokenIntrospectionResponse,
    StandardTokenResponse, TokenResponse, TokenUrl,
    basic::{BasicClient, BasicErrorResponseType, BasicTokenType},
};
use reqwest::{Client as HttpClient, Error as HttpError, Url, header::HeaderMap};
use serde::Serialize;
use tokio::sync::Mutex;
use tracing::{Level, event, instrument};

use crate::{
    Error as ManagerError,
    auth::{Auth, AuthLocation},
};

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
type AuthError =
    RequestTokenError<HttpClientError<HttpError>, StandardErrorResponse<BasicErrorResponseType>>;

#[derive(Debug)]
pub(crate) enum ModelClient {
    ClientCredentials {
        http: HttpClient,
        auth_params: Box<AuthClient>,
        auth_client: HttpClient,
        scope: Option<Scope>,
        token_data: Mutex<TokenData>,
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
                            .unwrap_or_else(|_| panic!("Invalid auth url \"{auth_url}\"")),
                    );

                let auth_client = HttpClient::new();

                let client_scope: Option<Scope>;

                if let Some(scope) = scope {
                    client_scope = Some(Scope::new(scope));
                } else {
                    client_scope = None;
                }

                let (token, expiration) =
                    get_client_credentials_token(&auth_params, client_scope.clone(), &auth_client)
                        .await
                        .expect("Couldn't get token");

                let (http_client, url) = create_http_client(url, headers, parameters);

                (
                    ModelClient::ClientCredentials {
                        http: http_client,
                        auth_params: Box::new(auth_params),
                        auth_client,
                        scope: client_scope,
                        token_data: Mutex::new(TokenData { token, expiration }),
                    },
                    url,
                )
            }
            Auth::None => {
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
    ) -> Result<String, ManagerError> {
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
                let token: String;

                {
                    let mut guard = token_data.lock().await;

                    if guard.expiration < Utc::now() {
                        match get_client_credentials_token(
                            auth_params,
                            scope.to_owned(),
                            auth_client,
                        )
                        .await
                        {
                            Ok(values) => {
                                (guard.token, guard.expiration) = values;
                            }
                            Err(error) => {
                                event!(Level::ERROR, "Couldn't get token: {error}");

                                return Err(ManagerError {
                                    status: 500,
                                    message: String::from("Couldn't renew token"),
                                });
                            }
                        };
                    }
                    token = guard.token.clone();
                }

                http.post(url)
                    .header("Authorization", format!("Bearer {token}"))
                    .json(&body)
                    .send()
                    .await?
                    .text()
                    .await?
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
        Url::parse_with_params(&url, params.iter())
            .unwrap_or_else(|_| panic!("Invalid URL \"{url}\" with parameters \"{params:?}\""))
    } else {
        Url::parse(&url).unwrap_or_else(|_| panic!("Invalid URL \"{url}\""))
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

async fn get_client_credentials_token(
    config: &AuthClient,
    scope: Option<Scope>,
    client: &HttpClient,
) -> Result<(String, DateTime<Utc>), AuthError> {
    let mut auth_client = config.exchange_client_credentials();

    if let Some(scope) = scope {
        auth_client = auth_client.add_scope(scope);
    }

    let token = auth_client.request_async(client).await?;

    Ok((
        token.access_token().secret().to_owned(),
        Utc::now()
            .checked_add_signed(TimeDelta::seconds(
                token
                    .expires_in()
                    .expect("Token without expiration date")
                    .as_secs()
                    .try_into()
                    .unwrap(),
            ))
            .expect("Date out of range"),
    ))
}
