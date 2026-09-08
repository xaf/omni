use std::collections::HashMap;

use serde::Deserialize;
use serde::Serialize;
use tera::Tera;

use crate::internal::cache::utils::Empty;
use crate::internal::config::template::config_template_context;
use crate::internal::config::template::render_config_template;
use crate::internal::config::template::tera_render_error_message;
use crate::internal::config::FeuilletageConfigContext;
use crate::internal::config::FeuilletageConfigLevel;
use crate::internal::config::FeuilletageConfigSource;
use crate::internal::config::FeuilletageConfigValue;
use crate::internal::config::Value as FeuilletageValue;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_warning;

fn synthetic_context() -> FeuilletageConfigContext {
    FeuilletageConfigContext::new(
        FeuilletageConfigSource::Programmatic,
        FeuilletageConfigLevel::Local,
    )
}

pub(super) fn select_local_scope<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &feuilletage::ContextValue<S, L>,
) -> Option<feuilletage::ContextValue<S, L>> {
    match value {
        feuilletage::ContextValue::Object(values, context) => {
            let values: indexmap::IndexMap<_, _> = values
                .iter()
                .filter_map(|(key, value)| {
                    select_local_scope(value).map(|value| (key.clone(), value))
                })
                .collect();
            (!values.is_empty()).then(|| feuilletage::ContextValue::object(values, context.clone()))
        }
        feuilletage::ContextValue::Array(values, context) => {
            let values: Vec<_> = values.iter().filter_map(select_local_scope).collect();
            (!values.is_empty()).then(|| feuilletage::ContextValue::array(values, context.clone()))
        }
        _ if value.context().level.name() == "local" => Some(value.clone()),
        _ => None,
    }
}

#[derive(Debug, Clone, feuilletage::Config)]
#[feuilletage(parse_as = "feuilletage::Value", skip_serialize, skip_deserialize)]
pub struct StoredConfig {
    value: FeuilletageValue,
    contextual_value: FeuilletageConfigValue,
}

impl Default for StoredConfig {
    fn default() -> Self {
        Self::from_value(FeuilletageValue::Null)
    }
}

impl StoredConfig {
    fn from_value(value: FeuilletageValue) -> Self {
        let contextual_value = value_to_feuilletage_config_value(&value, synthetic_context());
        Self {
            value,
            contextual_value,
        }
    }

    fn from_context_value<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
        value: &feuilletage::ContextValue<S, L>,
    ) -> Self {
        Self {
            value: FeuilletageValue::from(value),
            contextual_value: clone_context_value(value),
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self.value, FeuilletageValue::Null)
            || matches!(&self.value, FeuilletageValue::Object(values) if values.is_empty())
    }

    fn is_present(&self) -> bool {
        !matches!(self.value, FeuilletageValue::Null)
    }

    #[allow(dead_code)]
    pub fn value(&self) -> &FeuilletageValue {
        &self.value
    }

    pub fn to_feuilletage_config_value(&self) -> FeuilletageConfigValue {
        self.contextual_value.clone()
    }
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>
    feuilletage::FromParsed<feuilletage::Value, S, L> for StoredConfig
{
    fn from_parsed(
        _parsed: feuilletage::Value,
        original: &feuilletage::ContextValue<S, L>,
        _tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        Ok(Self::from_context_value(original))
    }
}

impl Serialize for StoredConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_feuilletage_value(&self.value, serializer)
    }
}

impl<'de> Deserialize<'de> for StoredConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;
        Ok(Self::from_value(yaml_value_to_feuilletage_value(value)))
    }
}

fn clone_context<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    context: &feuilletage::Context<S, L>,
) -> FeuilletageConfigContext {
    let source = context
        .source
        .file_path()
        .map(|path| FeuilletageConfigSource::File(path.to_path_buf()))
        .unwrap_or_else(|| match context.source.display_name().as_str() {
            "environment" => FeuilletageConfigSource::Environment,
            "default" => FeuilletageConfigSource::Default,
            _ => FeuilletageConfigSource::Programmatic,
        });
    let level = match context.level.name() {
        "system" => FeuilletageConfigLevel::System,
        "user" => FeuilletageConfigLevel::User,
        _ => FeuilletageConfigLevel::Local,
    };

    FeuilletageConfigContext {
        source,
        format: context.format.clone(),
        level,
        mutability: context.mutability.clone(),
    }
}

