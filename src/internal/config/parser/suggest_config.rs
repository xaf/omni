use std::collections::HashMap;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils::Empty;

// Feuilletage imports
use crate::internal::config::FeuilletageConfigContext;
use crate::internal::config::FeuilletageConfigValue;
use crate::internal::config::FeuilletageConfigLevel;
use crate::internal::config::FeuilletageConfigSource;
use crate::internal::config::Value as FeuilletageValue;

// ============================================================================
// FEUILLETAGE CONVERSION: Store feuilletage::Value directly
// ============================================================================
//
// Strategy: Store feuilletage::Value directly instead of serializing to YAML string.
// This is the clean approach that has eliminated config_value from suggest_config.
//
// Key points:
// 1. The `config` field stores `feuilletage::Value` directly
// 2. `config()` returns `&feuilletage::Value`
// 3. `feuilletage_config_value()` returns `feuilletage::ConfigValue` for merging
// 4. Custom FromConfigValue impl handles variant-style parsing
//
// ============================================================================

/// Create a synthetic ConfigContext for deserialized values
fn synthetic_context() -> FeuilletageConfigContext {
    FeuilletageConfigContext::new(FeuilletageConfigSource::Programmatic, FeuilletageConfigLevel::Local)
}

/// Wrapper for storing arbitrary config as feuilletage::Value
#[derive(Debug, Clone)]
pub struct StoredConfig(pub FeuilletageValue);

impl Default for StoredConfig {
    fn default() -> Self {
        StoredConfig(FeuilletageValue::Null)
    }
}

impl StoredConfig {
    pub fn is_empty(&self) -> bool {
        matches!(self.0, FeuilletageValue::Null)
    }

    /// Get the stored feuilletage::Value
    /// This is the new API - callers should migrate to using this instead of to_config_value()
    #[allow(dead_code)]
    pub fn value(&self) -> &FeuilletageValue {
        &self.0
    }

    /// Convert to feuilletage::ConfigValue for merging with feuilletage::Config
    pub fn to_feuilletage_config_value(&self) -> FeuilletageConfigValue {
        value_to_feuilletage_config_value(&self.0, synthetic_context())
    }
}

impl Serialize for StoredConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize the feuilletage::Value directly
        serialize_feuilletage_value(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for StoredConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize as serde_yaml::Value and convert to feuilletage::Value
        let value: serde_yaml::Value = serde_yaml::Value::deserialize(deserializer)?;
        let feuilletage_value = yaml_value_to_feuilletage_value(value);
        Ok(StoredConfig(feuilletage_value))
    }
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel> feuilletage::FromContextValue<S, L>
    for StoredConfig
{
    fn from_context_value(
        value: &feuilletage::ContextValue<S, L>,
        _tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        // Convert ContextValue to Value (stripping context)
        Ok(StoredConfig(FeuilletageValue::from(value)))
    }
}

/// Convert feuilletage::Value to feuilletage::ConfigValue with the given context
fn value_to_feuilletage_config_value(value: &FeuilletageValue, context: FeuilletageConfigContext) -> FeuilletageConfigValue {
    match value {
        FeuilletageValue::Null => FeuilletageConfigValue::null(context),
        FeuilletageValue::Bool(b) => FeuilletageConfigValue::bool(*b, context),
        FeuilletageValue::Int(i) => FeuilletageConfigValue::int(*i, context),
        FeuilletageValue::Float(f) => FeuilletageConfigValue::float(*f, context),
        FeuilletageValue::String(s) => FeuilletageConfigValue::string(s.clone(), context),
        FeuilletageValue::Array(arr) => {
            let items: Vec<FeuilletageConfigValue> = arr
                .iter()
                .map(|v| value_to_feuilletage_config_value(v, context.clone()))
                .collect();
            FeuilletageConfigValue::array(items, context)
        }
        FeuilletageValue::Object(map) => {
            let items: indexmap::IndexMap<String, FeuilletageConfigValue> = map
                .iter()
                .map(|(k, v)| (k.clone(), value_to_feuilletage_config_value(v, context.clone())))
                .collect();
            FeuilletageConfigValue::object(items, context)
        }
    }
}

/// Convert serde_yaml::Value to feuilletage::Value (contextless)
fn yaml_value_to_feuilletage_value(value: serde_yaml::Value) -> FeuilletageValue {
    match value {
        serde_yaml::Value::Null => FeuilletageValue::Null,
        serde_yaml::Value::Bool(b) => FeuilletageValue::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                FeuilletageValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                FeuilletageValue::Float(f)
            } else {
                FeuilletageValue::Null
            }
        }
        serde_yaml::Value::String(s) => FeuilletageValue::String(s),
        serde_yaml::Value::Sequence(arr) => {
            let items: Vec<FeuilletageValue> = arr
                .into_iter()
                .map(yaml_value_to_feuilletage_value)
                .collect();
            FeuilletageValue::Array(items)
        }
        serde_yaml::Value::Mapping(map) => {
            let items: indexmap::IndexMap<String, FeuilletageValue> = map
                .into_iter()
                .filter_map(|(k, v)| {
                    let key = match k {
                        serde_yaml::Value::String(s) => s,
                        _ => return None,
                    };
                    Some((key, yaml_value_to_feuilletage_value(v)))
                })
                .collect();
            FeuilletageValue::Object(items)
        }
        serde_yaml::Value::Tagged(tagged) => yaml_value_to_feuilletage_value(tagged.value),
    }
}

