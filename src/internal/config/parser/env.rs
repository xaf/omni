use std::collections::HashMap;
use std::ops::Deref;
use std::path::PathBuf;

use itertools::Itertools;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils::Empty;
use crate::internal::commands::utils::abs_path_from_path;

use crate::internal::config::CompoteError;
use crate::internal::config::CompoteConfigValue;
use crate::internal::config::CompoteErrorTracker;
use crate::internal::config::CompoteFromConfigValue;
use crate::internal::config::CompoteConfigSource;

#[derive(Debug, Default, Deserialize, Clone)]
pub struct EnvConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<EnvOperationConfig>,
}

impl Deref for EnvConfig {
    type Target = Vec<EnvOperationConfig>;

    fn deref(&self) -> &Self::Target {
        &self.operations
    }
}

impl From<EnvConfig> for Vec<EnvOperationConfig> {
    fn from(env_config: EnvConfig) -> Self {
        env_config.operations
    }
}

impl Serialize for EnvConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if self.is_empty() {
            serializer.serialize_none()
        } else {
            self.operations.serialize(serializer)
        }
    }
}

impl Empty for EnvConfig {
    fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

impl compote::IsEmpty for EnvConfig {
    fn is_empty(&self) -> bool {
        Empty::is_empty(self)
    }
}

impl CompoteFromConfigValue for EnvConfig {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteError> {
        let operations = match value {
            CompoteConfigValue::Array(array, _) => {
                let mut ops = Vec::new();
                for (idx, item) in array.iter().enumerate() {
                    tracker.push_index(idx);
                    match EnvOperationConfig::parse_entry(item, tracker) {
                        Ok(parsed_ops) => ops.extend(parsed_ops),
                        Err(e) => tracker.record(e),
                    }
                    tracker.pop();
                }
                ops
            }
            CompoteConfigValue::Object(table, ctx) => {
                // If this is a map, create a list sorted by key for deterministic output
                let mut ops = Vec::new();
                for key in table.keys().sorted() {
                    let item_value = table.get(key).unwrap();
                    tracker.push_field(key);
                    // Create a single-key object for parsing
                    let mut single_entry = indexmap::IndexMap::new();
                    single_entry.insert(key.clone(), item_value.clone());
                    let entry_value = CompoteConfigValue::object(single_entry, ctx.clone());
                    match EnvOperationConfig::parse_entry(&entry_value, tracker) {
                        Ok(parsed_ops) => ops.extend(parsed_ops),
                        Err(e) => tracker.record(e),
                    }
                    tracker.pop();
                }
                ops
            }
            CompoteConfigValue::Null(_) => Vec::new(),
            _ => {
                return Err(CompoteError::TypeMismatch {
                    path: tracker.current_path(),
                    expected: "array or object".to_string(),
                    actual: value.type_name().to_string(),
                });
            }
        };