fn clone_context_value<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &feuilletage::ContextValue<S, L>,
) -> FeuilletageConfigValue {
    let context = clone_context(value.context());
    match value {
        feuilletage::ContextValue::Null(_) => FeuilletageConfigValue::null(context),
        feuilletage::ContextValue::Bool(value, _) => FeuilletageConfigValue::bool(*value, context),
        feuilletage::ContextValue::Int(value, _) => FeuilletageConfigValue::int(*value, context),
        feuilletage::ContextValue::Float(value, _) => {
            FeuilletageConfigValue::float(*value, context)
        }
        feuilletage::ContextValue::String(value, _) => {
            FeuilletageConfigValue::string(value.clone(), context)
        }
        feuilletage::ContextValue::Array(values, _) => {
            FeuilletageConfigValue::array(values.iter().map(clone_context_value).collect(), context)
        }
        feuilletage::ContextValue::Object(values, _) => FeuilletageConfigValue::object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), clone_context_value(value)))
                .collect(),
            context,
        ),
    }
}

fn value_to_feuilletage_config_value(
    value: &FeuilletageValue,
    context: FeuilletageConfigContext,
) -> FeuilletageConfigValue {
    match value {
        FeuilletageValue::Null => FeuilletageConfigValue::null(context),
        FeuilletageValue::Bool(value) => FeuilletageConfigValue::bool(*value, context),
        FeuilletageValue::Int(value) => FeuilletageConfigValue::int(*value, context),
        FeuilletageValue::Float(value) => FeuilletageConfigValue::float(*value, context),
        FeuilletageValue::String(value) => FeuilletageConfigValue::string(value.clone(), context),
        FeuilletageValue::Array(values) => FeuilletageConfigValue::array(
            values
                .iter()
                .map(|value| value_to_feuilletage_config_value(value, context.clone()))
                .collect(),
            context,
        ),
        FeuilletageValue::Object(values) => FeuilletageConfigValue::object(
            values
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        value_to_feuilletage_config_value(value, context.clone()),
                    )
                })
                .collect(),
            context,
        ),
    }
}

fn yaml_value_to_feuilletage_value(value: serde_yaml::Value) -> FeuilletageValue {
    match value {
        serde_yaml::Value::Null => FeuilletageValue::Null,
        serde_yaml::Value::Bool(value) => FeuilletageValue::Bool(value),
        serde_yaml::Value::Number(value) => value
            .as_i64()
            .map(FeuilletageValue::Int)
            .or_else(|| value.as_f64().map(FeuilletageValue::Float))
            .unwrap_or(FeuilletageValue::Null),
        serde_yaml::Value::String(value) => FeuilletageValue::String(value),
        serde_yaml::Value::Sequence(values) => FeuilletageValue::Array(
            values
                .into_iter()
                .map(yaml_value_to_feuilletage_value)
                .collect(),
        ),
        serde_yaml::Value::Mapping(values) => FeuilletageValue::Object(
            values
                .into_iter()
                .filter_map(|(key, value)| match key {
                    serde_yaml::Value::String(key) => {
                        Some((key, yaml_value_to_feuilletage_value(value)))
                    }
                    _ => None,
                })
                .collect(),
        ),
        serde_yaml::Value::Tagged(value) => yaml_value_to_feuilletage_value(value.value),
    }
}

fn serialize_feuilletage_value<S>(
    value: &FeuilletageValue,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        FeuilletageValue::Null => serializer.serialize_none(),
        FeuilletageValue::Bool(value) => serializer.serialize_bool(*value),
        FeuilletageValue::Int(value) => serializer.serialize_i64(*value),
        FeuilletageValue::Float(value) => serializer.serialize_f64(*value),
        FeuilletageValue::String(value) => serializer.serialize_str(value),
        FeuilletageValue::Array(values) => {
            use serde::ser::SerializeSeq;
            let mut sequence = serializer.serialize_seq(Some(values.len()))?;
            for value in values {
                sequence.serialize_element(&SerializableFeuilletageValue(value))?;
            }
            sequence.end()
        }
        FeuilletageValue::Object(values) => {
            use serde::ser::SerializeMap;
            let mut map = serializer.serialize_map(Some(values.len()))?;
            for (key, value) in values {
                map.serialize_entry(key, &SerializableFeuilletageValue(value))?;
            }
            map.end()
        }
    }
}

