use std::collections::HashMap;

use serde::Deserialize;
use serde::Serialize;
use tera::Context;
use tera::Tera;

use crate::internal::cache::utils::Empty;
use crate::internal::config::template::config_template_context;
use crate::internal::config::template::render_config_template;
use crate::internal::config::template::tera_render_error_message;
use crate::internal::config::ConfigValue;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_warning;

// Compote imports
use compote::ConfigContext as CompoteConfigContext;
use compote::ConfigError as CompoteConfigError;
use compote::ConfigLevel as CompoteConfigLevel;
use compote::ConfigSource as CompoteConfigSource;
use compote::ConfigValue as CompoteConfigValue;
use compote::ErrorTracker as CompoteErrorTracker;
use compote::FromConfigValue as CompoteFromConfigValue;
use compote::Value as CompoteValue;

// ============================================================================
// COMPOTE CONVERSION: Store compote::Value directly
// ============================================================================
//
// Strategy: Store compote::Value directly instead of serializing to YAML string.
// This is the clean approach that moves toward eliminating config_value entirely.
//
// Key points:
// 1. The `config` field stores `compote::Value` directly
// 2. `config()` returns `&compote::Value`
// 3. `config_value()` converts to config_value::ConfigValue for backward compat
// 4. Custom FromConfigValue impl handles variant-style parsing
//
// ============================================================================

/// Create a synthetic ConfigContext for deserialized values
fn synthetic_context() -> CompoteConfigContext {
    CompoteConfigContext::new(CompoteConfigSource::Programmatic, CompoteConfigLevel::Local)
}

/// Wrapper for storing arbitrary config as compote::Value
#[derive(Debug, Clone)]
pub struct StoredConfig(pub CompoteValue);

impl Default for StoredConfig {
    fn default() -> Self {
        StoredConfig(CompoteValue::Null)
    }
}

impl StoredConfig {
    pub fn is_empty(&self) -> bool {
        matches!(self.0, CompoteValue::Null)
    }

    /// Get the stored compote::Value
    /// This is the new API - callers should migrate to using this instead of to_config_value()
    #[allow(dead_code)]
    pub fn value(&self) -> &CompoteValue {
        &self.0
    }

    /// Convert to config_value::ConfigValue for backward compatibility
    /// This method will be removed once config_value is fully eliminated
    pub fn to_config_value(&self) -> ConfigValue {
        if self.is_empty() {
            return ConfigValue::default();
        }

        // Convert compote::Value to config_value::Value, then to ConfigValue
        let primitive = compote_value_to_config_value(&self.0);
        match primitive.to_yaml_string() {
            Ok(yaml_str) => {
                ConfigValue::from_str_with(Default::default(), Default::default(), &yaml_str)
                    .unwrap_or_default()
            }
            Err(_) => ConfigValue::default(),
        }
    }
}

impl Serialize for StoredConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize the compote::Value directly
        serialize_compote_value(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for StoredConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize as serde_yaml::Value and convert to compote::Value
        let value: serde_yaml::Value = serde_yaml::Value::deserialize(deserializer)?;
        let compote_value = yaml_value_to_compote_value(value);
        Ok(StoredConfig(compote_value))
    }
}

impl CompoteFromConfigValue for StoredConfig {
    fn from_config_value(
        value: &CompoteConfigValue,
        _tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteConfigError> {
        // Just clone the value directly
        Ok(StoredConfig(value.value.clone()))
    }
}

/// Convert compote::Value to config_value::Value
fn compote_value_to_config_value(value: &CompoteValue) -> config_value::Value {
    match value {
        CompoteValue::Null => config_value::Value::Null,
        CompoteValue::Bool(b) => config_value::Value::Bool(*b),
        CompoteValue::Int(i) => config_value::Value::Integer(*i),
        CompoteValue::Float(f) => config_value::Value::Float(*f),
        CompoteValue::String(s) => config_value::Value::String(s.clone()),
        CompoteValue::Array(arr) => {
            let items: Vec<config_value::Value> =
                arr.iter().map(|v| compote_value_to_config_value(&v.value)).collect();
            config_value::Value::Sequence(items)
        }
        CompoteValue::Object(map) => {
            let items: std::collections::HashMap<String, config_value::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), compote_value_to_config_value(&v.value)))
                .collect();
            config_value::Value::Mapping(items)
        }
    }
}

