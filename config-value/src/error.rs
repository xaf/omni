use std::collections::HashMap;
use thiserror::Error;

use crate::Value;

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

/// Configuration error kinds
#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum ConfigErrorKind {
    //  Cxxx for configuration errors
    //    C0xx for key errors
    #[error("C001")]
    MissingKey,
    #[error("C002")]
    EmptyKey,
    #[error("C003")]
    NotExactlyOneKeyInTable,

    //    C1xx for value errors
    #[error("C101")]
    InvalidValueType,
    #[error("C102")]
    InvalidValue,
    #[error("C103")]
    InvalidRange,
    #[error("C104")]
    InvalidPackage,
    #[error("C110")]
    UnsupportedValueInContext,
    #[error("C120")]
    ParsingError,

    //  Mxxx for metadata errors
    //    M0xx for larger missing errors
    #[error("M001")]
    MetadataHeaderMissingHelp,
    #[error("M002")]
    MetadataHeaderMissingSyntax,

    //    M1xx for key or subkey errors
    #[error("M101")]
    MetadataHeaderUnknownKey,
    #[error("M102")]
    MetadataHeaderMissingSubkey,
    #[error("M103")]
    MetadataHeaderContinueWithoutKey,
    #[error("M104")]
    MetadataHeaderDuplicateKey,

    //    M2xx for value errors
    #[error("M201")]
    MetadataHeaderInvalidValueType,

    //    M3xx for group errors
    #[error("M301")]
    MetadataHeaderGroupMissingParameters,
    #[error("M308")]
    MetadataHeaderGroupEmptyPart,
    #[error("M309")]
    MetadataHeaderGroupUnknownConfigKey,

    //    M4xx for parameter errors
    #[error("M401")]
    MetadataHeaderParameterInvalidKeyValue,
    #[error("M402")]
    MetadataHeaderParameterMissingDescription,
    #[error("M408")]
    MetadataHeaderParameterEmptyPart,
    #[error("M409")]
    MetadataHeaderParameterUnknownConfigKey,

    //  Pxxx for path errors
    #[error("P001")]
    OmniPathNotFound,
    #[error("P002")]
    OmniPathFileNotExecutable,
    #[error("P003")]
    OmniPathFileFailedToLoadMetadata,

    //  Uxxx for user-defined errors
    //    U1xx for path command errors
    #[error("U101")]
    UserDefinedPathCommandMissingTag,
    #[error("U102")]
    UserDefinedPathCommandInvalidTagValue,

    //    U2xx for config command errors
    #[error("U201")]
    UserDefinedConfigCommandMissingTag,
    #[error("U202")]
    UserDefinedConfigCommandInvalidTagValue,
}

impl ConfigErrorKind {
    pub fn default_ignored(&self) -> bool {
        matches!(self, ConfigErrorKind::MetadataHeaderMissingSyntax)
    }