struct SerializableFeuilletageValue<'a>(&'a FeuilletageValue);

impl Serialize for SerializableFeuilletageValue<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_feuilletage_value(self.0, serializer)
    }
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(untagged)]
enum SuggestConfigWire {
    Config {
        config: feuilletage::Value,
    },
    Template {
        template: String,
    },
    TemplateFile {
        #[feuilletage(relative_path)]
        template_file: String,
    },
    #[feuilletage(fallback)]
    ConfigValue(feuilletage::Value),
}

#[derive(Debug, Clone, feuilletage::Config)]
#[feuilletage(
    parse_as = "SuggestConfigWire",
    skip_serialize,
    skip_deserialize
)]
#[derive(Default)]
pub struct SuggestConfig {
    pub config: StoredConfig,
    pub template: String,
    pub template_file: String,
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>
    feuilletage::FromParsed<SuggestConfigWire, S, L> for SuggestConfig
{
    fn from_parsed(
        _parsed: SuggestConfigWire,
        original: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        let Some(original) = select_local_scope(original) else {
            return Ok(Self::default());
        };
        let parsed =
            <SuggestConfigWire as feuilletage::FromContextValue<S, L>>::from_context_value(
                &original, tracker,
            )?;

        match parsed {
            SuggestConfigWire::Config { .. } => {
                let config = original
                    .as_object()
                    .and_then(|values| values.get("config"))
                    .expect("config wire variant requires a config field");
                Ok(Self {
                    config: StoredConfig::from_context_value(config),
                    ..Default::default()
                })
            }
            SuggestConfigWire::Template { template } => Ok(Self {
                template,
                ..Default::default()
            }),
            SuggestConfigWire::TemplateFile { template_file } => Ok(Self {
                template_file,
                ..Default::default()
            }),
            SuggestConfigWire::ConfigValue(_) => Ok(Self {
                config: StoredConfig::from_context_value(&original),
                ..Default::default()
            }),
        }
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

impl Serialize for SuggestConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if self.config.is_present() {
            self.config.serialize(serializer)
        } else if !self.template.is_empty() || !self.template_file.is_empty() {
            let mut map = HashMap::new();
            if !self.template.is_empty() {
                map.insert("template".to_string(), self.template.clone());
            } else {
                map.insert("template_file".to_string(), self.template_file.clone());
            }
            map.serialize(serializer)
        } else {
            serializer.serialize_none()
        }
    }
}

impl SuggestConfig {
    #[allow(dead_code)]
    pub fn config(&self) -> FeuilletageValue {
        self.config_in_context(".")
    }

    #[allow(dead_code)]
    pub fn config_in_context(&self, path: &str) -> FeuilletageValue {
        FeuilletageValue::from(&self.feuilletage_config_value_in_context(path))
    }

    pub fn feuilletage_config_value(&self) -> FeuilletageConfigValue {
        self.feuilletage_config_value_in_context(".")
    }

    fn feuilletage_config_value_in_context(&self, path: &str) -> FeuilletageConfigValue {
        if !self.config.is_empty() {
            return self.config.to_feuilletage_config_value();
        }

        let context = config_template_context(path);
        self.feuilletage_config_value_with_context(&context)
    }