/// Convert serde_yaml::Value to compote::Value
fn yaml_value_to_compote_value(value: serde_yaml::Value) -> CompoteValue {
    match value {
        serde_yaml::Value::Null => CompoteValue::Null,
        serde_yaml::Value::Bool(b) => CompoteValue::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                CompoteValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                CompoteValue::Float(f)
            } else {
                CompoteValue::Null
            }
        }
        serde_yaml::Value::String(s) => CompoteValue::String(s),
        serde_yaml::Value::Sequence(arr) => {
            let items: Vec<CompoteConfigValue> = arr
                .into_iter()
                .map(|v| CompoteConfigValue {
                    value: yaml_value_to_compote_value(v),
                    context: synthetic_context(),
                })
                .collect();
            CompoteValue::Array(items)
        }
        serde_yaml::Value::Mapping(map) => {
            let items: indexmap::IndexMap<String, CompoteConfigValue> = map
                .into_iter()
                .filter_map(|(k, v)| {
                    let key = match k {
                        serde_yaml::Value::String(s) => s,
                        _ => return None,
                    };
                    Some((
                        key,
                        CompoteConfigValue {
                            value: yaml_value_to_compote_value(v),
                            context: synthetic_context(),
                        },
                    ))
                })
                .collect();
            CompoteValue::Object(items)
        }
        serde_yaml::Value::Tagged(tagged) => yaml_value_to_compote_value(tagged.value),
    }
}

/// Serialize compote::Value using serde
fn serialize_compote_value<S>(value: &CompoteValue, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        CompoteValue::Null => serializer.serialize_none(),
        CompoteValue::Bool(b) => serializer.serialize_bool(*b),
        CompoteValue::Int(i) => serializer.serialize_i64(*i),
        CompoteValue::Float(f) => serializer.serialize_f64(*f),
        CompoteValue::String(s) => serializer.serialize_str(s),
        CompoteValue::Array(arr) => {
            use serde::ser::SerializeSeq;
            let mut seq = serializer.serialize_seq(Some(arr.len()))?;
            for item in arr {
                seq.serialize_element(&SerializableCompoteValue(&item.value))?;
            }
            seq.end()
        }
        CompoteValue::Object(map) => {
            use serde::ser::SerializeMap;
            let mut m = serializer.serialize_map(Some(map.len()))?;
            for (k, v) in map {
                m.serialize_entry(k, &SerializableCompoteValue(&v.value))?;
            }
            m.end()
        }
    }
}

/// Helper for serializing compote::Value
struct SerializableCompoteValue<'a>(&'a CompoteValue);

impl<'a> Serialize for SerializableCompoteValue<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_compote_value(self.0, serializer)
    }
}

// Note: We implement FromConfigValue manually because:
// 1. The struct has variant-style parsing (config vs template vs template_file)
// 2. We need custom Serialize behavior (serialize config directly, not as struct field)
// 3. compote::Config derive would conflict with our custom Serialize impl
#[derive(Debug, Clone)]
pub struct SuggestConfig {
    /// Arbitrary config value, stored as compote::Value
    pub config: StoredConfig,

    pub template: String,

    pub template_file: String,
}

/// Implement FromConfigValue manually to handle variant-style parsing
impl CompoteFromConfigValue for SuggestConfig {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteConfigError> {
        // Check if it's an object with special keys
        if let CompoteValue::Object(map) = &value.value {
            // Check for "config" key - use its value directly
            if let Some(config_val) = map.get("config") {
                return Ok(Self {
                    config: StoredConfig(config_val.value.clone()),
                    template: String::new(),
                    template_file: String::new(),
                });
            }

            // Check for "template" key
            if let Some(template_val) = map.get("template") {
                if let CompoteValue::String(s) = &template_val.value {
                    return Ok(Self {
                        config: StoredConfig::default(),
                        template: s.clone(),
                        template_file: String::new(),
                    });
                }
            }

            // Check for "template_file" key
            if let Some(template_file_val) = map.get("template_file") {
                if let CompoteValue::String(s) = &template_file_val.value {
                    return Ok(Self {
                        config: StoredConfig::default(),
                        template: String::new(),
                        template_file: s.clone(),
                    });
                }
            }
        }

        // If not a special variant, treat the entire value as config
        let stored_config = StoredConfig::from_config_value(value, tracker)?;
        Ok(Self {
            config: stored_config,
            template: String::new(),
            template_file: String::new(),
        })
    }
}

impl Empty for SuggestConfig {
    fn is_empty(&self) -> bool {
        self.config.is_empty() && self.template.is_empty() && self.template_file.is_empty()
    }
}

impl compote::IsEmpty for SuggestConfig {
    fn is_empty(&self) -> bool {
        Empty::is_empty(self)
    }
}

// Custom serialization: serialize config directly if present
impl Serialize for SuggestConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if !self.config.is_empty() {
            // Serialize the config value directly (not wrapped in a struct)
            self.config.serialize(serializer)
        } else if !self.template.is_empty() || !self.template_file.is_empty() {
            let mut map = HashMap::new();
            if !self.template.is_empty() {
                map.insert("template".to_string(), self.template.clone());
            } else if !self.template_file.is_empty() {
                map.insert("template_file".to_string(), self.template_file.clone());
            }
            map.serialize(serializer)
        } else {
            serializer.serialize_none()
        }
    }
}

impl SuggestConfig {
    /// Get the config as compote::Value
    /// This is the new API - callers should migrate to using this instead of config_value()
    #[allow(dead_code)]
    pub fn config(&self) -> &CompoteValue {
        self.config_in_context(".")
    }