/// Serialize feuilletage::Value using serde
fn serialize_feuilletage_value<S>(value: &FeuilletageValue, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        FeuilletageValue::Null => serializer.serialize_none(),
        FeuilletageValue::Bool(b) => serializer.serialize_bool(*b),
        FeuilletageValue::Int(i) => serializer.serialize_i64(*i),
        FeuilletageValue::Float(f) => serializer.serialize_f64(*f),
        FeuilletageValue::String(s) => serializer.serialize_str(s),
        FeuilletageValue::Array(arr) => {
            use serde::ser::SerializeSeq;
            let mut seq = serializer.serialize_seq(Some(arr.len()))?;
            for item in arr {
                seq.serialize_element(&SerializableFeuilletageValue(item))?;
            }
            seq.end()
        }
        FeuilletageValue::Object(map) => {
            use serde::ser::SerializeMap;
            let mut m = serializer.serialize_map(Some(map.len()))?;
            for (k, v) in map {
                m.serialize_entry(k, &SerializableFeuilletageValue(v))?;
            }
            m.end()
        }
    }
}

/// Helper for serializing feuilletage::Value
struct SerializableFeuilletageValue<'a>(&'a FeuilletageValue);

impl<'a> Serialize for SerializableFeuilletageValue<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_feuilletage_value(self.0, serializer)
    }
}

// ==========================================================================
// CANNOT CONVERT TO DERIVE MACRO - TECHNICAL LIMITATIONS
// ==========================================================================
//
// SuggestConfig requires manual FromContextValue because:
//
// 1. **Variant-key detection pattern**: The struct checks for specific keys
//    (`config`, `template`, `template_file`) to determine which field to
//    populate. If none found, the entire value becomes `config`. This is
//    not a standard enum tagged pattern - it's key-presence detection.
//
// 2. **Custom Serialize behavior**: The Serialize impl serializes `config`
//    directly (not wrapped in a struct field). The derive macro would
//    generate struct-style serialization which would break compatibility.
//
// 3. **Serialize impl conflict**: Since we need custom Serialize, using
//    `#[derive(feuilletage::Config)]` would generate a conflicting Serialize impl.
//
// To convert this, feuilletage would need:
// - A `#[feuilletage(key_presence_variant)]` or similar pattern
// - A way to opt-out of Serialize generation while keeping FromContextValue
// ==========================================================================
#[derive(Debug, Clone)]
pub struct SuggestConfig {
    /// Arbitrary config value, stored as feuilletage::Value
    pub config: StoredConfig,

    pub template: String,

    pub template_file: String,
}

/// Implement FromContextValue manually to handle variant-style parsing
impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel> feuilletage::FromContextValue<S, L>
    for SuggestConfig
{
    fn from_context_value(
        value: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        // Check if it's an object with special keys
        if let feuilletage::ContextValue::Object(map, _) = value {
            // Check for "config" key - use its value directly
            if let Some(config_val) = map.get("config") {
                return Ok(Self {
                    config: StoredConfig(FeuilletageValue::from(config_val)),
                    template: String::new(),
                    template_file: String::new(),
                });
            }

            // Check for "template" key
            if let Some(template_val) = map.get("template") {
                if let feuilletage::ContextValue::String(s, _) = template_val {
                    return Ok(Self {
                        config: StoredConfig::default(),
                        template: s.clone(),
                        template_file: String::new(),
                    });
                }
            }

            // Check for "template_file" key
            if let Some(template_file_val) = map.get("template_file") {
                if let feuilletage::ContextValue::String(s, _) = template_file_val {
                    return Ok(Self {
                        config: StoredConfig::default(),
                        template: String::new(),
                        template_file: s.clone(),
                    });
                }
            }
        }

        // If not a special variant, treat the entire value as config
        let stored_config =
            <StoredConfig as feuilletage::FromContextValue<S, L>>::from_context_value(value, tracker)?;
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

impl feuilletage::IsEmpty for SuggestConfig {
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
    /// Get the config as feuilletage::Value
    /// This is the new API - callers should migrate to using this instead of config_value()
    #[allow(dead_code)]
    pub fn config(&self) -> &FeuilletageValue {
        self.config_in_context(".")
    }

    /// Get the config as feuilletage::Value in the given context
    /// This is the new API - callers should migrate to using this instead of config_value_in_context()
    #[allow(dead_code)]
    pub fn config_in_context(&self, _path: &str) -> &FeuilletageValue {
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

    /// Get the config as feuilletage::ConfigValue for merging with feuilletage::Config
    ///
    /// This is the primary API for getting the suggest config for merging purposes.
    /// It converts the internal feuilletage::Value to a feuilletage::ConfigValue with synthetic context.
    ///
    /// Note: Template rendering is not yet supported via this method.
    pub fn feuilletage_config_value(&self) -> FeuilletageConfigValue {
        // For now, we only support direct config (no template rendering)
        // TODO: Add template rendering support for feuilletage path
        self.config.to_feuilletage_config_value()
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
                let feuilletage_value = yaml_value_to_feuilletage_value(config_val.clone());
                return Ok(SuggestConfig {
                    config: StoredConfig(feuilletage_value),
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
        let feuilletage_value = yaml_value_to_feuilletage_value(value);
        Ok(SuggestConfig {
            config: StoredConfig(feuilletage_value),
            template: String::new(),
            template_file: String::new(),
        })
    }
}
