use axum::{body::Body, http::Response, response::IntoResponse};
use reqwest::Error as HttpError;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Error {
    pub(crate) status: u16,
    pub(crate) message: String,
}

impl From<HttpError> for Error {
    fn from(value: HttpError) -> Self {
        Error {
            status: if let Some(status) = value.status() {
                status.as_u16()
            } else {
                500
            },
            message: value.to_string(),
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response<Body> {
        Response::builder()
            .status(self.status)
            .body(Body::from(self.message))
            .unwrap()
    }
}