    /// Get the config as compote::Value in the given context
    /// This is the new API - callers should migrate to using this instead of config_value_in_context()
    #[allow(dead_code)]
    pub fn config_in_context(&self, _path: &str) -> &CompoteValue {
        // For direct config, just return it
        // Template processing returns owned value, so we need internal mutability or different API
        // For now, if template is set, we don't support context-based config retrieval via this method
        if !self.config.is_empty() {
            return self.config.value();
        }

        // If template/template_file is set, caller should use config_value() instead
        // which handles template rendering
        &self.config.0
    }

    /// Get the config as config_value::ConfigValue (for backward compatibility)
    /// This method will be removed once config_value is fully eliminated from omni
    pub fn config_value(&self) -> ConfigValue {
        self.config_value_in_context(".")
    }

    /// Get the config as config_value::ConfigValue in context (for backward compatibility)
    pub fn config_value_in_context(&self, path: &str) -> ConfigValue {
        let context = config_template_context(path);
        self.config_value_with_context(&context)
    }

    fn config_value_with_context(&self, template_context: &Context) -> ConfigValue {
        // If we have config stored directly, convert to ConfigValue
        if !self.config.is_empty() {
            return self.config.to_config_value();
        }

        let mut template = Tera::default();
        if !self.template.is_empty() {
            if let Err(err) = template.add_raw_template("suggest_config", &self.template) {
                omni_warning!(tera_render_error_message(err));
                omni_warning!("suggest_config will be ignored");
                return ConfigValue::default();
            }
        } else if !self.template_file.is_empty() {
            if let Err(err) = template.add_template_file(&self.template_file, None) {
                omni_warning!(tera_render_error_message(err));
                omni_warning!("suggest_config will be ignored");
                return ConfigValue::default();
            }
        }

        if !template.templates.is_empty() {
            match render_config_template(&template, template_context) {
                Ok(yaml_str) => {
                    // Parse YAML string using compote
                    match serde_yaml::from_str::<serde_yaml::Value>(&yaml_str) {
                        Ok(yaml_value) => {
                            // Convert to compote::Value and deserialize
                            let compote_value = yaml_value_to_compote_value(yaml_value);
                            let config_value = CompoteConfigValue {
                                value: compote_value,
                                context: synthetic_context(),
                            };
                            let mut tracker = CompoteErrorTracker::new();
                            match Self::from_config_value(&config_value, &mut tracker) {
                                Ok(suggest) => {
                                    // In case this is recursive for some reason...
                                    return suggest.config_value_with_context(template_context);
                                }
                                Err(err) => {
                                    omni_warning!(format!(
                                        "Failed to parse suggest_config template: {}",
                                        err
                                    ));
                                    omni_warning!("suggest_config will be ignored");
                                }
                            }
                        }
                        Err(err) => {
                            omni_warning!(format!(
                                "Failed to parse suggest_config template: {}",
                                err
                            ));
                            omni_warning!("suggest_config will be ignored");
                        }
                    }
                }
                Err(err) => {
                    omni_warning!(tera_render_error_message(err));
                    omni_warning!("suggest_config will be ignored");
                }
            }
        }

        ConfigValue::default()
    }
}

impl Default for SuggestConfig {
    fn default() -> Self {
        Self {
            config: StoredConfig::default(),
            template: String::new(),
            template_file: String::new(),
        }
    }
}

// Manual Deserialize implementation for backwards compatibility with cache files and serde usage
impl<'de> Deserialize<'de> for SuggestConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize as a map first and check for variant keys
        let value: serde_yaml::Value = serde_yaml::Value::deserialize(deserializer)?;

        // Check if it's a mapping with known keys
        if let serde_yaml::Value::Mapping(ref map) = value {
            // Check for "config" key
            if let Some(config_val) = map.get(&serde_yaml::Value::String("config".to_string())) {
                let compote_value = yaml_value_to_compote_value(config_val.clone());
                return Ok(SuggestConfig {
                    config: StoredConfig(compote_value),
                    template: String::new(),
                    template_file: String::new(),
                });
            }

            // Check for "template" key
            if let Some(serde_yaml::Value::String(template)) =
                map.get(&serde_yaml::Value::String("template".to_string()))
            {
                return Ok(SuggestConfig {
                    config: StoredConfig::default(),
                    template: template.clone(),
                    template_file: String::new(),
                });
            }

            // Check for "template_file" key
            if let Some(serde_yaml::Value::String(template_file)) =
                map.get(&serde_yaml::Value::String("template_file".to_string()))
            {
                return Ok(SuggestConfig {
                    config: StoredConfig::default(),
                    template: String::new(),
                    template_file: template_file.clone(),
                });
            }
        }

        // Otherwise, treat the entire value as config
        let compote_value = yaml_value_to_compote_value(value);
        Ok(SuggestConfig {
            config: StoredConfig(compote_value),
            template: String::new(),
            template_file: String::new(),
        })
    }
}
