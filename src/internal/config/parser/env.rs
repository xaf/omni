use std::collections::HashMap;
use std::ops::Deref;

use itertools::Itertools;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils::Empty;
use crate::internal::commands::utils::abs_path_from_path;

// ============================================================================
// EnvConfig - top-level wrapper
// ============================================================================

#[derive(Debug, Default, Clone)]
pub struct EnvConfig {
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

impl feuilletage::IsEmpty for EnvConfig {
    fn is_empty(&self) -> bool {
        Empty::is_empty(self)
    }
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel> feuilletage::FromContextValue<S, L>
    for EnvConfig
{
    fn from_context_value(
        value: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        if matches!(value, feuilletage::ContextValue::Null(_)) {
            return Ok(Self::default());
        }

        // Apply the transform to normalize the input
        let mut transformed = value.clone();
        env_entries_transform(&mut transformed)?;

        // Parse as Vec<EnvVarConfig>
        let entries: Vec<EnvVarConfig> =
            feuilletage::FromContextValue::from_context_value(&transformed, tracker)?;

        // Flatten to operations
        let operations = entries.iter().flat_map(|e| e.to_operations()).collect();

        Ok(Self { operations })
    }
}

// ============================================================================
// EnvOperationConfig - output struct (unchanged)
// ============================================================================

#[derive(Debug, Clone)]
pub struct EnvOperationConfig {
    pub name: String,
    pub value: Option<String>,
    pub operation: EnvOperationEnum,
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

// ============================================================================
// EnvOpValue - value+type pair with manual FromContextValue
// ============================================================================

#[derive(Debug, Clone, Default)]
struct EnvOpValue {
    value: Option<String>,
    value_type: String,
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel> feuilletage::FromContextValue<S, L>
    for EnvOpValue
{
    fn from_context_value(
        value: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        match value {
            feuilletage::ContextValue::Null(_) => Ok(EnvOpValue {
                value: None,
                value_type: "text".to_string(),
            }),
            feuilletage::ContextValue::Object(table, _) => {
                let parsed_value = if let Some(value_cv) = table.get("value") {
                    match value_cv {
                        feuilletage::ContextValue::Null(_) => None,
                        _ => Some(coerce_to_string(value_cv, tracker)?),
                    }
                } else {
                    None
                };
                let vtype = if let Some(type_cv) = table.get("type") {
                    match type_cv {
                        feuilletage::ContextValue::String(s, _) if s == "text" || s == "path" => {
                            s.clone()
                        }
                        feuilletage::ContextValue::String(s, _) => {
                            return Err(feuilletage::Error::InvalidValue {
                                path: tracker.current_path(),
                                message: format!("type must be 'text' or 'path', got '{}'", s),
                            });
                        }
                        _ => {
                            return Err(feuilletage::Error::TypeMismatch {
                                path: tracker.current_path(),
                                expected: "string".to_string(),
                                actual: type_cv.type_name().to_string(),
                            });
                        }
                    }
                } else {
                    "text".to_string()
                };
                Ok(EnvOpValue {
                    value: parsed_value,
                    value_type: vtype,
                })
            }
            // Scalar value -> coerce to string
            _ => {
                let s = coerce_to_string(value, tracker)?;
                Ok(EnvOpValue {
                    value: Some(s),
                    value_type: "text".to_string(),
                })
            }
        }
    }
}

// ============================================================================
// EnvVarConfig - feuilletage-derived struct for a single env var entry
// ============================================================================

#[derive(Debug, Clone, Default, feuilletage::Config)]
#[feuilletage(skip_serialize)]
struct EnvVarConfig {
    name: String,
    #[feuilletage(default, allow_single)]
    set: Vec<EnvOpValue>,
    #[feuilletage(default, allow_single)]
    prepend: Vec<EnvOpValue>,
    #[feuilletage(default, allow_single)]
    append: Vec<EnvOpValue>,
    #[feuilletage(default, allow_single)]
    remove: Vec<EnvOpValue>,
    #[feuilletage(default, allow_single)]
    prefix: Vec<EnvOpValue>,
    #[feuilletage(default, allow_single)]
    suffix: Vec<EnvOpValue>,
    // Fallback: when no operation key present, {value: X, type: Y} means implicit Set
    #[feuilletage(default)]
    value: Option<String>,
    #[feuilletage(default = "text", rename = "type")]
    value_type: String,
    // Source path for path resolution, extracted from context metadata
    #[feuilletage(from_context = "source.file_path")]
    source_path: Option<std::path::PathBuf>,
}

impl EnvVarConfig {
    fn to_operations(&self) -> Vec<EnvOperationConfig> {
        let mut ops = Vec::new();

        // `set` takes priority and only uses the first element (matching original behavior)
        if !self.set.is_empty() {
            if let Some(op) = self.set.first() {
                ops.push(self.make_op(op, EnvOperationEnum::Set));
            }
            return ops;
        }

        // Other operations can coexist (order matches original iteration)
        for op in &self.remove {
            ops.push(self.make_op(op, EnvOperationEnum::Remove));
        }
        for op in &self.prepend {
            ops.push(self.make_op(op, EnvOperationEnum::Prepend));
        }
        for op in &self.append {
            ops.push(self.make_op(op, EnvOperationEnum::Append));
        }
        for op in &self.prefix {
            ops.push(self.make_op(op, EnvOperationEnum::Prefix));
        }
        for op in &self.suffix {
            ops.push(self.make_op(op, EnvOperationEnum::Suffix));
        }

        if ops.is_empty() {
            // No operation keys → implicit Set (value/type fallback)
            let resolved = self.resolve_path(self.value.as_deref(), &self.value_type);
            ops.push(EnvOperationConfig {
                name: self.name.clone(),
                value: resolved,
                operation: EnvOperationEnum::Set,
            });
        }

        ops
    }