        Ok(Self { operations })
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct EnvOperationConfig {
    pub name: String,
    pub value: Option<String>,
    pub operation: EnvOperationEnum,
}

impl EnvOperationConfig {
    /// Parse a single entry from the env config.
    /// The entry should be a single-key object where the key is the env var name.
    fn parse_entry(
        config_value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Vec<Self>, CompoteError> {
        let table = match config_value {
            CompoteConfigValue::Object(obj, _) => obj,
            _ => {
                return Err(CompoteError::TypeMismatch {
                    path: tracker.current_path(),
                    expected: "object".to_string(),
                    actual: config_value.type_name().to_string(),
                });
            }
        };

        // There should be exactly one key/value pair
        if table.len() != 1 {
            return Err(CompoteError::InvalidValue {
                path: tracker.current_path(),
                message: format!("expected exactly one key in env entry, got {}", table.len()),
            });
        }

        let (name, value) = table.iter().next().unwrap();

        // Parse the value based on its structure
        Self::parse_value(name, value, config_value.context(), tracker)
    }

    /// Parse the value for an env var entry.
    fn parse_value(
        name: &str,
        value: &CompoteConfigValue,
        context: &compote::Context,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Vec<Self>, CompoteError> {
        match value {
            CompoteConfigValue::Object(table, _) => {
                // Check for operation keys
                if let Some(set_value) = table.get("set") {
                    tracker.push_field("set");
                    let result = Self::parse_operation_value(
                        name,
                        set_value,
                        EnvOperationEnum::Set,
                        context,
                        tracker,
                    );
                    tracker.pop();
                    return result.map(|op| op.into_iter().take(1).collect());
                }

                let mut operations = Vec::new();
                let mut matched_any = false;

                for (op_key, op_enum) in [
                    ("remove", EnvOperationEnum::Remove),
                    ("prepend", EnvOperationEnum::Prepend),
                    ("append", EnvOperationEnum::Append),
                    ("prefix", EnvOperationEnum::Prefix),
                    ("suffix", EnvOperationEnum::Suffix),
                ] {
                    if let Some(op_value) = table.get(op_key) {
                        matched_any = true;
                        tracker.push_field(op_key);
                        match Self::parse_operation_value(name, op_value, op_enum, context, tracker)
                        {
                            Ok(ops) => operations.extend(ops),
                            Err(e) => tracker.record(e),
                        }
                        tracker.pop();
                    }
                }

                if matched_any {
                    return Ok(operations);
                }

                // No operation keys found, treat as a "set" with value/type fields
                Self::parse_table_value(name, table, context, tracker)
            }
            // Simple scalar value means "set"
            _ => Self::parse_operation_value(name, value, EnvOperationEnum::Set, context, tracker),
        }
    }

    /// Parse operation value (can be scalar, array, or table with value/type)
    fn parse_operation_value(
        name: &str,
        value: &CompoteConfigValue,
        operation: EnvOperationEnum,
        context: &compote::Context,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Vec<Self>, CompoteError> {
        match value {
            CompoteConfigValue::Array(array, _) => {
                let mut operations = Vec::new();
                for (idx, item) in array.iter().enumerate() {
                    tracker.push_index(idx);
                    match Self::parse_single_operation(name, item, operation, context, tracker) {
                        Ok(op) => operations.push(op),
                        Err(e) => tracker.record(e),
                    }
                    tracker.pop();
                }
                Ok(operations)
            }
            CompoteConfigValue::Object(table, _) => {
                Self::parse_table_value(name, table, context, tracker).map(|ops| {
                    ops.into_iter()
                        .map(|mut op| {
                            op.operation = operation;
                            op
                        })
                        .collect()
                })
            }
            _ => {
                Self::parse_single_operation(name, value, operation, context, tracker)
                    .map(|op| vec![op])
            }
        }
    }

    /// Parse a table with value/type fields
    fn parse_table_value(
        name: &str,
        table: &indexmap::IndexMap<String, CompoteConfigValue>,
        context: &compote::Context,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Vec<Self>, CompoteError> {
        let value_type = if let Some(type_cv) = table.get("type") {
            match type_cv {
                CompoteConfigValue::String(s, _) if s == "text" || s == "path" => s.clone(),
                CompoteConfigValue::String(s, _) => {
                    return Err(CompoteError::InvalidValue {
                        path: tracker.current_path(),
                        message: format!("type must be 'text' or 'path', got '{}'", s),
                    });
                }
                _ => {
                    return Err(CompoteError::TypeMismatch {
                        path: tracker.current_path(),
                        expected: "string".to_string(),
                        actual: type_cv.type_name().to_string(),
                    });
                }
            }
        } else {
            "text".to_string()
        };

        let parsed_value = if let Some(value_cv) = table.get("value") {
            Self::extract_value(value_cv, &value_type, context, tracker)?
        } else {
            None
        };

        // If no value and operation is not Set, that's an error
        // (but we can't know the operation here, caller handles it)

        Ok(vec![Self {
            name: name.to_string(),
            value: parsed_value,
            operation: EnvOperationEnum::Set,
        }])
    }

    /// Parse a single operation from a scalar or table value
    fn parse_single_operation(
        name: &str,
        value: &CompoteConfigValue,
        operation: EnvOperationEnum,
        context: &compote::Context,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteError> {
        let (parsed_value, value_type) = match value {
            CompoteConfigValue::Object(table, _) => {
                let vtype = if let Some(type_cv) = table.get("type") {
                    match type_cv {
                        CompoteConfigValue::String(s, _) if s == "text" || s == "path" => s.clone(),
                        CompoteConfigValue::String(s, _) => {
                            return Err(CompoteError::InvalidValue {
                                path: tracker.current_path(),
                                message: format!("type must be 'text' or 'path', got '{}'", s),
                            });
                        }
                        _ => {
                            return Err(CompoteError::TypeMismatch {
                                path: tracker.current_path(),
                                expected: "string".to_string(),
                                actual: type_cv.type_name().to_string(),
                            });
                        }
                    }
                } else {
                    "text".to_string()
                };

                let val = if let Some(value_cv) = table.get("value") {
                    Self::extract_value(value_cv, &vtype, context, tracker)?
                } else {
                    None
                };

                (val, vtype)
            }
            _ => {
                let val = Self::extract_value(value, "text", context, tracker)?;
                (val, "text".to_string())
            }
        };

        // Validate: non-Set operations require a value
        if parsed_value.is_none() && operation != EnvOperationEnum::Set {
            return Err(CompoteError::InvalidValue {
                path: tracker.current_path(),
                message: "missing required 'value' field".to_string(),
            });
        }

        // Allow null value for "set" operation with "text" type to unset the variable
        if parsed_value.is_none() && operation == EnvOperationEnum::Set && value_type != "text" {
            return Err(CompoteError::InvalidValue {
                path: tracker.current_path(),
                message: "missing required 'value' field for path type".to_string(),
            });
        }

        Ok(Self {
            name: name.to_string(),
            value: parsed_value,
            operation,
        })
    }

    /// Extract a string value, handling path resolution if needed
    fn extract_value(
        value: &CompoteConfigValue,
        value_type: &str,
        context: &compote::Context,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Option<String>, CompoteError> {
        // Handle null for set operations
        if matches!(value, CompoteConfigValue::Null(_)) {
            return Ok(None);
        }

        // Try to coerce to string
        let string_value = match value {
            CompoteConfigValue::String(s, _) => s.clone(),
            CompoteConfigValue::Int(i, _) => i.to_string(),
            CompoteConfigValue::Float(f, _) => f.to_string(),
            CompoteConfigValue::Bool(b, _) => b.to_string(),
            _ => {
                return Err(CompoteError::TypeMismatch {
                    path: tracker.current_path(),
                    expected: "string".to_string(),
                    actual: value.type_name().to_string(),
                });
            }
        };

        // If path type, resolve relative to config file
        if value_type == "path" {
            let source_path = match &context.source {
                CompoteConfigSource::File(path) => Some(path.clone()),
                _ => None,
            };

            if let Some(source_path) = source_path {
                let parent_path = source_path
                    .parent()
                    .map(|p| p.to_string_lossy().to_string());
                let resolved = abs_path_from_path(
                    string_value.as_str(),
                    parent_path.as_deref(),
                );
                return Ok(Some(resolved.to_string_lossy().to_string()));
            }
        }

        Ok(Some(string_value))
    }
}

impl Serialize for EnvOperationConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.operation {
            EnvOperationEnum::Set => {
                let mut env_var = HashMap::new();
                env_var.insert(self.name.clone(), self.value.clone());
                env_var.serialize(serializer)
            }
            EnvOperationEnum::Prepend
            | EnvOperationEnum::Append
            | EnvOperationEnum::Remove
            | EnvOperationEnum::Prefix
            | EnvOperationEnum::Suffix => {
                let mut env_var_wrapped = HashMap::new();
                env_var_wrapped.insert(self.operation.to_string(), self.value.clone());

                let mut env_var = HashMap::new();
                env_var.insert(self.name.clone(), env_var_wrapped);
                env_var.serialize(serializer)
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Copy, Default, Hash)]
pub enum EnvOperationEnum {
    /// Set the environment variable to the specified value.
    /// If the value is `null`, then the variable is unset.
    /// This is the default operation.
    #[default]
    #[serde(rename = "s", alias = "set")]
    Set,
    /// Prepend the specified value to a list-style environment variable.
    /// If the variable is not set, it will be created with the specified value.
    #[serde(rename = "p", alias = "prepend")]
    Prepend,
    /// Append the specified value to a list-style environment variable.
    /// If the variable is not set, it will be created with the specified value.
    #[serde(rename = "a", alias = "append")]
    Append,
    /// Remove the specified value from a list-style environment variable.
    /// If the variable is not set, this operation has no effect.
    #[serde(rename = "r", alias = "remove")]
    Remove,
    /// Add the specified value as a prefix to the environment variable.
    /// If the variable is not set, it will be created with the specified value.
    #[serde(rename = "pf", alias = "prefix")]
    Prefix,
    /// Add the specified value as a suffix to the environment variable.
    /// If the variable is not set, it will be created with the specified value.
    #[serde(rename = "sf", alias = "suffix")]
    Suffix,
}

impl std::fmt::Display for EnvOperationEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", std::str::from_utf8(self.as_bytes()).unwrap())
    }
}

impl EnvOperationEnum {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            EnvOperationEnum::Set => b"set",
            EnvOperationEnum::Prepend => b"prepend",
            EnvOperationEnum::Append => b"append",
            EnvOperationEnum::Remove => b"remove",
            EnvOperationEnum::Prefix => b"prefix",
            EnvOperationEnum::Suffix => b"suffix",
        }
    }

    pub fn is_default(other: &EnvOperationEnum) -> bool {
        *other == EnvOperationEnum::default()
    }
}
