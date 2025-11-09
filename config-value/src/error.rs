use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to parse configuration: {0}")]
    ParseError(String),

    #[error("Invalid value type: expected {expected}, got {actual}")]
    InvalidValueType { expected: String, actual: String },

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("YAML error: {0}")]
    YamlError(#[from] serde_yaml::Error),
}
