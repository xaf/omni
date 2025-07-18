use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::rc::Rc;

use serde::ser::SerializeMap;
use serde::Serialize;
use serde_yaml::Value as YamlValue;
use thiserror::Error;

use crate::internal::commands::utils::abs_or_rel_path;
use crate::internal::user_interface::colors::StringColor;

#[derive(Clone)]
pub enum ConfigErrorHandler {
    Active {
        context: HashMap<String, YamlValue>,
        errors: Rc<RefCell<Vec<ConfigError>>>,
    },
    Noop,
}

impl Default for ConfigErrorHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigErrorHandler {
    pub fn new() -> Self {
        Self::Active {
            context: HashMap::new(),
            errors: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn noop() -> Self {
        Self::Noop
    }

    #[inline(always)]
    pub fn with_context<V: Into<YamlValue>>(&self, key: &str, value: V) -> Self {
        match self {
            Self::Active { context, errors } => {
                let mut new_context = context.clone();
                new_context.insert(key.to_string(), value.into());
                Self::Active {
                    context: new_context,
                    errors: errors.clone(),
                }
            }
            Self::Noop => Self::Noop,
        }
    }

    #[inline(always)]
    pub fn with_expected<V: Into<YamlValue>>(&self, expected: V) -> Self {
        self.with_context("expected", expected.into())
    }

    #[inline(always)]
    pub fn with_actual<V: Into<YamlValue>>(&self, actual: V) -> Self {
        self.with_context("actual", actual.into())
    }

    #[inline(always)]
    pub fn with_file<P: AsRef<Path>>(&self, path: P) -> Self {
        self.with_context(
            "file",
            path.as_ref().to_str().unwrap_or_default().to_string(),
        )
    }

    #[inline(always)]
    pub fn with_lineno(&self, lineno: usize) -> Self {
        self.with_context("lineno", lineno as u64)
    }

    #[inline(always)]
    pub fn with_key<S: AsRef<str>>(&self, key: S) -> Self {
        match self {
            Self::Active { context, errors } => {
                // Update the key
                let key = key.as_ref();
                let new_key = match context.get("key") {
                    Some(YamlValue::String(cur)) => format!("{cur}.{key}"),
                    Some(_) | None => key.to_string(),
                };

                // Create a new context
                let mut new_context = context.clone();
                new_context.insert("key".to_string(), new_key.into());

                Self::Active {
                    context: new_context,
                    errors: errors.clone(),
                }
            }
            Self::Noop => Self::Noop,
        }
    }

    #[inline(always)]
    pub fn with_index(&self, index: usize) -> Self {
        match self {
            Self::Active { context, errors } => {
                // Update the key
                let new_key = format!(
                    "{}[{}]",
                    context
                        .get("key")
                        .unwrap_or(&YamlValue::Null)
                        .as_str()
                        .unwrap_or("."),
                    index
                );

                // Create a new context
                let mut new_context = context.clone();
                new_context.insert("key".to_string(), new_key.into());

                Self::Active {
                    context: new_context,
                    errors: errors.clone(),
                }
            }
            Self::Noop => Self::Noop,
        }
    }

    #[inline(always)]
    pub fn error(&self, kind: ConfigErrorKind) {
        if let Self::Active { context, errors } = self {
            match ConfigError::new_from_kind(kind, context.clone()) {
                Ok(error) => errors.borrow_mut().push(error),
                Err(e) => panic!("Unable to create error: {e}"),
            }
        }
    }

    #[inline(always)]
    pub fn errors(&self) -> Vec<ConfigError> {
        match self {
            Self::Active { errors, .. } => errors.borrow().clone(),
            Self::Noop => vec![],
        }
    }

    #[inline(always)]
    pub fn has_errors(&self) -> bool {
        match self {
            Self::Active { errors, .. } => !errors.borrow().is_empty(),
            Self::Noop => false,
        }
    }

    #[inline(always)]
    pub fn last_error(&self) -> Option<ConfigError> {
        match self {
            Self::Active { errors, .. } => errors.borrow().last().cloned(),
            Self::Noop => None,
        }
    }

    #[inline(always)]
    pub fn extend(&self, other: &Self) {
        match (self, other) {
            (Self::Noop, _) | (_, Self::Noop) => {}
            (
                Self::Active { errors, .. },
                Self::Active {
                    errors: other_errors,
                    ..
                },
            ) => {
                let mut errors = errors.borrow_mut();
                errors.extend_from_slice(&other_errors.borrow());
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ConfigError {
    file: String,
    lineno: usize,
    kind: ConfigErrorKind,
    context: HashMap<String, YamlValue>,
}

impl Serialize for ConfigError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut map = serializer.serialize_map(Some(4))?;
        map.serialize_entry("file", &abs_or_rel_path(self.file()))?;
        map.serialize_entry("lineno", &self.lineno())?;
        map.serialize_entry("errorcode", &self.errorcode())?;
        map.serialize_entry("message", &self.message())?;
        map.end()
    }
}

impl ConfigError {
    pub fn new_from_kind(
        kind: ConfigErrorKind,
        context: HashMap<String, YamlValue>,
    ) -> Result<Self, String> {
        let file = context
            .get("file")
            .ok_or("Missing 'file' key in context")?
            .as_str()
            .ok_or("Value for 'file' is not a string")?;

        let lineno = context
            .get("lineno")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(0);

        Ok(Self {
            file: file.to_string(),
            lineno,
            kind,
            context,
        })
    }

    pub fn file(&self) -> &str {
        &self.file
    }

    pub fn lineno(&self) -> usize {
        self.lineno
    }

    pub fn errorcode(&self) -> String {
        self.kind.to_string()
    }

    pub fn kind(&self) -> &ConfigErrorKind {
        &self.kind
    }

    pub fn message(&self) -> String {
        self.kind
            .message_from_context(&self.context)
            .unwrap_or("<error generating message from error context>".to_string())
    }

    pub fn default_ignored(&self) -> bool {
        self.kind.default_ignored()
    }

    pub fn printable(&self) -> String {
        format!(
            "{file}{colon}{lineno}{colon}{errorcode}{colon}{message}",
            colon = ":".light_black(),
            file = abs_or_rel_path(self.file()).light_blue(),
            lineno = self.lineno().light_green(),
            errorcode = self.errorcode().red(),
            message = self.message(),
        )
    }

    #[cfg(test)]
    pub fn context_str(&self, key: &str) -> String {
        self.context
            .get(key)
            .map(|v| v.as_str().unwrap_or_default().to_string())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub fn context_usize(&self, key: &str) -> usize {
        self.context
            .get(key)
            .map(|v| v.as_u64().unwrap_or(0) as usize)
            .unwrap_or(0)
    }
}

impl Ord for ConfigError {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.file
            .cmp(&other.file)
            .then(self.lineno.cmp(&other.lineno))
            .then(self.errorcode().cmp(&other.errorcode()))
            .then(self.message().cmp(&other.message()))
    }
}

impl PartialOrd for ConfigError {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.printable())
    }
}

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
        context: &HashMap<String, YamlValue>,
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
                    YamlValue::String(s) => vec![s.to_string()],
                    YamlValue::Sequence(seq) => {
                        let mut values = Vec::new();
                        for value in seq {
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
                    YamlValue::String(s) => vec![s.to_string()],
                    YamlValue::Sequence(seq) => {
                        let mut values = Vec::new();
                        for value in seq {
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
                    .unwrap_or(&YamlValue::Null)
                    .as_str()
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
                    .unwrap_or(&YamlValue::Null)
                    .as_str()
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

/// This is the error type for the `parse_args` function
#[derive(Debug)]
pub enum ParseArgsErrorKind {
    ParserBuildError(String),
    ArgumentParsingError(clap::Error),
    InvalidValue(String),
}

impl ParseArgsErrorKind {
    #[cfg(test)]
    pub fn simple(&self) -> String {
        match self {
            Self::ParserBuildError(e) => e.clone(),
            Self::ArgumentParsingError(e) => {
                // Return the first block until the first empty line
                let err_str = e
                    .to_string()
                    .split('\n')
                    .map(|line| line.trim())
                    .take_while(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                let err_str = err_str.trim_start_matches("error: ");
                err_str.to_string()
            }
            Self::InvalidValue(e) => e.clone(),
        }
    }
}

impl PartialEq for ParseArgsErrorKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::ParserBuildError(a), Self::ParserBuildError(b)) => a == b,
            (Self::ArgumentParsingError(a), Self::ArgumentParsingError(b)) => {
                a.to_string() == b.to_string()
            }
            (Self::InvalidValue(a), Self::InvalidValue(b)) => a == b,
            _ => false,
        }
    }
}

impl fmt::Display for ParseArgsErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::ParserBuildError(e) => write!(f, "{e}"),
            Self::ArgumentParsingError(e) => write!(f, "{e}"),
            Self::InvalidValue(e) => write!(f, "{e}"),
        }
    }
}
