use std::collections::HashMap;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use tera::Context;
use tera::Tera;

use crate::internal::cache::utils::Empty;
use crate::internal::config::template::config_template_context;
use crate::internal::config::template::render_config_template;
use crate::internal::config::template::tera_render_error_message;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_warning;

// Compote imports
use compote::ConfigError as CompoteConfigError;
use compote::Context as CompoteConfigContext;
use compote::ContextValue as CompoteConfigValue;
use compote::ErrorTracker as CompoteErrorTracker;
use compote::FromContextValue as CompoteFromConfigValue;
use compote::Level as CompoteConfigLevel;
use compote::Source as CompoteConfigSource;

/// Create a synthetic ConfigContext for deserialized values (from templates)
fn synthetic_context() -> CompoteConfigContext {
    CompoteConfigContext::new(CompoteConfigSource::Programmatic, CompoteConfigLevel::Local)
}

/// Convert serde_yaml::Value to compote::ContextValue
fn yaml_value_to_compote_config_value(value: serde_yaml::Value) -> CompoteConfigValue {
    let ctx = synthetic_context();
    match value {
        serde_yaml::Value::Null => CompoteConfigValue::null(ctx),
        serde_yaml::Value::Bool(b) => CompoteConfigValue::bool(b, ctx),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                CompoteConfigValue::int(i, ctx)
            } else if let Some(f) = n.as_f64() {
                CompoteConfigValue::float(f, ctx)
            } else {
                CompoteConfigValue::null(ctx)
            }
        }
        serde_yaml::Value::String(s) => CompoteConfigValue::string(s, ctx),
        serde_yaml::Value::Sequence(arr) => {
            let items: Vec<CompoteConfigValue> = arr
                .into_iter()
                .map(yaml_value_to_compote_config_value)
                .collect();
            CompoteConfigValue::array(items, ctx)
        }
        serde_yaml::Value::Mapping(map) => {
            let items: indexmap::IndexMap<String, CompoteConfigValue> = map
                .into_iter()
                .filter_map(|(k, v)| {
                    let key = match k {
                        serde_yaml::Value::String(s) => s,
                        _ => return None,
                    };
                    Some((key, yaml_value_to_compote_config_value(v)))
                })
                .collect();
            CompoteConfigValue::object(items, ctx)
        }
        serde_yaml::Value::Tagged(tagged) => yaml_value_to_compote_config_value(tagged.value),
    }
}

#[derive(Default, Debug, Deserialize, Clone)]
pub struct SuggestCloneConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    repositories: Vec<SuggestCloneRepositoryConfig>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub template: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub template_file: String,
}

impl Empty for SuggestCloneConfig {
    fn is_empty(&self) -> bool {
        self.repositories.is_empty() && self.template.is_empty() && self.template_file.is_empty()
    }
}

impl compote::IsEmpty for SuggestCloneConfig {
    fn is_empty(&self) -> bool {
        Empty::is_empty(self)
    }
}

impl Serialize for SuggestCloneConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if !self.repositories.is_empty() {
            self.repositories.serialize(serializer)
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

impl SuggestCloneConfig {
    pub fn repositories(&self, quiet: bool) -> Vec<SuggestCloneRepositoryConfig> {
        self.repositories_in_context(".", quiet)
    }

    pub fn repositories_in_context(
        &self,
        path: &str,
        quiet: bool,
    ) -> Vec<SuggestCloneRepositoryConfig> {
        let context = config_template_context(path);
        self.repositories_with_context(&context, quiet)
    }

