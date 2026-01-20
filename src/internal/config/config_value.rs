use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::PathEntryConfig;
use crate::internal::env::user_home;

// Re-export core types from config-value
pub use config_value::{
    ConfigData,
    ExtendStrategy as ConfigExtendStrategy,
    Scope,
    Source,
    Value,
};

// Omni-specific source type
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConfigSource {
    #[default]
    Default,
    File(String),
    Package(PathEntryConfig),
    Null,
}

impl Source for ConfigSource {
    fn priority(&self) -> u32 {
        match self {
            Self::Null => 0,
            Self::Default => 1,
            Self::File(_) => 10,
            Self::Package(_) => 10,
        }
    }

    fn description(&self) -> String {
        match self {
            Self::Default => "default".to_string(),
            Self::File(path) => format!("file: {}", path),
            Self::Package(package) => format!("package: {}", package.path),
            Self::Null => "null".to_string(),
        }
    }
}

impl ConfigSource {
    pub fn path(&self) -> Option<String> {
        match self {
            Self::File(path) => Some(path.to_string()),
            Self::Package(package) => Some(package.path.clone()),
            _ => None,
        }
    }
}

// Omni-specific scope type
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq, Ord, PartialOrd)]
pub enum ConfigScope {
    Null,
    #[default]
    Default,
    System,
    User,
    Workdir,
}

impl Scope for ConfigScope {
    fn description(&self) -> String {
        format!("{:?}", self)
    }
}

// Type alias for omni's ConfigValue
pub type ConfigValue = config_value::ConfigValue<ConfigSource, ConfigScope>;

// Type alias for omni's config loader
pub type OmniConfigLoader = config_value::ConfigLoader<ConfigSource, ConfigScope>;

/// Create a config loader with omni's transformation rules
pub fn omni_config_loader() -> OmniConfigLoader {
    OmniConfigLoader::new()
        .with_transform_enabled(true)
        .with_transform(omni_transform)
}

// Omni-specific helper functions for ConfigValue
#[allow(dead_code)]
pub fn omni_from_str(value: &str) -> Result<ConfigValue, serde_yaml::Error> {
    let value_obj = Value::from_yaml_str(value)?;
    Ok(config_value::ConfigValue::from_config_value(
        ConfigSource::Null,
        ConfigScope::Null,
        value_obj,
    ))
}

#[allow(dead_code)]
pub fn omni_from_table(table: HashMap<String, ConfigValue>) -> ConfigValue {
    config_value::ConfigValue::from_table(ConfigSource::Null, ConfigScope::Null, table)
}

// Omni-specific path resolution transform
fn omni_transform(value: &mut ConfigValue, keypath: &[String]) {
    if !should_transform_keypath(keypath) {
        return;
    }

    // We need to access the internal value - use the generic methods
    if let Some(string_value) = value.as_str() {
        let mut abs_path = string_value.clone();

        // Handle ~/ expansion
        if abs_path.starts_with("~/") {
            abs_path = Path::new(&user_home())
                .join(abs_path.trim_start_matches("~/"))
                .to_str()
                .unwrap()
                .to_string();
        }

        // Handle relative paths
        if !abs_path.starts_with('/') {
            let source = value.source();
            match source {
                ConfigSource::File(source_path) => {
                    if let Some(parent) = Path::new(source_path).parent() {
                        abs_path = parent.join(&abs_path).to_str().unwrap().to_string();
                    }
                }
                ConfigSource::Package(package_config) => {
                    if let Some(relpath) = Path::new(&package_config.path).parent() {
                        let relpath = relpath.join(&abs_path).to_str().unwrap().to_string();

                        // For package sources, create a mapping with package and path
                        let mut package_path = HashMap::new();
                        package_path.insert(
                            "package".to_string(),
                            ConfigValue::from_config_value(
                                source.clone(),
                                value.scope().clone(),
                                Value::String(
                                    package_config.package.clone().unwrap().to_string(),
                                ),
                            ),
                        );
                        package_path.insert(
                            "path".to_string(),
                            ConfigValue::from_config_value(
                                source.clone(),
                                value.scope().clone(),
                                Value::String(relpath),
                            ),
                        );

                        // Replace the value with the mapping
                        *value = ConfigValue::from_table(
                            source.clone(),
                            value.scope().clone(),
                            package_path,
                        );
                        return;
                    }
                }
                _ => {}
            }
        }

        // Update the value with the resolved path
        *value = ConfigValue::from_config_value(
            value.source().clone(),
            value.scope().clone(),
            Value::String(abs_path),
        );
    }
}

