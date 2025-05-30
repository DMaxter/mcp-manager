pub enum Auth {
    ApiKey(AuthLocation),
    OAuth2 {
        url: String,
        client_id: String,
        client_secret: String,
        scope: Option<String>,
    },
    NoAuth,
}

pub enum AuthLocation {
    Header(String, String),
    Params(String, String),
}
