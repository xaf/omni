use std::collections::HashMap;
use std::str::FromStr;

use serde::Serialize;
use tera::Context;
use tera::Tera;

use crate::internal::cache::utils::Empty;
use crate::internal::config::parser::suggest_config::select_local_scope;
use crate::internal::config::template::config_template_context;
use crate::internal::config::template::register_partial_resolve_placeholder;
use crate::internal::config::template::render_config_template;
use crate::internal::config::template::tera_render_error_message;
use crate::internal::config::FeuilletageConfigContext;
use crate::internal::config::FeuilletageConfigLevel;
use crate::internal::config::FeuilletageConfigSource;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_warning;

fn synthetic_context() -> FeuilletageConfigContext {
    FeuilletageConfigContext::new(
        FeuilletageConfigSource::Programmatic,
        FeuilletageConfigLevel::Local,
    )
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(untagged)]
enum SuggestCloneConfigWire {
    Repositories {
        repositories: Vec<feuilletage::Value>,
    },
    Template {
        template: String,
    },
    TemplateFile {
        #[feuilletage(relative_path)]
        template_file: String,
    },
    RepositoriesList(Vec<feuilletage::Value>),
    #[feuilletage(fallback)]
    Other(feuilletage::Value),
}

#[derive(Default, Debug, Clone, feuilletage::Config)]
#[feuilletage(
    parse_as = "SuggestCloneConfigWire",
    skip_serialize,
    skip_deserialize
)]
pub struct SuggestCloneConfig {
    repositories: Vec<SuggestCloneRepositoryConfig>,
    pub template: String,
    pub template_file: String,
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>
    feuilletage::FromParsed<SuggestCloneConfigWire, S, L> for SuggestCloneConfig
{
    fn from_parsed(
        _parsed: SuggestCloneConfigWire,
        original: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        let Some(original) = select_local_scope(original) else {
            return Ok(Self::default());
        };
        let parsed =
            <SuggestCloneConfigWire as feuilletage::FromContextValue<S, L>>::from_context_value(
                &original, tracker,
            )?;

        match parsed {
            SuggestCloneConfigWire::Repositories { .. } => {
                let repositories = original
                    .as_object()
                    .and_then(|values| values.get("repositories"))
                    .expect("repositories wire variant requires a repositories field");
                tracker.push_field("repositories");
                let parsed = <Vec<SuggestCloneRepositoryConfig> as feuilletage::FromContextValue<
                    S,
                    L,
                >>::from_context_value(repositories, tracker);
                tracker.pop();
                Ok(Self {
                    repositories: parsed?,
                    ..Default::default()
                })
            }
            SuggestCloneConfigWire::RepositoriesList(_) => {
                Ok(Self {
                    repositories:
                        <Vec<SuggestCloneRepositoryConfig> as feuilletage::FromContextValue<
                            S,
                            L,
                        >>::from_context_value(&original, tracker)?,
                    ..Default::default()
                })
            }
            SuggestCloneConfigWire::Template { template } => Ok(Self {
                template,
                ..Default::default()
            }),
            SuggestCloneConfigWire::TemplateFile { template_file } => Ok(Self {
                template_file,
                ..Default::default()
            }),
            SuggestCloneConfigWire::Other(value) => match value {
                feuilletage::Value::Null | feuilletage::Value::Object(_) => Ok(Self::default()),
                _ => Err(feuilletage::Error::TypeMismatch {
                    expected: "array or table".to_string(),
                    actual: value.type_name().to_string(),
                    path: tracker.current_path(),
                }),
            },
        }
    }
}

impl Empty for SuggestCloneConfig {
    fn is_empty(&self) -> bool {
        self.repositories.is_empty() && self.template.is_empty() && self.template_file.is_empty()
    }
}

impl feuilletage::IsEmpty for SuggestCloneConfig {
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
            } else {
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
            if let Err(error) = template.add_raw_template("suggest_clone", &self.template) {
                if !quiet {
                    omni_warning!(tera_render_error_message(error));
                    omni_warning!("suggest_clone will be ignored");
                }
                return vec![];
            }
        } else if !self.template_file.is_empty() {
            if let Err(error) = template.add_template_file(&self.template_file, None) {
                if !quiet {
                    omni_warning!(tera_render_error_message(error));
                    omni_warning!("suggest_clone will be ignored");
                }
                return vec![];
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
                                Ok(suggest_clone) => {
                                    return suggest_clone
                                        .repositories_with_context(template_context, quiet);
                                }
                                Err(error) => {
                                    if !quiet {
                                        omni_warning!(format!(
                                            "Failed to parse suggest_clone template: {error}"
                                        ));
                                        omni_warning!("suggest_clone will be ignored");
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            if !quiet {
                                omni_warning!(format!(
                                    "Failed to parse suggest_clone template: {error}"
                                ));
                                omni_warning!("suggest_clone will be ignored");
                            }
                        }
                    }
                }
                Err(error) => {
                    if !quiet {
                        omni_warning!(tera_render_error_message(error));
                        omni_warning!("suggest_clone will be ignored");
                    }
                }
            }
        }

        vec![]
    }
}

#[derive(Debug, Clone, PartialEq, feuilletage::Config)]
#[feuilletage(value_matched)]
#[derive(Default)]
pub enum SuggestCloneTypeEnum {
    #[feuilletage(variant = "package")]
    #[default]
    Package,
    #[feuilletage(variant = "worktree")]
    Worktree,
}


