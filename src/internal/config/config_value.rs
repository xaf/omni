use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::ConfigErrorHandler;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::parser::PathEntryConfig;
use crate::internal::config::utils::sort_serde_yaml;
use crate::internal::env::user_home;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_error;

// Re-export core types from config-value
pub use config_value::{
    ConfigData, ConfigError, ExtendStrategy as ConfigExtendStrategy, Scope, Source, Value,
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

// Extend options wrapper for omni-specific defaults
#[derive(Debug, Clone)]
pub struct ConfigExtendOptions {
    pub strategy: ConfigExtendStrategy,
    pub transform: bool,
}

impl Default for ConfigExtendOptions {
    fn default() -> Self {
        Self {
            strategy: ConfigExtendStrategy::Default,
            transform: true,
        }
    }
}

impl ConfigExtendOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_strategy(mut self, strategy: ConfigExtendStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn with_transform(mut self, transform: bool) -> Self {
        self.transform = transform;
        self
    }

    fn into_inner(self) -> config_value::ExtendOptions {
        config_value::ExtendOptions::default()
            .with_strategy(self.strategy)
            .with_transform(self.transform)
    }
}

// Extension trait for omni-specific ConfigValue methods
pub trait ConfigValueExt {
    fn empty() -> Self;
    fn from_str(value: &str) -> Result<Self, serde_yaml::Error>
    where
        Self: Sized;
    fn from_table(table: HashMap<String, ConfigValue>) -> Self;
    fn extend(&mut self, other: ConfigValue, options: ConfigExtendOptions, keypath: Vec<String>);
    fn unwrap(&self) -> serde_yaml::Value;
    fn as_yaml(&self) -> String;
    fn get_source(&self) -> &ConfigSource;
    fn get_scope(&self) -> ConfigScope;

    // Helper methods with ConfigErrorHandler for error reporting
    fn get_as_str_or_none(&self, key: &str, error_handler: &ConfigErrorHandler) -> Option<String>;
    fn get_as_str_or_default(
        &self,
        key: &str,
        default: &str,
        error_handler: &ConfigErrorHandler,
    ) -> String;
    fn get_as_str_array(&self, key: &str, error_handler: &ConfigErrorHandler) -> Vec<String>;
    fn get_as_bool_or_none(&self, key: &str, error_handler: &ConfigErrorHandler) -> Option<bool>;
    fn get_as_bool_or_default(
        &self,
        key: &str,
        default: bool,
        error_handler: &ConfigErrorHandler,
    ) -> bool;
    fn get_as_float_or_none(&self, key: &str, error_handler: &ConfigErrorHandler) -> Option<f64>;
    fn get_as_float_or_default(
        &self,
        key: &str,
        default: f64,
        error_handler: &ConfigErrorHandler,
    ) -> f64;
    fn get_as_integer_or_none(&self, key: &str, error_handler: &ConfigErrorHandler)
        -> Option<i64>;
    fn select_keys(&self, keys: Vec<String>) -> Option<ConfigValue>;
}

impl ConfigValueExt for ConfigValue {
    fn empty() -> Self {
        ConfigValue::empty(ConfigSource::Default, ConfigScope::Default)
    }

    fn from_str(value: &str) -> Result<Self, serde_yaml::Error> {
        let value: serde_yaml::Value = serde_yaml::from_str(value)?;
        Ok(ConfigValue::from_value(
            ConfigSource::Null,
            ConfigScope::Null,
            value,
        ))
    }

    fn from_table(table: HashMap<String, ConfigValue>) -> Self {
        ConfigValue::from_table(ConfigSource::Null, ConfigScope::Null, table)
    }

    fn extend(&mut self, other: ConfigValue, options: ConfigExtendOptions, keypath: Vec<String>) {
        // Omni-specific extend with path resolution transform
        let transform_enabled = options.transform;
        self.extend_with_transform(
            other,
            options.into_inner(),
            keypath,
            |value: &mut ConfigValue, keypath: &[String]| {
                if transform_enabled {
                    omni_transform(value, keypath);
                }
            },
        );
    }

    fn unwrap(&self) -> serde_yaml::Value {
        // Use the new unwrap() method from config-value that returns Value,
        // then convert to serde_yaml::Value for backward compatibility
        let value = config_value::ConfigValue::unwrap(self);
        value.into()
    }

    fn as_yaml(&self) -> String {
        // Use the new as_yaml() method from config-value which handles sorting
        config_value::ConfigValue::as_yaml(self)
    }

    fn get_source(&self) -> &ConfigSource {
        self.source()
    }

    fn get_scope(&self) -> ConfigScope {
        self.scope().clone()
    }

    fn get_as_str_or_none(&self, key: &str, error_handler: &ConfigErrorHandler) -> Option<String> {
        if let Some(value) = self.get(key) {
            match value.as_str_forced() {
                Some(value) => Some(value),
                None => {
                    error_handler
                        .with_expected("string")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    None
                }
            }
        } else {
            None
        }
    }

    fn get_as_str_or_default(
        &self,
        key: &str,
        default: &str,
        error_handler: &ConfigErrorHandler,
    ) -> String {
        if let Some(value) = self.get(key) {
            match value.as_str_forced() {
                Some(value) => value,
                None => {
                    error_handler
                        .with_expected("string")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    default.to_string()
                }
            }
        } else {
            default.to_string()
        }
    }

    fn get_as_str_array(&self, key: &str, error_handler: &ConfigErrorHandler) -> Vec<String> {
        // Use the base method but report errors
        let result = self.get_as_str_array(key);

        // Check if we got nothing and there was a value that couldn't be converted
        if result.is_empty() {
            if let Some(value) = self.get(key) {
                if !value.is_array() && !value.is_str() {
                    error_handler
                        .with_expected("string or array of strings")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                }
            }
        }

        result
    }

    fn get_as_bool_or_none(&self, key: &str, error_handler: &ConfigErrorHandler) -> Option<bool> {
        if let Some(value) = self.get(key) {
            match value.as_bool_forced() {
                Some(value) => Some(value),
                None => {
                    error_handler
                        .with_expected("bool")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    None
                }
            }
        } else {
            None
        }
    }

    fn get_as_bool_or_default(
        &self,
        key: &str,
        default: bool,
        error_handler: &ConfigErrorHandler,
    ) -> bool {
        if let Some(value) = self.get(key) {
            match value.as_bool_forced() {
                Some(value) => value,
                None => {
                    error_handler
                        .with_expected("bool")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    default
                }
            }
        } else {
            default
        }
    }

    fn get_as_float_or_none(&self, key: &str, error_handler: &ConfigErrorHandler) -> Option<f64> {
        if let Some(value) = self.get(key) {
            match value.as_float() {
                Some(value) => Some(value),
                None => {
                    error_handler
                        .with_expected("float")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    None
                }
            }
        } else {
            None
        }
    }

    fn get_as_float_or_default(
        &self,
        key: &str,
        default: f64,
        error_handler: &ConfigErrorHandler,
    ) -> f64 {
        if let Some(value) = self.get(key) {
            match value.as_float() {
                Some(value) => value,
                None => {
                    error_handler
                        .with_expected("float")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    default
                }
            }
        } else {
            default
        }
    }

    fn get_as_integer_or_none(
        &self,
        key: &str,
        error_handler: &ConfigErrorHandler,
    ) -> Option<i64> {
        if let Some(value) = self.get(key) {
            match value.as_integer() {
                Some(value) => Some(value),
                None => {
                    error_handler
                        .with_expected("integer")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    None
                }
            }
        } else {
            None
        }
    }

    fn select_keys(&self, keys: Vec<String>) -> Option<ConfigValue> {
        if let Some(mapping) = self.as_table() {
            let mut new_mapping = HashMap::new();
            for key in keys {
                if let Some(value) = mapping.get(&key) {
                    new_mapping.insert(key, value.clone());
                }
            }
            return Some(ConfigValue::from_table(
                self.source().clone(),
                self.scope().clone(),
                new_mapping,
            ));
        }
        None
    }
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
                            ConfigValue::from_value(
                                source.clone(),
                                value.scope().clone(),
                                serde_yaml::Value::String(
                                    package_config.package.clone().unwrap().to_string(),
                                ),
                            ),
                        );
                        package_path.insert(
                            "path".to_string(),
                            ConfigValue::from_value(
                                source.clone(),
                                value.scope().clone(),
                                serde_yaml::Value::String(relpath),
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
        *value = ConfigValue::from_value(
            value.source().clone(),
            value.scope().clone(),
            serde_yaml::Value::String(abs_path),
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

// Implement Deserialize for ConfigValue by deserializing as serde_yaml::Value first
impl<'de> Deserialize<'de> for ConfigValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;
        Ok(ConfigValue::from_value(
            ConfigSource::Default,
            ConfigScope::Default,
            value,
        ))
    }
}

impl Default for ConfigValue {
    fn default() -> Self {
        ConfigValue::new_null(ConfigSource::Null, ConfigScope::Null)
    }
}