    fn repositories_with_context(
        &self,
        template_context: &Context,
        quiet: bool,
    ) -> Vec<SuggestCloneRepositoryConfig> {
        if !self.repositories.is_empty() {
            return self.repositories.clone();
        }

        let mut template = Tera::default();
        if !self.template.is_empty() {
            if let Err(err) = template.add_raw_template("suggest_clone", &self.template) {
                if !quiet {
                    omni_warning!(tera_render_error_message(err));
                    omni_warning!("suggest_clone will be ignored");
                }
                return vec![];
            }
        } else if !self.template_file.is_empty() {
            if let Err(err) = template.add_template_file(&self.template_file, None) {
                if !quiet {
                    omni_warning!(tera_render_error_message(err));
                    omni_warning!("suggest_clone will be ignored");
                }
                return vec![];
            }
        }

        if !template.templates.is_empty() {
            match render_config_template(&template, template_context) {
                Ok(yaml_str) => {
                    // Parse YAML string using compote
                    match serde_yaml::from_str::<serde_yaml::Value>(&yaml_str) {
                        Ok(yaml_value) => {
                            // Convert to compote::ContextValue and deserialize
                            let config_value = yaml_value_to_compote_config_value(yaml_value);
                            let mut tracker = CompoteErrorTracker::new();
                            match Self::from_config_value(&config_value, &mut tracker) {
                                Ok(suggest_clone) => {
                                    // In case this is recursive for some reason...
                                    return suggest_clone
                                        .repositories_with_context(template_context, quiet);
                                }
                                Err(err) => {
                                    if !quiet {
                                        omni_warning!(format!(
                                            "Failed to parse suggest_clone template: {}",
                                            err
                                        ));
                                        omni_warning!("suggest_clone will be ignored");
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            if !quiet {
                                omni_warning!(format!(
                                    "Failed to parse suggest_clone template: {}",
                                    err
                                ));
                                omni_warning!("suggest_clone will be ignored");
                            }
                        }
                    }
                }
                Err(err) => {
                    if !quiet {
                        omni_warning!(tera_render_error_message(err));
                        omni_warning!("suggest_clone will be ignored");
                    }
                }
            }
        }

        vec![]
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum SuggestCloneTypeEnum {
    #[serde(rename = "package")]
    Package,
    #[serde(rename = "worktree")]
    Worktree,
}

impl FromStr for SuggestCloneTypeEnum {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "package" => Ok(Self::Package),
            "worktree" => Ok(Self::Worktree),
            _ => Err(format!("Invalid: {s}")),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SuggestCloneRepositoryConfig {
    pub handle: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    pub clone_type: SuggestCloneTypeEnum,
}

impl SuggestCloneRepositoryConfig {
    pub fn clone_as_package(&self) -> bool {
        self.clone_type == SuggestCloneTypeEnum::Package
    }
}

// ============================================================================
// Compote Native Implementation
// ============================================================================

/// Helper to select only Local (Workdir) scope values
/// Returns None if the entire value should be rejected (not from Local scope)
fn select_local_scope(value: &CompoteConfigValue) -> Option<CompoteConfigValue> {
    match value {
        CompoteConfigValue::Object(map, ctx) => {
            let filtered: indexmap::IndexMap<String, CompoteConfigValue> = map
                .iter()
                .filter_map(|(k, v)| select_local_scope(v).map(|filtered| (k.clone(), filtered)))
                .collect();
            if filtered.is_empty() {
                None
            } else {
                Some(CompoteConfigValue::object(filtered, ctx.clone()))
            }
        }
        CompoteConfigValue::Array(arr, ctx) => {
            let filtered: Vec<CompoteConfigValue> = arr
                .iter()
                .filter_map(select_local_scope)
                .collect();
            if filtered.is_empty() {
                None
            } else {
                Some(CompoteConfigValue::array(filtered, ctx.clone()))
            }
        }
        _ => {
            // For scalar values, only keep if from Local scope
            if matches!(value.context().level, CompoteConfigLevel::Local) {
                Some(value.clone())
            } else {
                None
            }
        }
    }
}

impl CompoteFromConfigValue for SuggestCloneTypeEnum {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteConfigError> {
        let s = String::from_config_value(value, tracker)?;
        Self::from_str(&s).map_err(|_| CompoteConfigError::InvalidValue {
            message: format!("Invalid clone type '{}', expected 'package' or 'worktree'", s),
            path: tracker.current_path(),
        })
    }
}

impl CompoteFromConfigValue for SuggestCloneRepositoryConfig {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteConfigError> {
        // Can be a simple string (handle only) or a table
        match value {
            CompoteConfigValue::String(s, _) => Ok(Self {
                handle: s.clone(),
                args: vec![],
                clone_type: SuggestCloneTypeEnum::Package,
            }),
            CompoteConfigValue::Object(table, _) => {
                // handle is required
                let handle = if let Some(v) = table.get("handle") {
                    tracker.push_field("handle");
                    let result = String::from_config_value(v, tracker)?;
                    tracker.pop();
                    result
                } else {
                    tracker.push_field("handle");
                    let path = tracker.current_path();
                    tracker.pop();
                    return Err(CompoteConfigError::MissingField { path });
                };

                // args is optional, parse shell words from string
                let args = if let Some(v) = table.get("args") {
                    tracker.push_field("args");
                    let args_str = String::from_config_value(v, tracker)?;
                    tracker.pop();
                    shell_words::split(&args_str).unwrap_or_default()
                } else {
                    vec![]
                };

                // clone_type is optional, defaults to Package
                let clone_type = if let Some(v) = table.get("clone_type") {
                    tracker.push_field("clone_type");
                    let result =
                        <SuggestCloneTypeEnum as CompoteFromConfigValue>::from_config_value(
                            v, tracker,
                        )?;
                    tracker.pop();
                    result
                } else {
                    SuggestCloneTypeEnum::Package
                };

                Ok(Self {
                    handle,
                    args,
                    clone_type,
                })
            }
            _ => Err(CompoteConfigError::TypeMismatch {
                expected: "string or table".to_string(),
                actual: value.type_name().to_string(),
                path: tracker.current_path(),
            }),
        }
    }
}

impl CompoteFromConfigValue for SuggestCloneConfig {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteConfigError> {
        // This config only accepts Local (Workdir) scope values
        let filtered = match select_local_scope(value) {
            Some(v) => v,
            None => return Ok(Self::default()),
        };

        match &filtered {
            CompoteConfigValue::Null(_) => Ok(Self::default()),
            // Array format: list of repository configs
            CompoteConfigValue::Array(arr, _) => {
                let mut repositories = Vec::new();
                for (idx, v) in arr.iter().enumerate() {
                    tracker.push_index(idx);
                    match <SuggestCloneRepositoryConfig as CompoteFromConfigValue>::from_config_value(v, tracker) {
                        Ok(repo) => repositories.push(repo),
                        Err(e) => {
                            tracker.record(e);
                        }
                    }
                    tracker.pop();
                }
                Ok(Self {
                    repositories,
                    ..Default::default()
                })
            }
            // Table format: can have repositories, template, or template_file
            CompoteConfigValue::Object(table, _) => {
                // Check for repositories array
                if let Some(v) = table.get("repositories") {
                    if let CompoteConfigValue::Array(arr, _) = v {
                        let mut repositories = Vec::new();
                        for (idx, repo_v) in arr.iter().enumerate() {
                            tracker.push_field("repositories");
                            tracker.push_index(idx);
                            match <SuggestCloneRepositoryConfig as CompoteFromConfigValue>::from_config_value(repo_v, tracker) {
                                Ok(repo) => repositories.push(repo),
                                Err(e) => {
                                    tracker.record(e);
                                }
                            }
                            tracker.pop();
                            tracker.pop();
                        }
                        return Ok(Self {
                            repositories,
                            ..Default::default()
                        });
                    }
                }

                // Check for template
                if let Some(v) = table.get("template") {
                    tracker.push_field("template");
                    let template = String::from_config_value(v, tracker)?;
                    tracker.pop();
                    return Ok(Self {
                        template,
                        ..Default::default()
                    });
                }

                // Check for template_file
                if let Some(v) = table.get("template_file") {
                    tracker.push_field("template_file");
                    let template_file = String::from_config_value(v, tracker)?;
                    tracker.pop();
                    return Ok(Self {
                        template_file,
                        ..Default::default()
                    });
                }

                Ok(Self::default())
            }
            _ => Err(CompoteConfigError::TypeMismatch {
                expected: "array or table".to_string(),
                actual: filtered.type_name().to_string(),
                path: tracker.current_path(),
            }),
        }
    }
}
