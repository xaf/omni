use std::collections::HashMap;
use std::str::FromStr;

use serde::Serialize;
use tera::Context;
use tera::Tera;

use crate::internal::cache::utils::Empty;
use crate::internal::config::template::config_template_context;
use crate::internal::config::template::register_partial_resolve_placeholder;
use crate::internal::config::template::render_config_template;
use crate::internal::config::template::tera_render_error_message;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_warning;

// Compote imports
use crate::internal::config::CompoteConfigContext;
use crate::internal::config::CompoteConfigValue;
use crate::internal::config::CompoteErrorTracker;
use crate::internal::config::CompoteConfigLevel;
use crate::internal::config::CompoteConfigSource;

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

#[derive(Default, Debug, Clone)]
pub struct SuggestCloneConfig {
    repositories: Vec<SuggestCloneRepositoryConfig>,
    pub template: String,
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
        register_partial_resolve_placeholder(&mut template);
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

        if template.get_template_names().next().is_some() {
            match render_config_template(&template, template_context) {
                Ok(yaml_str) => {
                    // Parse YAML string using compote
                    match serde_yaml::from_str::<serde_yaml::Value>(&yaml_str) {
                        Ok(yaml_value) => {
                            // Convert to compote::ContextValue and deserialize
                            let config_value = yaml_value_to_compote_config_value(yaml_value);
                            let mut tracker = CompoteErrorTracker::new();
                            match <Self as compote::FromContextValue<_, _>>::from_context_value(&config_value, &mut tracker) {
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

#[derive(Debug, Clone, PartialEq, compote::Config)]
#[compote(value_matched)]
pub enum SuggestCloneTypeEnum {
    #[compote(variant = "package")]
    Package,
    #[compote(variant = "worktree")]
    Worktree,
}

impl Default for SuggestCloneTypeEnum {
    fn default() -> Self {
        Self::Package
    }
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

/// Transform function that converts a String ContextValue into an Array ContextValue
/// using shell_words::split(), enabling Vec<String> deserialization from a shell command string.
fn shell_words_transform<S: compote::CustomSource, L: compote::CustomLevel>(
    value: &mut compote::ContextValue<S, L>,
    _context: &compote::Context<S, L>,
) -> Result<(), compote::Error> {
    if let compote::ContextValue::String(s, ctx) = value {
        let words = shell_words::split(s).unwrap_or_default();
        let arr = words
            .into_iter()
            .map(|w| compote::ContextValue::string(w, ctx.clone()))
            .collect();
        *value = compote::ContextValue::array(arr, ctx.clone());
    }
    Ok(())
}

#[derive(Debug, Serialize, Clone, compote::Config)]
#[compote(scalar_as = "handle", skip_serialize)]
pub struct SuggestCloneRepositoryConfig {
    pub handle: String,
    #[compote(default, transform = "crate::internal::config::parser::suggest_clone::shell_words_transform")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[compote(default)]
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
fn select_local_scope<S: compote::CustomSource, L: compote::CustomLevel>(
    value: &compote::ContextValue<S, L>,
) -> Option<compote::ContextValue<S, L>> {
    match value {
        compote::ContextValue::Object(map, ctx) => {
            let filtered: indexmap::IndexMap<String, compote::ContextValue<S, L>> = map
                .iter()
                .filter_map(|(k, v)| select_local_scope(v).map(|filtered| (k.clone(), filtered)))
                .collect();
            if filtered.is_empty() {
                None
            } else {
                Some(compote::ContextValue::object(filtered, ctx.clone()))
            }
        }
        compote::ContextValue::Array(arr, ctx) => {
            let filtered: Vec<compote::ContextValue<S, L>> = arr
                .iter()
                .filter_map(select_local_scope)
                .collect();
            if filtered.is_empty() {
                None
            } else {
                Some(compote::ContextValue::array(filtered, ctx.clone()))
            }
        }
        _ => {
            // For scalar values, only keep if from Local scope
            if value.context().level.name() == "local" {
                Some(value.clone())
            } else {
                None
            }
        }
    }
}

// Manual impl replaced by derive macro:
// impl<S: compote::CustomSource, L: compote::CustomLevel> compote::FromContextValue<S, L>
//     for SuggestCloneTypeEnum
// {
//     fn from_context_value(
//         value: &compote::ContextValue<S, L>,
//         tracker: &mut compote::ErrorTracker,
//     ) -> Result<Self, compote::Error> {
//         let s = String::from_context_value(value, tracker)?;
//         Self::from_str(&s).map_err(|_| compote::Error::InvalidValue {
//             message: format!("Invalid clone type '{}', expected 'package' or 'worktree'", s),
//             path: tracker.current_path(),
//         })
//     }
// }

// ==========================================================================
// CANNOT CONVERT TO DERIVE MACRO - TECHNICAL LIMITATION
// ==========================================================================
//
// SuggestCloneConfig requires manual FromContextValue because:
//
// 1. **Level-based filtering**: The config only accepts values from Local
//    (Workdir) scope. It uses `select_local_scope()` to filter out values
//    from other levels. Compote doesn't support level-based value filtering.
//
// 2. **Multi-format input**: Accepts array, object with repositories/template/
//    template_file keys, or returns default. This polymorphic parsing pattern
//    goes beyond what derive macros can express.
//
// To convert this, compote would need:
// - A `#[compote(filter_by_level = "local")]` attribute
// - Or a way to specify pre-processing filters on the input value
// ==========================================================================
impl<S: compote::CustomSource, L: compote::CustomLevel> compote::FromContextValue<S, L>
    for SuggestCloneConfig
{
    fn from_context_value(
        value: &compote::ContextValue<S, L>,
        tracker: &mut compote::ErrorTracker,
    ) -> Result<Self, compote::Error> {
        // This config only accepts Local (Workdir) scope values
        let filtered = match select_local_scope(value) {
            Some(v) => v,
            None => return Ok(Self::default()),
        };

        match &filtered {
            compote::ContextValue::Null(_) => Ok(Self::default()),
            // Array format: list of repository configs
            compote::ContextValue::Array(arr, _) => {
                let mut repositories = Vec::new();
                for (idx, v) in arr.iter().enumerate() {
                    tracker.push_index(idx);
                    match <SuggestCloneRepositoryConfig as compote::FromContextValue<S, L>>::from_context_value(v, tracker) {
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
            compote::ContextValue::Object(table, _) => {
                // Check for repositories array
                if let Some(v) = table.get("repositories") {
                    if let compote::ContextValue::Array(arr, _) = v {
                        let mut repositories = Vec::new();
                        for (idx, repo_v) in arr.iter().enumerate() {
                            tracker.push_field("repositories");
                            tracker.push_index(idx);
                            match <SuggestCloneRepositoryConfig as compote::FromContextValue<S, L>>::from_context_value(repo_v, tracker) {
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
                    let template = String::from_context_value(v, tracker)?;
                    tracker.pop();
                    return Ok(Self {
                        template,
                        ..Default::default()
                    });
                }

                // Check for template_file
                if let Some(v) = table.get("template_file") {
                    tracker.push_field("template_file");
                    let template_file = String::from_context_value(v, tracker)?;
                    tracker.pop();
                    return Ok(Self {
                        template_file,
                        ..Default::default()
                    });
                }

                Ok(Self::default())
            }
            _ => Err(compote::Error::TypeMismatch {
                expected: "array or table".to_string(),
                actual: filtered.type_name().to_string(),
                path: tracker.current_path(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn template_supports_documented_conditionals_and_partial_resolve() {
        let config = SuggestCloneConfig {
            template: r#"
- {{ partial_resolve(handle="omni-example") }}
{% if prompts.team == "team1" %}
- {{ partial_resolve(handle="team1-tools") }}
{% endif %}
"#
            .to_string(),
            ..Default::default()
        };
        let mut context = Context::new();
        context.insert(
            "repo",
            &json!({"handle": "https://github.com/omnicli/omni.git"}),
        );
        context.insert("prompts", &json!({"team": "team1"}));

        let repositories = config.repositories_with_context(&context, true);

        assert_eq!(repositories.len(), 2);
        assert_eq!(
            repositories[0].handle,
            "https://github.com/omnicli/omni-example"
        );
        assert_eq!(
            repositories[1].handle,
            "https://github.com/omnicli/team1-tools"
        );
    }
}
