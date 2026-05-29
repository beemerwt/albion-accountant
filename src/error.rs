#[derive(Debug)]
pub struct DecodeError(pub String);

pub type Result<T> = std::result::Result<T, DecodeError>;

impl From<&str> for DecodeError {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl From<String> for DecodeError {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<std::io::Error> for DecodeError {
    fn from(value: std::io::Error) -> Self {
        Self(value.to_string())
    }
}

impl From<serde_json::Error> for DecodeError {
    fn from(value: serde_json::Error) -> Self {
        Self(value.to_string())
    }
}

impl From<albion_network_lib::DecodeError> for DecodeError {
    fn from(value: albion_network_lib::DecodeError) -> Self {
        Self(value.0)
    }
}

impl From<dotenvy::Error> for DecodeError {
    fn from(value: dotenvy::Error) -> Self {
        Self(value.to_string())
    }
}

impl From<rusqlite::Error> for DecodeError {
    fn from(value: rusqlite::Error) -> Self {
        Self(value.to_string())
    }
}
