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

#[derive(Debug, Default, Clone, feuilletage::Config)]
#[feuilletage(parse_as = "EnvConfigWire", skip_serialize, skip_deserialize)]
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

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>
    feuilletage::FromParsed<EnvConfigWire, S, L> for EnvConfig
{
    fn from_parsed(
        parsed: EnvConfigWire,
        _original: &feuilletage::ContextValue<S, L>,
        _tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        let operations = parsed
            .0
            .iter()
            .flat_map(EnvVarConfig::to_operations)
            .collect();

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
// EnvOpValue - value+type pair
// ============================================================================

#[derive(Debug, Clone, Default, feuilletage::Config)]
#[feuilletage(parse_as = "EnvOpValueWire", skip_serialize, skip_deserialize)]
struct EnvOpValue {
    value: Option<String>,
    value_type: String,
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>
    feuilletage::FromParsed<EnvOpValueWire, S, L> for EnvOpValue
{
    fn from_parsed(
        parsed: EnvOpValueWire,
        _original: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        let (value, value_type) = match parsed {
            EnvOpValueWire::Fields(fields) => (
                fields.value.into_string(),
                if fields.type_was_null {
                    return Err(env_value_type_mismatch(tracker, "null"));
                } else {
                    fields.value_type.into_string(tracker)?
                },
            ),
            EnvOpValueWire::Scalar(value) => (value.into_string(), "text".to_string()),
            EnvOpValueWire::InvalidArray => {
                return Err(feuilletage::Error::TypeMismatch {
                    path: tracker.current_path(),
                    expected: "scalar value".to_string(),
                    actual: "array".to_string(),
                });
            }
        };

        Ok(Self { value, value_type })
    }
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(untagged, skip_serialize, skip_deserialize)]
enum EnvOpValueWire {
    Fields(EnvOpValueFieldsWire),
    Scalar(EnvScalarWire),
    #[feuilletage(variant = predicate("env_value_is_array"))]
    InvalidArray,
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(
    transform = "self::normalize_env_op_value_fields_wire",
    skip_serialize,
    skip_deserialize
)]
struct EnvOpValueFieldsWire {
    #[feuilletage(default)]
    value: EnvScalarWire,
    #[feuilletage(default, rename = "type")]
    value_type: EnvValueTypeWire,
    #[feuilletage(default, rename = "__omni_env_type_was_null")]
    type_was_null: bool,
}

fn normalize_env_op_value_fields_wire<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &mut feuilletage::ContextValue<S, L>,
    _context: &feuilletage::Context<S, L>,
) -> Result<(), feuilletage::Error> {
    let feuilletage::ContextValue::Object(fields, _) = value else {
        return Ok(());
    };

    fields.shift_remove("__omni_env_type_was_null");
    let Some(feuilletage::ContextValue::Null(context)) = fields.get("type") else {
        return Ok(());
    };
    let context = context.clone();
    fields.shift_remove("type");
    fields.insert(
        "__omni_env_type_was_null".to_string(),
        feuilletage::ContextValue::bool(true, context),
    );
    Ok(())
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(untagged, skip_serialize, skip_deserialize)]
#[derive(Default)]
enum EnvScalarWire {
    #[feuilletage(variant = null)]
    #[default]
    Null,
    #[feuilletage(variant = any_bool)]
    Bool(bool),
    #[feuilletage(variant = any_int)]
    Int(i64),
    #[feuilletage(variant = any_float)]
    Float(f64),
    #[feuilletage(variant = any_string)]
    String(String),
}


impl EnvScalarWire {
    fn into_string(self) -> Option<String> {
        match self {
            Self::Null => None,
            Self::Bool(value) => Some(value.to_string()),
            Self::Int(value) => Some(value.to_string()),
            Self::Float(value) => Some(value.to_string()),
            Self::String(value) => Some(value),
        }
    }
}

#[derive(Debug, Default, feuilletage::Config)]
#[feuilletage(untagged, skip_serialize, skip_deserialize)]
enum EnvValueTypeWire {
    #[default]
    #[feuilletage(variant = "text")]
    Text,
    #[feuilletage(variant = "path")]
    Path,
    #[feuilletage(variant = any_string)]
    InvalidString(String),
    #[feuilletage(variant = any_bool)]
    InvalidBool,
    #[feuilletage(variant = any_int)]
    InvalidInt,
    #[feuilletage(variant = any_float)]
    InvalidFloat,
    #[feuilletage(variant = predicate("env_value_is_array"))]
    InvalidArray,
    #[feuilletage(variant = predicate("env_value_is_object"))]
    InvalidObject,
}

impl EnvValueTypeWire {
    fn into_string(
        self,
        tracker: &feuilletage::ErrorTracker,
    ) -> Result<String, feuilletage::Error> {
        match self {
            Self::Text => Ok("text".to_string()),
            Self::Path => Ok("path".to_string()),
            Self::InvalidString(value) => Err(feuilletage::Error::InvalidValue {
                path: tracker.current_path(),
                message: format!("type must be 'text' or 'path', got '{}'", value),
            }),
            Self::InvalidBool => Err(env_value_type_mismatch(tracker, "bool")),
            Self::InvalidInt => Err(env_value_type_mismatch(tracker, "int")),
            Self::InvalidFloat => Err(env_value_type_mismatch(tracker, "float")),
            Self::InvalidArray => Err(env_value_type_mismatch(tracker, "array")),
            Self::InvalidObject => Err(env_value_type_mismatch(tracker, "object")),
        }
    }
}

fn env_value_type_mismatch(
    tracker: &feuilletage::ErrorTracker,
    actual: &str,
) -> feuilletage::Error {
    feuilletage::Error::TypeMismatch {
        path: tracker.current_path(),
        expected: "string".to_string(),
        actual: actual.to_string(),
    }
}

fn env_value_is_array<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &feuilletage::ContextValue<S, L>,
) -> bool {
    matches!(value, feuilletage::ContextValue::Array(_, _))
}

fn env_value_is_object<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &feuilletage::ContextValue<S, L>,
) -> bool {
    matches!(value, feuilletage::ContextValue::Object(_, _))
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
                    let parent = source_path
                        .parent()
                        .map(|p| p.to_string_lossy().to_string());
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

#[derive(Debug, feuilletage::Config)]
#[feuilletage(
    transparent,
    transform = "self::env_entries_transform",
    skip_serialize,
    skip_deserialize
)]
struct EnvConfigWire(Vec<EnvVarConfig>);

