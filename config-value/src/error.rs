use std::collections::HashMap;
use thiserror::Error;

use crate::Value;

// Re-export compote's ConfigError for use in the mapping
pub use compote::Error as CompoteError;

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

    /// Maps this `ConfigErrorKind` to a `compote::Error`.
    ///
    /// This provides backward compatibility by allowing existing error handling
    /// code to internally use compote's error system while maintaining the
    /// existing public API.
    ///
    /// # Arguments
    ///
    /// * `context` - The context map containing error details (key, file, expected, actual, etc.)
    ///
    /// # Error Code Mapping
    ///
    /// | ConfigErrorKind | Compote Error | Code |
    /// |-----------------|---------------|------|
    /// | EmptyKey | MissingField | C001 |
    /// | MissingKey | MissingField | C001 |
    /// | InvalidValueType | TypeMismatch | C101 |
    /// | InvalidValue | InvalidValue | C102 |
    /// | InvalidRange | InvalidValue | C102 |
    /// | InvalidPackage | InvalidValue | C102 |
    /// | NotExactlyOneKeyInTable | InvalidValue | C102 |
    /// | UnsupportedValueInContext | InvalidValue | C102 |
    /// | ParsingError | ParseError | C120 |
    /// | OmniPathFileNotExecutable | FileNotExecutable | C130 |
    /// | OmniPathFileFailedToLoadMetadata | FileMetadataError | C131 |
    /// | OmniPathNotFound | PathNotFound | C132 |
    /// | MetadataHeader* variants | C140-C143 | varies |
    /// | UserDefined* variants | C150-C152 | varies |
    pub fn to_compote_error(&self, context: &HashMap<String, Value>) -> CompoteError {
        // Helper to get string from context
        let get_str = |key: &str| -> String {
            context
                .get(key)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        };

        // Helper to format a value for display
        let format_value = |v: &Value| -> String {
            if let Some(s) = v.as_str() {
                s.to_string()
            } else {
                format!("{:?}", v)
            }
        };

        // Get path from key or file context
        let path = get_str("key");
        let file = get_str("file");

        match self {
            // C001 - MissingField
            ConfigErrorKind::EmptyKey | ConfigErrorKind::MissingKey => {
                CompoteError::MissingField { path }
            }

            // C101 - TypeMismatch
            ConfigErrorKind::InvalidValueType => {
                let expected = context
                    .get("expected")
                    .map(|v| {
                        if let Some(s) = v.as_str() {
                            s.to_string()
                        } else if let Some(seq) = v.as_sequence() {
                            seq.iter()
                                .filter_map(|item| item.as_str())
                                .collect::<Vec<_>>()
                                .join(" or ")
                        } else {
                            format!("{:?}", v)
                        }
                    })
                    .unwrap_or_else(|| "unknown".to_string());

                let actual = context
                    .get("actual")
                    .map(format_value)
                    .unwrap_or_else(|| "unknown".to_string());

                CompoteError::TypeMismatch {
                    path,
                    expected,
                    actual,
                }
            }

            // C102 - InvalidValue
            ConfigErrorKind::InvalidValue => {
                let expected = context
                    .get("expected")
                    .map(|v| {
                        if let Some(s) = v.as_str() {
                            format!("'{}'", s)
                        } else if let Some(seq) = v.as_sequence() {
                            let values: Vec<String> = seq
                                .iter()
                                .filter_map(|item| item.as_str().map(|s| format!("'{}'", s)))
                                .collect();
                            format!("one of [{}]", values.join(", "))
                        } else {
                            format!("{:?}", v)
                        }
                    })
                    .unwrap_or_else(|| "valid value".to_string());

                let actual = context
                    .get("actual")
                    .map(format_value)
                    .unwrap_or_else(|| "unknown".to_string());

                CompoteError::InvalidValue {
                    path,
                    message: format!("expected {}, got {}", expected, actual),
                }
            }

            ConfigErrorKind::InvalidRange => {
                let min = context
                    .get("min")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let max = context
                    .get("max")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                CompoteError::InvalidValue {
                    path,
                    message: format!("invalid range [{}, {}[", min, max),
                }
            }

            ConfigErrorKind::InvalidPackage => {
                let package = get_str("package");
                CompoteError::InvalidValue {
                    path,
                    message: format!("invalid package '{}'", package),
                }
            }

            ConfigErrorKind::NotExactlyOneKeyInTable => {
                let actual = context
                    .get("actual")
                    .map(format_value)
                    .unwrap_or_else(|| "unknown".to_string());

                CompoteError::InvalidValue {
                    path,
                    message: format!(
                        "expected table with single key-value pair, got {}",
                        actual
                    ),
                }
            }

            ConfigErrorKind::UnsupportedValueInContext => {
                let actual = context
                    .get("actual")
                    .map(format_value)
                    .unwrap_or_else(|| "unknown".to_string());

                CompoteError::InvalidValue {
                    path,
                    message: format!("value {} not supported in this context", actual),
                }
            }

            // C120 - ParseError
            ConfigErrorKind::ParsingError => {
                let actual = context
                    .get("actual")
                    .map(format_value)
                    .unwrap_or_default();
                let error = get_str("error");

                CompoteError::Custom {
                    code: "C120".to_string(),
                    path: if file.is_empty() { path } else { file },
                    message: format!("unable to parse value '{}': {}", actual, error),
                }
            }

            // C130 - FileNotExecutable
            ConfigErrorKind::OmniPathFileNotExecutable => CompoteError::FileNotExecutable {
                path: if file.is_empty() { path } else { file },
            },

            // C131 - FileMetadataError
            ConfigErrorKind::OmniPathFileFailedToLoadMetadata => {
                CompoteError::FileMetadataError {
                    path: if file.is_empty() { path } else { file },
                    message: "failed to load metadata".to_string(),
                }
            }

            // C132 - PathNotFound
            ConfigErrorKind::OmniPathNotFound => CompoteError::PathNotFound {
                path: if file.is_empty() { path } else { file },
                message: "path not found".to_string(),
            },

            // C140 - MetadataHeaderParseError (generic metadata errors)
            ConfigErrorKind::MetadataHeaderMissingSubkey
            | ConfigErrorKind::MetadataHeaderContinueWithoutKey
            | ConfigErrorKind::MetadataHeaderUnknownKey
            | ConfigErrorKind::MetadataHeaderDuplicateKey
            | ConfigErrorKind::MetadataHeaderInvalidValueType => {
                let message = self
                    .message_from_context(context)
                    .unwrap_or_else(|_| "metadata header parse error".to_string());
                CompoteError::MetadataHeaderParseError {
                    path: if file.is_empty() { path } else { file },
                    message,
                }
            }

            // C141 - MetadataHeaderEmptyPart
            ConfigErrorKind::MetadataHeaderGroupEmptyPart
            | ConfigErrorKind::MetadataHeaderParameterEmptyPart => {
                let group = context
                    .get("group")
                    .or_else(|| context.get("parameter"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                CompoteError::MetadataHeaderEmptyPart {
                    path: if file.is_empty() { path } else { file },
                    group,
                }
            }

            // C142 - MetadataHeaderMissingField
            ConfigErrorKind::MetadataHeaderMissingHelp => {
                CompoteError::MetadataHeaderMissingField {
                    path: if file.is_empty() { path } else { file },
                    field: "help".to_string(),
                }
            }

            ConfigErrorKind::MetadataHeaderMissingSyntax => {
                CompoteError::MetadataHeaderMissingField {
                    path: if file.is_empty() { path } else { file },
                    field: "syntax".to_string(),
                }
            }

            ConfigErrorKind::MetadataHeaderGroupMissingParameters => {
                let group = get_str("group");
                CompoteError::MetadataHeaderMissingField {
                    path: if file.is_empty() { path } else { file },
                    field: format!("parameters for group '{}'", group),
                }
            }

            ConfigErrorKind::MetadataHeaderParameterMissingDescription => {
                let parameter = get_str("parameter");
                CompoteError::MetadataHeaderMissingField {
                    path: if file.is_empty() { path } else { file },
                    field: format!("description for parameter '{}'", parameter),
                }
            }

            // C143 - MetadataHeaderInvalidSyntax
            ConfigErrorKind::MetadataHeaderGroupUnknownConfigKey
            | ConfigErrorKind::MetadataHeaderParameterUnknownConfigKey
            | ConfigErrorKind::MetadataHeaderParameterInvalidKeyValue => {
                let message = self
                    .message_from_context(context)
                    .unwrap_or_else(|_| "invalid metadata header syntax".to_string());
                CompoteError::MetadataHeaderInvalidSyntax {
                    path: if file.is_empty() { path } else { file },
                    message,
                }
            }

            // C150-C152 - UserDefinedCommand errors
            ConfigErrorKind::UserDefinedPathCommandMissingTag
            | ConfigErrorKind::UserDefinedConfigCommandMissingTag => {
                let tag = get_str("tag");
                CompoteError::UserDefinedCommandMissingField {
                    path: if file.is_empty() { path } else { file },
                    field: tag,
                }
            }

            ConfigErrorKind::UserDefinedPathCommandInvalidTagValue
            | ConfigErrorKind::UserDefinedConfigCommandInvalidTagValue => {
                let tag = get_str("tag");
                let expected = get_str("expected");
                let actual = get_str("actual");
                CompoteError::UserDefinedCommandInvalidValue {
                    path: if file.is_empty() { path } else { file },
                    field: tag,
                    message: format!("expected value to {}, got '{}'", expected, actual),
                }
            }
        }
    }
}