fn should_transform_keypath(keypath: &[String]) -> bool {
    if keypath.is_empty() {
        return false;
    }

    match (keypath.len(), keypath[0].as_str()) {
        // path => append => <item> or path => prepend => <item>
        (3, "path") => matches!(keypath[1].as_str(), "append" | "prepend"),
        // org => <item> => worktree
        (3, "org") => matches!(keypath[2].as_str(), "worktree"),
        // cache => path
        (2, "cache") => matches!(keypath[1].as_str(), "path"),
        // suggest_clone => template_file
        (2, "suggest_clone") => matches!(keypath[1].as_str(), "template_file"),
        // suggest_config => template_file
        (2, "suggest_config") => matches!(keypath[1].as_str(), "template_file"),
        // worktree
        (1, "worktree") => true,
        // sandbox
        (1, "sandbox") => true,
        _ => false,
    }
}

// All ConfigValue methods are now provided by the config-value crate
// No wrapper functions needed here anymore

/// Convert omni's ConfigValue to compote's ConfigValue for use with compote's FromConfigValue trait.
///
/// This is needed when deserializing from the old ConfigLoader's raw_config into types
/// that derive `compote::Config`.
pub fn to_compote_config_value(old_value: &ConfigValue) -> compote::ConfigValue {
    let context = to_compote_context(old_value);
    let value = to_compote_inner_value(old_value, &context);
    compote::ConfigValue { value, context }
}

/// Convert old ConfigValue's inner value to compote Value
fn to_compote_inner_value(
    old_value: &ConfigValue,
    _parent_context: &compote::ConfigContext,
) -> compote::Value {
    if old_value.is_null() {
        compote::Value::Null
    } else if let Some(b) = old_value.as_bool() {
        compote::Value::Bool(b)
    } else if let Some(i) = old_value.as_integer() {
        compote::Value::Int(i)
    } else if let Some(f) = old_value.as_float() {
        compote::Value::Float(f)
    } else if let Some(s) = old_value.as_str() {
        compote::Value::String(s)
    } else if let Some(array) = old_value.as_array() {
        let converted: Vec<compote::ConfigValue> = array
            .iter()
            .map(|item| {
                let context = to_compote_context(&item);
                let inner = to_compote_inner_value(&item, &context);
                compote::ConfigValue {
                    value: inner,
                    context,
                }
            })
            .collect();
        compote::Value::Array(converted)
    } else if let Some(table) = old_value.as_table() {
        let converted: indexmap::IndexMap<String, compote::ConfigValue> = table
            .iter()
            .map(|(k, v)| {
                let context = to_compote_context(v);
                let inner = to_compote_inner_value(v, &context);
                (
                    k.clone(),
                    compote::ConfigValue {
                        value: inner,
                        context,
                    },
                )
            })
            .collect();
        compote::Value::Object(converted)
    } else {
        // Fallback to null for unknown types
        compote::Value::Null
    }
}

/// Convert old ConfigValue's context to compote ConfigContext
fn to_compote_context(old_value: &ConfigValue) -> compote::ConfigContext {
    let source = match old_value.source().path() {
        Some(path) => compote::ConfigSource::File(std::path::PathBuf::from(path)),
        None => compote::ConfigSource::Programmatic,
    };

    let level = match old_value.scope() {
        ConfigScope::System => compote::ConfigLevel::System,
        ConfigScope::User => compote::ConfigLevel::User,
        ConfigScope::Workdir => compote::ConfigLevel::Local,
        _ => compote::ConfigLevel::Local,
    };

    compote::ConfigContext::new(source, level)
}