    fn make_op(&self, op: &EnvOpValue, operation: EnvOperationEnum) -> EnvOperationConfig {
        let resolved = self.resolve_path(op.value.as_deref(), &op.value_type);
        EnvOperationConfig {
            name: self.name.clone(),
            value: resolved,
            operation,
        }
    }

    fn resolve_path(&self, value: Option<&str>, value_type: &str) -> Option<String> {
        value.map(|v| {
            if value_type == "path" {
                if let Some(ref source_path) = self.source_path {
                    let parent = source_path.parent().map(|p| p.to_string_lossy().to_string());
                    abs_path_from_path(v, parent.as_deref())
                        .to_string_lossy()
                        .to_string()
                } else {
                    v.to_string()
                }
            } else {
                v.to_string()
            }
        })
    }
}

// ============================================================================
// Transform: normalizes input for Vec<EnvVarConfig> parsing
// ============================================================================

/// Operation keys recognized in env var config objects.
const OPERATION_KEYS: &[&str] = &["set", "prepend", "append", "remove", "prefix", "suffix"];

/// Pre-processes the ContextValue input to normalize env config entries.
///
/// - If input is Object: convert to sorted array of single-key objects
/// - For each array element that is a single-key object `{KEY: value}`:
///   - Extract key -> inject as `name` field
///   - If value is null: create `{name: KEY, set: [{value: null}]}`
///   - If value is scalar: create `{name: KEY, set: value}`
///   - If value is object with op keys: merge and wrap null op values
///   - If value is object without op keys: merge as value/type fields
fn env_entries_transform<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &mut feuilletage::ContextValue<S, L>,
) -> Result<(), feuilletage::Error> {
    // Step 1: If input is an Object, convert to sorted array of single-key objects
    if matches!(value, feuilletage::ContextValue::Object(_, _)) {
        // Clone the value to avoid borrow conflict (read from clone, write to original)
        let cloned = value.clone();
        let ctx = cloned.context().clone();
        if let feuilletage::ContextValue::Object(table, _) = &cloned {
            let mut items = Vec::new();
            for key in table.keys().sorted() {
                let item_value = table.get(key).unwrap().clone();
                let item_ctx = item_value.context().clone();
                let mut single_entry = indexmap::IndexMap::new();
                single_entry.insert(key.clone(), item_value);
                items.push(feuilletage::ContextValue::object(single_entry, item_ctx));
            }
            *value = feuilletage::ContextValue::array(items, ctx);
        }
    }

    // Step 2: Process each array element
    if let feuilletage::ContextValue::Array(items, _) = value {
        for item in items.iter_mut() {
            // Clone context and check structure before mutable borrow
            let item_ctx = item.context().clone();
            let item_clone = item.clone();
            if let feuilletage::ContextValue::Object(table, _) = &item_clone {
                if table.len() != 1 {
                    continue;
                }
                let (name, var_value) = table.iter().next().unwrap();
                let new_obj = normalize_env_entry(name, var_value, &item_ctx);
                *item = feuilletage::ContextValue::object(new_obj, item_ctx);
            }
        }
    }

    Ok(())
}

/// Normalize a single env entry `{KEY: value}` into a flat object with `name` field.
fn normalize_env_entry<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    name: &str,
    var_value: &feuilletage::ContextValue<S, L>,
    ctx: &feuilletage::Context<S, L>,
) -> indexmap::IndexMap<String, feuilletage::ContextValue<S, L>> {
    let mut obj = indexmap::IndexMap::new();
    obj.insert(
        "name".to_string(),
        feuilletage::ContextValue::string(name, ctx.clone()),
    );

    match var_value {
        feuilletage::ContextValue::Null(_) => {
            // null -> unset: wrap as {set: [{value: null}]} to prevent feuilletage
            // from treating null as "missing field" (which would use default)
            let null_wrapped = wrap_null_value(ctx);
            let set_array = feuilletage::ContextValue::array(vec![null_wrapped], ctx.clone());
            obj.insert("set".to_string(), set_array);
        }
        feuilletage::ContextValue::Object(inner_table, _) => {
            let has_op_keys = inner_table
                .keys()
                .any(|k| OPERATION_KEYS.contains(&k.as_str()));

            if has_op_keys {
                // Merge object fields, wrapping null operation values
                for (k, v) in inner_table {
                    if OPERATION_KEYS.contains(&k.as_str()) {
                        if matches!(v, feuilletage::ContextValue::Null(_)) {
                            // Wrap null operation value to preserve it
                            let null_wrapped = wrap_null_value(ctx);
                            obj.insert(
                                k.clone(),
                                feuilletage::ContextValue::array(vec![null_wrapped], ctx.clone()),
                            );
                        } else {
                            obj.insert(k.clone(), v.clone());
                        }
                    } else {
                        obj.insert(k.clone(), v.clone());
                    }
                }
            } else {
                // No operation keys → treat as {value: X, type: Y} style
                for (k, v) in inner_table {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }
        // Scalar value -> implicit set
        _ => {
            obj.insert("set".to_string(), var_value.clone());
        }
    }

    obj
}

/// Create a `{value: null}` ContextValue object to wrap null values.
fn wrap_null_value<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    ctx: &feuilletage::Context<S, L>,
) -> feuilletage::ContextValue<S, L> {
    let mut wrapped = indexmap::IndexMap::new();
    wrapped.insert(
        "value".to_string(),
        feuilletage::ContextValue::null(ctx.clone()),
    );
    feuilletage::ContextValue::object(wrapped, ctx.clone())
}

// ============================================================================
// Helper: coerce ContextValue to String
// ============================================================================

fn coerce_to_string<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &feuilletage::ContextValue<S, L>,
    tracker: &mut feuilletage::ErrorTracker,
) -> Result<String, feuilletage::Error> {
    match value {
        feuilletage::ContextValue::String(s, _) => Ok(s.clone()),
        feuilletage::ContextValue::Int(i, _) => Ok(i.to_string()),
        feuilletage::ContextValue::Float(f, _) => Ok(f.to_string()),
        feuilletage::ContextValue::Bool(b, _) => Ok(b.to_string()),
        _ => Err(feuilletage::Error::TypeMismatch {
            path: tracker.current_path(),
            expected: "scalar value".to_string(),
            actual: value.type_name().to_string(),
        }),
    }
}

// ============================================================================
// EnvOperationEnum (unchanged)
// ============================================================================

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