    fn feuilletage_config_value_with_context(
        &self,
        template_context: &tera::Context,
    ) -> FeuilletageConfigValue {
        if !self.config.is_empty() {
            return self.config.to_feuilletage_config_value();
        }

        let mut template = Tera::default();
        if !self.template.is_empty() {
            if let Err(error) = template.add_raw_template("suggest_config", &self.template) {
                omni_warning!(tera_render_error_message(error));
                omni_warning!("suggest_config will be ignored");
                return FeuilletageConfigValue::null(synthetic_context());
            }
        } else if !self.template_file.is_empty() {
            if let Err(error) = template.add_template_file(&self.template_file, None) {
                omni_warning!(tera_render_error_message(error));
                omni_warning!("suggest_config will be ignored");
                return FeuilletageConfigValue::null(synthetic_context());
            }
        }

        if template.get_template_names().next().is_some() {
            match render_config_template(&template, template_context) {
                Ok(rendered) => {
                    match feuilletage::loader::load_yaml(&rendered, synthetic_context()) {
                        Ok(value) => {
                            let mut tracker = feuilletage::ErrorTracker::new();
                            match <Self as feuilletage::FromContextValue>::from_context_value(
                                &value,
                                &mut tracker,
                            ) {
                                Ok(suggest) => {
                                    return suggest
                                        .feuilletage_config_value_with_context(template_context);
                                }
                                Err(error) => {
                                    omni_warning!(format!(
                                        "Failed to parse suggest_config template: {error}"
                                    ));
                                }
                            }
                        }
                        Err(error) => {
                            omni_warning!(format!(
                                "Failed to parse suggest_config template: {error}"
                            ));
                        }
                    }
                }
                Err(error) => {
                    omni_warning!(tera_render_error_message(error));
                }
            }
            omni_warning!("suggest_config will be ignored");
        }

        FeuilletageConfigValue::null(synthetic_context())
    }
}


impl<'de> Deserialize<'de> for SuggestConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;

        if let serde_yaml::Value::Mapping(ref values) = value {
            if let Some(config) = values.get(serde_yaml::Value::String("config".to_string())) {
                return Ok(Self {
                    config: StoredConfig::from_value(yaml_value_to_feuilletage_value(
                        config.clone(),
                    )),
                    ..Default::default()
                });
            }
            if let Some(serde_yaml::Value::String(template)) =
                values.get(serde_yaml::Value::String("template".to_string()))
            {
                return Ok(Self {
                    template: template.clone(),
                    ..Default::default()
                });
            }
            if let Some(serde_yaml::Value::String(template_file)) =
                values.get(serde_yaml::Value::String("template_file".to_string()))
            {
                return Ok(Self {
                    template_file: template_file.clone(),
                    ..Default::default()
                });
            }
        }

        Ok(Self {
            config: StoredConfig::from_value(yaml_value_to_feuilletage_value(value)),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn parse(
        yaml: &str,
        context: FeuilletageConfigContext,
    ) -> (SuggestConfig, feuilletage::ErrorTracker) {
        let value = feuilletage::loader::load_yaml(yaml, context).unwrap();
        let mut tracker = feuilletage::ErrorTracker::new();
        let config = <SuggestConfig as feuilletage::FromContextValue>::from_context_value(
            &value,
            &mut tracker,
        )
        .unwrap();
        (config, tracker)
    }

    fn local_context() -> FeuilletageConfigContext {
        FeuilletageConfigContext::new(
            FeuilletageConfigSource::Programmatic,
            FeuilletageConfigLevel::Local,
        )
    }

    #[test]
    fn parses_direct_and_explicit_config_forms() {
        let (direct, direct_errors) = parse("commands:\n  hello: echo hi\n", local_context());
        let (explicit, explicit_errors) = parse(
            "config:\n  commands:\n    hello: echo hi\ntemplate: ignored\n",
            local_context(),
        );

        assert!(direct_errors.errors().is_empty());
        assert!(explicit_errors.errors().is_empty());
        assert_eq!(direct.config.value(), explicit.config.value());
        assert!(direct.template.is_empty());
        assert!(explicit.template.is_empty());
    }

    #[test]
    fn renders_inline_template_to_config() {
        let (config, tracker) = parse(
            "template: |\n  commands:\n    hello: echo hi\n",
            local_context(),
        );
        assert!(tracker.errors().is_empty());

        let rendered = FeuilletageValue::from(
            &config.feuilletage_config_value_with_context(&tera::Context::new()),
        );
        let commands = rendered
            .as_object()
            .and_then(|value| value.get("commands"))
            .and_then(FeuilletageValue::as_object)
            .unwrap();
        assert_eq!(
            commands.get("hello").and_then(FeuilletageValue::as_str),
            Some("echo hi")
        );
    }

    #[test]
    fn resolves_template_file_relative_to_source_and_renders_it() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("omni.yaml");
        let template_path = directory.path().join("suggest.yaml");
        fs::write(&template_path, "commands:\n  hello: echo hi\n").unwrap();
        let context =
            FeuilletageConfigContext::new_from_file(config_path, FeuilletageConfigLevel::Local);

        let (config, tracker) = parse("template_file: suggest.yaml\n", context);

        assert!(tracker.errors().is_empty());
        assert_eq!(config.template_file, template_path.to_string_lossy());
        assert!(config
            .feuilletage_config_value_with_context(&tera::Context::new())
            .as_object()
            .unwrap()
            .contains_key("commands"));
    }

    #[test]
    fn preserves_source_context_for_explicit_config_values() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("omni.yaml");
        let context = FeuilletageConfigContext::new_from_file(
            config_path.clone(),
            FeuilletageConfigLevel::Local,
        );
        let (config, tracker) = parse("config:\n  feature: enabled\n", context);
        assert!(tracker.errors().is_empty());

        let value = config.feuilletage_config_value();
        let feature = value.as_object().unwrap().get("feature").unwrap();
        assert_eq!(
            feature.context().source,
            FeuilletageConfigSource::File(config_path)
        );
        assert_eq!(feature.context().level, FeuilletageConfigLevel::Local);
    }