    pub fn message_from_context(
        &self,
        context: &HashMap<String, Value>,
    ) -> Result<String, String> {
        let message = match self {
            ConfigErrorKind::InvalidValueType => {
                let key = context
                    .get("key")
                    .ok_or("Missing 'key' key in context")?
                    .as_str()
                    .ok_or("Value for 'key' is not a string")?;

                let expected = match context
                    .get("expected")
                    .ok_or("Missing 'expected' key in context")?
                {
                    v if v.as_str().is_some() => vec![v.as_str().unwrap().to_string()],
                    v if v.as_sequence().is_some() => {
                        let mut values = Vec::new();
                        for value in v.as_sequence().unwrap() {
                            if let Some(s) = value.as_str() {
                                values.push(s.to_string());
                            }
                        }
                        values
                    }
                    _ => {
                        return Err("Value for 'expected' is not a string or a sequence".to_string())
                    }
                };

                let actual = context
                    .get("actual")
                    .ok_or("Missing 'actual' key in context")?;

                format!(
                    "value for key '{}' should be {} but found {:?}",
                    key,
                    if expected.len() == 1 {
                        format!("a '{}'", expected[0])
                    } else {
                        format!("any type of {expected:?}")
                    },
                    actual,
                )
            }
            ConfigErrorKind::InvalidValue => {
                let key = context
                    .get("key")
                    .ok_or("Missing 'key' key in context")?
                    .as_str()
                    .ok_or("Value for 'key' is not a string")?;

                let expected = match context
                    .get("expected")
                    .ok_or("Missing 'expected' key in context")?
                {
                    v if v.as_str().is_some() => vec![v.as_str().unwrap().to_string()],
                    v if v.as_sequence().is_some() => {
                        let mut values = Vec::new();
                        for value in v.as_sequence().unwrap() {
                            if let Some(s) = value.as_str() {
                                values.push(s.to_string());
                            }
                        }
                        values
                    }
                    _ => {
                        return Err("Value for 'expected' is not a sequence".to_string());
                    }
                };

                let actual = context
                    .get("actual")
                    .ok_or("Missing 'actual' key in context")?;

                format!(
                    "value for key '{}' should be {} but found {:?}",
                    key,
                    if expected.len() == 1 {
                        format!("'{}'", expected[0])
                    } else {
                        format!("one of {expected:?}")
                    },
                    actual,
                )
            }
            ConfigErrorKind::InvalidRange => {
                let key = context
                    .get("key")
                    .ok_or("Missing 'key' key in context")?
                    .as_str()
                    .ok_or("Value for 'key' is not a string")?;

                let min = context
                    .get("min")
                    .ok_or("Missing 'min' key in context")?
                    .as_u64()
                    .ok_or("Value for 'min' is not a number")?;

                let max = context
                    .get("max")
                    .ok_or("Missing 'max' key in context")?
                    .as_u64()
                    .ok_or("Value for 'max' is not a number")?;

                format!(
                    "value for key '{key}' should define a valid range, but found [{min}, {max}[ instead"
                )
            }
            ConfigErrorKind::InvalidPackage => {
                let key = context
                    .get("key")
                    .ok_or("Missing 'key' key in context")?
                    .as_str()
                    .ok_or("Value for 'key' is not a string")?;

                let package = context
                    .get("package")
                    .ok_or("Missing 'package' key in context")?
                    .as_str()
                    .ok_or("Value for 'package' is not a string")?;

                format!("value for key '{key}' should be a valid package, but found '{package}'")
            }
            ConfigErrorKind::MissingKey => {
                let key = context
                    .get("key")
                    .ok_or("Missing 'key' key in context")?
                    .as_str()
                    .ok_or("Value for 'key' is not a string")?;

                format!("key '{key}' is missing")
            }
            ConfigErrorKind::EmptyKey => {
                let key = context
                    .get("key")
                    .ok_or("Missing 'key' key in context")?
                    .as_str()
                    .ok_or("Value for 'key' is not a string")?;

                format!("value for key '{key}' is empty")
            }
            ConfigErrorKind::NotExactlyOneKeyInTable => {
                let key = context
                    .get("key")
                    .ok_or("Missing 'key' key in context")?
                    .as_str()
                    .ok_or("Value for 'key' is not a string")?;

                let actual = context
                    .get("actual")
                    .ok_or("Missing 'actual' key in context")?;

                format!(
                    "value for key '{key}' should be a table with a single key-value pair but found {actual:?}"
                )
            }
            ConfigErrorKind::UnsupportedValueInContext => {
                let key = context
                    .get("key")
                    .ok_or("Missing 'key' key in context")?
                    .as_str()
                    .ok_or("Value for 'key' is not a string")?;

                let actual = context
                    .get("actual")
                    .ok_or("Missing 'actual' key in context")?;

                format!("value {actual:?} for '{key}' is not supported in this context")
            }
            ConfigErrorKind::ParsingError => {
                let key = context
                    .get("key")
                    .ok_or("Missing 'key' key in context")?
                    .as_str()
                    .ok_or("Value for 'key' is not a string")?;

                let actual = context
                    .get("actual")
                    .ok_or("Missing 'actual' key in context")?;

                let error = context
                    .get("error")
                    .ok_or("Missing 'error' key in context")?
                    .as_str()
                    .ok_or("Value for 'error' is not a string")?;

                format!("unable to parse value {actual:?} for key '{key}': {error}")
            }
            ConfigErrorKind::MetadataHeaderMissingSubkey => {
                let key = context
                    .get("key")
                    .ok_or("Missing 'key' key in context")?
                    .as_str()
                    .ok_or("Value for 'key' is not a string")?;

                format!("missing subkey for key '{key}'")
            }
            ConfigErrorKind::MetadataHeaderContinueWithoutKey => {
                "found a 'continue' ('+') line, but there is no current key".to_string()
            }
            ConfigErrorKind::MetadataHeaderUnknownKey => {
                let key = context
                    .get("key")
                    .ok_or("Missing 'key' key in context")?
                    .as_str()
                    .ok_or("Value for 'key' is not a string")?;

                format!("unknown key '{key}'")
            }
            ConfigErrorKind::MetadataHeaderDuplicateKey => {
                let key = context
                    .get("key")
                    .ok_or("Missing 'key' key in context")?
                    .as_str()
                    .ok_or("Value for 'key' is not a string")?;

                let prev_lineno = context
                    .get("prev_lineno")
                    .ok_or("Missing 'prev_lineno' key in context")?
                    .as_u64()
                    .ok_or("Value for 'prev_lineno' is not a number")?;

                format!("key '{key}' previously defined at line {prev_lineno}")
            }
            ConfigErrorKind::MetadataHeaderMissingSyntax => {
                "missing syntax for the command".to_string()
            }
            ConfigErrorKind::MetadataHeaderMissingHelp => {
                "missing help for the command".to_string()
            }
            ConfigErrorKind::MetadataHeaderInvalidValueType => {
                let key = context
                    .get("key")
                    .ok_or("Missing 'key' key in context")?
                    .as_str()
                    .ok_or("Value for 'key' is not a string")?;

                let value = context
                    .get("value")
                    .ok_or("Missing 'value' key in context")?
                    .as_str()
                    .ok_or("Value for 'value' is not a string")?;

                let expected = context
                    .get("expected")
                    .ok_or("Missing 'expected' key in context")?
                    .as_str()
                    .ok_or("Value for 'expected' is not a string")?;

                format!("invalid value '{value}' for key '{key}', expected {expected}",)
            }
            ConfigErrorKind::MetadataHeaderGroupEmptyPart => {
                let group = context
                    .get("group")
                    .ok_or("Missing 'group' key in context")?
                    .as_str()
                    .ok_or("Value for 'group' is not a string")?;

                format!("empty part in the definition of group '{group}'")
            }
            ConfigErrorKind::MetadataHeaderGroupUnknownConfigKey => {
                let group = context
                    .get("group")
                    .ok_or("Missing 'group' key in context")?
                    .as_str()
                    .ok_or("Value for 'group' is not a string")?;

                let config_key = context
                    .get("config_key")
                    .ok_or("Missing 'config_key' key in context")?
                    .as_str()
                    .ok_or("Value for 'config_key' is not a string")?;

                format!(
                    "unknown configuration key '{config_key}' in the definition of group '{group}'",
                )
            }
            ConfigErrorKind::MetadataHeaderGroupMissingParameters => {
                let group = context
                    .get("group")
                    .ok_or("Missing 'group' key in context")?
                    .as_str()
                    .ok_or("Value for 'group' is not a string")?;

                format!("group '{group}' does not have any parameters")
            }
            ConfigErrorKind::MetadataHeaderParameterEmptyPart => {
                let parameter = context
                    .get("parameter")
                    .ok_or("Missing 'parameter' key in context")?
                    .as_str()
                    .ok_or("Value for 'parameter' is not a string")?;

                format!("empty part in the definition of parameter '{parameter}'")
            }
            ConfigErrorKind::MetadataHeaderParameterUnknownConfigKey => {
                let parameter = context
                    .get("parameter")
                    .ok_or("Missing 'parameter' key in context")?
                    .as_str()
                    .ok_or("Value for 'parameter' is not a string")?;

                let config_key = context
                    .get("config_key")
                    .ok_or("Missing 'config_key' key in context")?
                    .as_str()
                    .ok_or("Value for 'config_key' is not a string")?;

                format!(
                    "unknown configuration key '{config_key}' in the definition of parameter '{parameter}'",
                )
            }
            ConfigErrorKind::MetadataHeaderParameterInvalidKeyValue => {
                let parameter = context
                    .get("parameter")
                    .ok_or("Missing 'parameter' key in context")?
                    .as_str()
                    .ok_or("Value for 'parameter' is not a string")?;

                let key = context
                    .get("key")
                    .ok_or("Missing 'key' key in context")?
                    .as_str()
                    .ok_or("Value for 'key' is not a string")?;

                let value = context
                    .get("value")
                    .ok_or("Missing 'value' key in context")?
                    .as_str()
                    .ok_or("Value for 'value' is not a string")?;

                format!(
                    "invalid value '{value}' for key '{key}' in the definition of parameter {parameter}"
                )
            }
            ConfigErrorKind::MetadataHeaderParameterMissingDescription => {
                let parameter = context
                    .get("parameter")
                    .ok_or("Missing 'parameter' key in context")?
                    .as_str()
                    .ok_or("Value for 'parameter' is not a string")?;

                format!("missing description for parameter '{parameter}'")
            }
            ConfigErrorKind::OmniPathNotFound => "path not found".to_string(),
            ConfigErrorKind::OmniPathFileNotExecutable => "file is not executable".to_string(),
            ConfigErrorKind::OmniPathFileFailedToLoadMetadata => {
                "failed to load metadata for file".to_string()
            }
            ConfigErrorKind::UserDefinedPathCommandMissingTag
            | ConfigErrorKind::UserDefinedConfigCommandMissingTag => {
                let tag = context
                    .get("tag")
                    .ok_or("Missing 'tag' key in context")?
                    .as_str()
                    .ok_or("Value for 'tag' is not a string")?;

                let key = context
                    .get("key")
                    .and_then(|v| v.as_str())
                    .map(|s| format!(" for command '{s}'"))
                    .unwrap_or_default();

                format!("required tag '{tag}' is missing{key}",)
            }
            ConfigErrorKind::UserDefinedPathCommandInvalidTagValue
            | ConfigErrorKind::UserDefinedConfigCommandInvalidTagValue => {
                let tag = context
                    .get("tag")
                    .ok_or("Missing 'tag' key in context")?
                    .as_str()
                    .ok_or("Value for 'tag' is not a string")?;

                let expected = context
                    .get("expected")
                    .ok_or("Missing 'expected' key in context")?
                    .as_str()
                    .ok_or("Value for 'expected' is not a string")?;

                let actual = context
                    .get("actual")
                    .ok_or("Missing 'actual' key in context")?
                    .as_str()
                    .ok_or("Value for 'actual' is not a string")?;

                let key = context
                    .get("key")
                    .and_then(|v| v.as_str())
                    .map(|s| format!(" for command '{s}'"))
                    .unwrap_or_default();

                format!(
                    "invalid value '{actual}' for tag '{tag}', expected value to {expected}{key}",
                )
            }
        };

        Ok(message)
    }
}