impl FromStr for SuggestCloneTypeEnum {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_lowercase().as_str() {
            "package" => Ok(Self::Package),
            "worktree" => Ok(Self::Worktree),
            _ => Err(format!("Invalid: {value}")),
        }
    }
}

fn shell_words_transform<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &mut feuilletage::ContextValue<S, L>,
    _context: &feuilletage::Context<S, L>,
) -> Result<(), feuilletage::Error> {
    if let feuilletage::ContextValue::String(string, context) = value {
        let words = shell_words::split(string).unwrap_or_default();
        let values = words
            .into_iter()
            .map(|word| feuilletage::ContextValue::string(word, context.clone()))
            .collect();
        *value = feuilletage::ContextValue::array(values, context.clone());
    }
    Ok(())
}

#[derive(Debug, Serialize, Clone, feuilletage::Config)]
#[feuilletage(scalar_as = "handle", skip_serialize)]
pub struct SuggestCloneRepositoryConfig {
    pub handle: String,
    #[feuilletage(
        default,
        transform = "crate::internal::config::parser::suggest_clone::shell_words_transform"
    )]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[feuilletage(default)]
    pub clone_type: SuggestCloneTypeEnum,
}

impl SuggestCloneRepositoryConfig {
    pub fn clone_as_package(&self) -> bool {
        self.clone_type == SuggestCloneTypeEnum::Package
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    fn parse(
        yaml: &str,
        context: FeuilletageConfigContext,
    ) -> (SuggestCloneConfig, feuilletage::ErrorTracker) {
        let value = feuilletage::loader::load_yaml(yaml, context).unwrap();
        let mut tracker = feuilletage::ErrorTracker::new();
        let config = <SuggestCloneConfig as feuilletage::FromContextValue>::from_context_value(
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
    fn parses_list_and_table_repository_forms() {
        let (list, list_errors) = parse("- one\n- two\n", local_context());
        let (table, table_errors) = parse(
            "repositories:\n  - handle: one\n    args: --branch main\n    clone_type: worktree\n",
            local_context(),
        );

        assert!(list_errors.errors().is_empty());
        assert!(table_errors.errors().is_empty());
        assert_eq!(
            list.repositories
                .iter()
                .map(|repository| repository.handle.as_str())
                .collect::<Vec<_>>(),
            vec!["one", "two"]
        );
        let repository = &table.repositories[0];
        assert_eq!(repository.args, vec!["--branch", "main"]);
        assert!(!repository.clone_as_package());
    }

    #[test]
    fn retains_valid_repositories_and_records_indexed_errors() {
        let (config, tracker) = parse(
            "- valid\n- args: --branch main\n- also-valid\n",
            local_context(),
        );

        assert_eq!(
            config
                .repositories
                .iter()
                .map(|repository| repository.handle.as_str())
                .collect::<Vec<_>>(),
            vec!["valid", "also-valid"]
        );
        assert!(!tracker.errors().is_empty());
        let errors = format!("{:?}", tracker.errors());
        assert!(errors.contains("1.handle"), "{errors}");
    }

    #[test]
    fn resolves_template_file_relative_to_source_and_renders_it() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("omni.yaml");
        let template_path = directory.path().join("suggest.yaml");
        fs::write(&template_path, "- one\n- two\n").unwrap();
        let context =
            FeuilletageConfigContext::new_from_file(config_path, FeuilletageConfigLevel::Local);

        let (config, tracker) = parse("template_file: suggest.yaml\n", context);

        assert!(tracker.errors().is_empty());
        assert_eq!(config.template_file, template_path.to_string_lossy());
        assert_eq!(
            config
                .repositories_with_context(&Context::new(), true)
                .len(),
            2
        );
    }

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

    #[test]
    fn serializes_using_original_shorthand_shapes() {
        let (repositories, _) = parse("- one\n", local_context());
        let (template, _) = parse("template: '- one'\n", local_context());

        let repositories = serde_yaml::to_value(repositories).unwrap();
        let template = serde_yaml::to_value(template).unwrap();
        assert!(repositories.as_sequence().is_some());
        assert_eq!(
            template.get("template").and_then(serde_yaml::Value::as_str),
            Some("- one")
        );
    }

    #[test]
    fn direct_deserialization_ignores_non_local_configuration() {
        let (config, tracker) = parse(
            "- user-repository\n",
            FeuilletageConfigContext::new(
                FeuilletageConfigSource::Programmatic,
                FeuilletageConfigLevel::System,
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
        let repository = feuilletage::ContextValue::object(
            [
                (
                    "handle".to_string(),
                    feuilletage::ContextValue::string("nested-local".to_string(), local.clone()),
                ),
                (
                    "args".to_string(),
                    feuilletage::ContextValue::string("--ignored".to_string(), user.clone()),
                ),
            ]
            .into_iter()
            .collect(),
            user.clone(),
        );
        let value = feuilletage::ContextValue::array(
            vec![
                feuilletage::ContextValue::string("user-only".to_string(), user.clone()),
                repository,
                feuilletage::ContextValue::string("local-only".to_string(), local),
            ],
            user,
        );
        let mut tracker = feuilletage::ErrorTracker::new();

        let config = <SuggestCloneConfig as feuilletage::FromContextValue>::from_context_value(
            &value,
            &mut tracker,
        )
        .unwrap();

        assert!(tracker.errors().is_empty());
        assert_eq!(
            config
                .repositories
                .iter()
                .map(|repository| repository.handle.as_str())
                .collect::<Vec<_>>(),
            vec!["nested-local", "local-only"]
        );
        assert!(config.repositories[0].args.is_empty());
    }
}