/// Operation keys recognized in env var config objects.
const OPERATION_KEYS: &[&str] = &["set", "prepend", "append", "remove", "prefix", "suffix"];

/// Pre-processes the ContextValue input to normalize env config entries.
///
/// - If input is Object: convert to sorted array of single-key objects
/// - For each array element that is a single-key object `{KEY: value}`:
///   - Extract key -> inject as `name` field
///   - If value is null: create `{name: KEY, set: [null]}`
///   - If value is scalar: create `{name: KEY, set: value}`
///   - If value is object with op keys: merge and wrap null op values
///   - If value is object without op keys: merge as value/type fields
fn env_entries_transform<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &mut feuilletage::ContextValue<S, L>,
    context: &feuilletage::Context<S, L>,
) -> Result<(), feuilletage::Error> {
    if value.is_null() {
        *value = feuilletage::ContextValue::array(Vec::new(), context.clone());
        return Ok(());
    }

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
            // Keep the operation field non-null so its default does not discard the unset.
            let set_array = feuilletage::ContextValue::array(
                vec![feuilletage::ContextValue::null(ctx.clone())],
                ctx.clone(),
            );
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
                            obj.insert(
                                k.clone(),
                                feuilletage::ContextValue::array(
                                    vec![feuilletage::ContextValue::null(ctx.clone())],
                                    ctx.clone(),
                                ),
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use feuilletage::FromContextValue;

    use super::*;

    fn parse_env_with_source(
        yaml: &str,
        source: feuilletage::Source,
    ) -> (
        Result<EnvConfig, feuilletage::Error>,
        feuilletage::ErrorTracker,
    ) {
        let context = feuilletage::Context::new(source, feuilletage::Level::User);
        let mut config = feuilletage::Config::default();
        config.load_yaml(yaml, context);
        let mut tracker = feuilletage::ErrorTracker::new();
        let result = EnvConfig::from_context_value(config.root(), &mut tracker);
        (result, tracker)
    }

    #[test]
    fn preserves_null_scalar_and_object_values() {
        let (env, tracker) = parse_env_with_source(
            "UNSET: null\nINT: 42\nBOOL: true\nOBJECT:\n  value: 3.5\n  type: text\n",
            feuilletage::Source::Programmatic,
        );
        let env = env.unwrap();

        let values = env
            .operations
            .iter()
            .map(|op| (op.name.as_str(), op.value.as_deref()))
            .collect::<HashMap<_, _>>();
        assert_eq!(values["UNSET"], None);
        assert_eq!(values["INT"], Some("42"));
        assert_eq!(values["BOOL"], Some("true"));
        assert_eq!(values["OBJECT"], Some("3.5"));
        assert!(tracker.errors().is_empty(), "{:#?}", tracker.errors());
    }

    #[test]
    fn preserves_array_form_and_one_to_many_scalar_coercions() {
        let (env, tracker) = parse_env_with_source(
            "- SECOND:\n    append:\n      - null\n      - true\n      - 42\n      - 3.5\n      - text\n- FIRST: first\n",
            feuilletage::Source::Programmatic,
        );
        let env = env.unwrap();

        assert_eq!(
            env.operations
                .iter()
                .map(|op| (op.name.as_str(), op.operation, op.value.as_deref()))
                .collect::<Vec<_>>(),
            vec![
                ("SECOND", EnvOperationEnum::Append, None),
                ("SECOND", EnvOperationEnum::Append, Some("true")),
                ("SECOND", EnvOperationEnum::Append, Some("42")),
                ("SECOND", EnvOperationEnum::Append, Some("3.5")),
                ("SECOND", EnvOperationEnum::Append, Some("text")),
                ("FIRST", EnvOperationEnum::Set, Some("first")),
            ]
        );
        assert!(tracker.errors().is_empty(), "{:#?}", tracker.errors());
    }

    #[test]
    fn accepts_null_top_level_as_empty_env() {
        let (env, tracker) = parse_env_with_source("null\n", feuilletage::Source::Programmatic);

        assert!(env.unwrap().operations.is_empty());
        assert!(tracker.errors().is_empty(), "{:#?}", tracker.errors());
    }

    #[test]
    fn resolves_path_values_relative_to_source_file() {
        let source = PathBuf::from("/tmp/omni-config/config.yaml");
        let (env, tracker) = parse_env_with_source(
            "DATA:\n  prepend:\n    value: ./data\n    type: path\n",
            feuilletage::Source::File(source),
        );
        let env = env.unwrap();

        assert_eq!(env.operations.len(), 1);
        assert_eq!(env.operations[0].operation, EnvOperationEnum::Prepend);
        assert_eq!(
            env.operations[0].value.as_deref(),
            Some("/tmp/omni-config/data")
        );
        assert!(tracker.errors().is_empty(), "{:#?}", tracker.errors());
    }

    #[test]
    fn preserves_invalid_type_diagnostics() {
        let context =
            feuilletage::Context::new(feuilletage::Source::Programmatic, feuilletage::Level::User);
        let mut config = feuilletage::Config::default();
        config.load_yaml("value: ok\ntype: binary\n", context);
        let mut tracker = feuilletage::ErrorTracker::new();
        let env = EnvOpValue::from_context_value(config.root(), &mut tracker);

        assert!(matches!(
            env,
            Err(feuilletage::Error::InvalidValue { message, .. })
                if message == "type must be 'text' or 'path', got 'binary'"
        ));
        assert!(tracker.errors().is_empty(), "{:#?}", tracker.errors());
    }

    #[test]
    fn preserves_non_string_type_diagnostics() {
        for (value_type, actual) in [
            ("true", "bool"),
            ("42", "int"),
            ("3.5", "float"),
            ("null", "null"),
            ("[]", "array"),
            ("{}", "object"),
        ] {
            let context = feuilletage::Context::new(
                feuilletage::Source::Programmatic,
                feuilletage::Level::User,
            );
            let mut config = feuilletage::Config::default();
            config.load_yaml(&format!("value: ok\ntype: {value_type}\n"), context);
            let mut tracker = feuilletage::ErrorTracker::new();
            let env = EnvOpValue::from_context_value(config.root(), &mut tracker);

            assert!(
                matches!(
                    &env,
                    Err(feuilletage::Error::TypeMismatch {
                        expected,
                        actual: error_actual,
                        ..
                    }) if expected == "string" && error_actual == actual
                ),
                "type {value_type} should report {actual}, got {env:?}"
            );
            assert!(tracker.errors().is_empty(), "{:#?}", tracker.errors());
        }
    }

    #[test]
    fn rejects_non_scalar_operation_values() {
        for value in ["[]", "{nested: value}"] {
            let context = feuilletage::Context::new(
                feuilletage::Source::Programmatic,
                feuilletage::Level::User,
            );
            let mut config = feuilletage::Config::default();
            config.load_yaml(&format!("value: {value}\n"), context);
            let mut tracker = feuilletage::ErrorTracker::new();

            assert!(
                EnvOpValue::from_context_value(config.root(), &mut tracker).is_err(),
                "operation value {value} should be rejected"
            );
        }
    }

    #[test]
    fn preserves_operation_precedence_and_order() {
        let (env, tracker) = parse_env_with_source(
            "ORDERED:\n  remove: remove\n  prepend: prepend\n  append: append\n  prefix: prefix\n  suffix: suffix\nSET_WINS:\n  set: selected\n  append: ignored\n",
            feuilletage::Source::Programmatic,
        );
        let env = env.unwrap();

        assert_eq!(
            env.operations
                .iter()
                .map(|op| (op.name.as_str(), op.operation, op.value.as_deref()))
                .collect::<Vec<_>>(),
            vec![
                ("ORDERED", EnvOperationEnum::Remove, Some("remove")),
                ("ORDERED", EnvOperationEnum::Prepend, Some("prepend")),
                ("ORDERED", EnvOperationEnum::Append, Some("append")),
                ("ORDERED", EnvOperationEnum::Prefix, Some("prefix")),
                ("ORDERED", EnvOperationEnum::Suffix, Some("suffix")),
                ("SET_WINS", EnvOperationEnum::Set, Some("selected")),
            ]
        );
        assert!(tracker.errors().is_empty(), "{:#?}", tracker.errors());
    }

    #[test]
    fn serialization_remains_null_or_an_operation_array() {
        assert_eq!(
            serde_json::to_value(EnvConfig::default()).unwrap(),
            serde_json::Value::Null
        );

        let (env, tracker) = parse_env_with_source(
            "PATH:\n  prepend: ./bin\nUNSET: null\n",
            feuilletage::Source::Programmatic,
        );
        let serialized = serde_json::to_value(env.unwrap()).unwrap();

        assert_eq!(
            serialized,
            serde_json::json!([
                {"PATH": {"prepend": "./bin"}},
                {"UNSET": null}
            ])
        );
        assert!(tracker.errors().is_empty(), "{:#?}", tracker.errors());
    }
}
