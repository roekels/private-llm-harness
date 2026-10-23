use serde::{Serialize, Serializer};

/// App error. Messages must never contain a secret value.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Msg(String),
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("Data error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("File error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn msg(s: impl Into<String>) -> Error {
    Error::Msg(s.into())
}

// Commands return errors to the webview as plain strings.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
