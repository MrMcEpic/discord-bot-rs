use std::fmt;

#[derive(Debug)]
pub enum BotError {
    Serenity(serenity::Error),
    Sqlx(sqlx::Error),
    Reqwest(reqwest::Error),
    SerdeJson(serde_json::Error),
    Other(String),
}

impl fmt::Display for BotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serenity(e) => write!(f, "Discord error: {e}"),
            Self::Sqlx(e) => write!(f, "Database error: {e}"),
            Self::Reqwest(e) => write!(f, "HTTP error: {e}"),
            Self::SerdeJson(e) => write!(f, "JSON error: {e}"),
            Self::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for BotError {}

impl From<serenity::Error> for BotError {
    fn from(e: serenity::Error) -> Self {
        Self::Serenity(e)
    }
}

impl From<sqlx::Error> for BotError {
    fn from(e: sqlx::Error) -> Self {
        Self::Sqlx(e)
    }
}

impl From<reqwest::Error> for BotError {
    fn from(e: reqwest::Error) -> Self {
        Self::Reqwest(e)
    }
}

impl From<serde_json::Error> for BotError {
    fn from(e: serde_json::Error) -> Self {
        Self::SerdeJson(e)
    }
}

impl From<String> for BotError {
    fn from(s: String) -> Self {
        Self::Other(s)
    }
}

pub type BotResult<T> = Result<T, BotError>;