    #[test]
    fn serializes_using_original_shorthand_shapes() {
        let direct: SuggestConfig = serde_yaml::from_str("commands:\n  hello: echo hi\n").unwrap();
        let template: SuggestConfig = serde_yaml::from_str("template: hello\n").unwrap();
        let empty_object: SuggestConfig = serde_yaml::from_str("{}\n").unwrap();

        assert!(Empty::is_empty(&empty_object));

        let direct = serde_yaml::to_value(direct).unwrap();
        let template = serde_yaml::to_value(template).unwrap();
        let empty_object = serde_yaml::to_value(empty_object).unwrap();
        assert!(direct.get("commands").is_some());
        assert_eq!(
            template.get("template").and_then(serde_yaml::Value::as_str),
            Some("hello")
        );
        assert!(empty_object.as_mapping().unwrap().is_empty());
    }

    #[test]
    fn direct_deserialization_ignores_non_local_configuration() {
        let (config, tracker) = parse(
            "commands:\n  hello: echo hi\n",
            FeuilletageConfigContext::new(
                FeuilletageConfigSource::Programmatic,
                FeuilletageConfigLevel::User,
            ),
        );

        assert!(tracker.errors().is_empty());
        assert!(Empty::is_empty(&config));
    }

    #[test]
    fn direct_deserialization_filters_mixed_provenance_recursively() {
        let user = FeuilletageConfigContext::new(
            FeuilletageConfigSource::Programmatic,
            FeuilletageConfigLevel::User,
        );
        let local = FeuilletageConfigContext::new(
            FeuilletageConfigSource::Programmatic,
            FeuilletageConfigLevel::Local,
        );
        let commands = feuilletage::ContextValue::object(
            [
                (
                    "user".to_string(),
                    feuilletage::ContextValue::string("ignored".to_string(), user.clone()),
                ),
                (
                    "local".to_string(),
                    feuilletage::ContextValue::string("kept".to_string(), local.clone()),
                ),
            ]
            .into_iter()
            .collect(),
            user.clone(),
        );
        let value = feuilletage::ContextValue::object(
            [("commands".to_string(), commands)].into_iter().collect(),
            user,
        );
        let mut tracker = feuilletage::ErrorTracker::new();

        let config = <SuggestConfig as feuilletage::FromContextValue>::from_context_value(
            &value,
            &mut tracker,
        )
        .unwrap();

        assert!(tracker.errors().is_empty());
        let value = config.feuilletage_config_value();
        let commands = value
            .as_object()
            .unwrap()
            .get("commands")
            .unwrap()
            .as_object()
            .unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands.get("local").unwrap().as_str(), Some("kept"));
    }
}
