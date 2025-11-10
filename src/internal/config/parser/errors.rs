use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::rc::Rc;

use serde::ser::SerializeMap;
use serde::Serialize;
use thiserror::Error;

use crate::internal::commands::utils::abs_or_rel_path;
use crate::internal::config::config_value::ConfigValue;
use crate::internal::user_interface::colors::StringColor;
use config_value::{ErrorHandler, Value};

// Re-export ConfigErrorKind so it can be accessed from this module
pub use config_value::ConfigErrorKind;

#[derive(Clone)]
pub enum ConfigErrorHandler {
    Active {
        context: HashMap<String, Value>,
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
    pub fn with_context<V: Into<Value>>(&self, key: &str, value: V) -> Self {
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
    pub fn with_expected<V: Into<Value>>(&self, expected: V) -> Self {
        self.with_context("expected", expected.into())
    }

    #[inline(always)]
    pub fn with_actual<V: Into<Value>>(&self, actual: V) -> Self {
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
                    Some(v) => {
                        if let Some(cur) = v.as_str() {
                            format!("{cur}.{key}")
                        } else {
                            key.to_string()
                        }
                    }
                    None => key.to_string(),
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
                let current_key = context
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                let new_key = format!("{}[{}]", current_key, index);

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

// Implement config_value::ErrorHandler trait for ConfigErrorHandler
impl ErrorHandler for ConfigErrorHandler {
    type ErrorKind = ConfigErrorKind;

    fn with_expected<V: Into<Value>>(self, expected: V) -> Self {
        self.with_context("expected", expected.into())
    }

    fn with_actual<S: config_value::Source, C: config_value::Scope>(
        self,
        actual: config_value::ConfigValue<S, C>,
    ) -> Self {
        let value: Value = actual.into();
        self.with_context("actual", value)
    }

    fn with_index(self, index: usize) -> Self {
        self.with_context("index", index as u64)
    }

    fn error(self, kind: Self::ErrorKind) {
        self.error(kind);
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ConfigError {
    file: String,
    lineno: usize,
    kind: ConfigErrorKind,
    context: HashMap<String, Value>,
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
        context: HashMap<String, Value>,
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
