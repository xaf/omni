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
pub fn omni_from_str(value: &str) -> Result<ConfigValue, serde_yaml::Error> {
    let value_obj = Value::from_yaml_str(value)?;
    Ok(config_value::ConfigValue::from_config_value(
        ConfigSource::Null,
        ConfigScope::Null,
        value_obj,
    ))
}

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
