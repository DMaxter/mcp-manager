pub enum Auth {
    ApiKey(AuthLocation),
    NoAuth,
}

pub enum AuthLocation {
    Header(String, String),
    Params(String, String),
}
