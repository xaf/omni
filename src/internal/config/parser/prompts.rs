use crate::internal::config::Value as FeuilletageValue;
use serde::Serialize;

use tera::Tera;

use crate::internal::cache::utils::Empty;
use crate::internal::cache::PromptsCache;
use crate::internal::config::template::config_template_context;
use crate::internal::config::template::register_partial_resolve_placeholder;
use crate::internal::config::template::render_config_template;
use crate::internal::config::template::tera_render_error_message;
use crate::internal::git_env;
use crate::internal::user_interface::colors::StringColor;
use crate::omni_warning;

// Feuilletage imports
use crate::internal::config::FeuilletageConfigContext;
use crate::internal::config::FeuilletageConfigLevel;
use crate::internal::config::FeuilletageConfigSource;

#[derive(Default, Debug, Clone, feuilletage::Config)]
#[feuilletage(parse_as = "PromptsConfigWire", skip_serialize)]
pub struct PromptsConfig {
    pub prompts: Vec<PromptConfig>,
}

impl Empty for PromptsConfig {
    fn is_empty(&self) -> bool {
        self.prompts.is_empty()
    }
}

impl feuilletage::IsEmpty for PromptsConfig {
    fn is_empty(&self) -> bool {
        Empty::is_empty(self)
    }
}

impl Serialize for PromptsConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.prompts.serialize(serializer)
    }
}

impl PromptsConfig {
    pub fn iter(&self) -> impl Iterator<Item = &PromptConfig> {
        self.prompts.iter()
    }
}

#[derive(Debug, Serialize, Clone, feuilletage::Config)]
#[feuilletage(parse_as = "PromptConfigWire", skip_serialize)]
pub struct PromptConfig {
    pub id: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "feuilletage_value_is_null")]
    pub default: FeuilletageValue,
    #[serde(flatten, skip_serializing_if = "PromptType::is_default")]
    pub prompt_type: PromptType,
    #[serde(skip_serializing_if = "PromptScope::is_default")]
    pub scope: PromptScope,
    #[serde(skip_serializing_if = "Option::is_none", rename = "if")]
    pub if_condition: Option<String>,
}

impl PromptConfig {
    pub fn should_prompt(&self) -> bool {
        match &self.if_condition {
            Some(if_condition) => {
                let if_condition = if_condition.trim().to_lowercase();

                matches!(if_condition.as_str(), "true" | "yes" | "on" | "1")
            }
            None => true,
        }
    }

    pub fn in_context(&self) -> Result<Self, String> {
        let template_context = config_template_context(".");

        // Dump self as yaml string
        let yaml = match serde_yaml::to_string(self) {
            Ok(yaml) => yaml,
            Err(err) => {
                return Err(format!(
                    "failed to serialize prompt {} to yaml: {}",
                    self.id, err
                ))
            }
        };

        let mut template = Tera::default();
        register_partial_resolve_placeholder(&mut template);
        let prompt_key = format!("prompt.{}", self.id);
        if let Err(err) = template.add_raw_template(&prompt_key, yaml.as_str()) {
            return Err(tera_render_error_message(err));
        }

        match render_config_template(&template, &template_context) {
            Ok(value) => {
                // Parse YAML using feuilletage
                let context = FeuilletageConfigContext::new(
                    FeuilletageConfigSource::Programmatic,
                    FeuilletageConfigLevel::Local,
                );
                let config_value = match feuilletage::loader::load_yaml(&value, context) {
                    Ok(cv) => cv,
                    Err(err) => {
                        return Err(format!(
                            "failed to parse prompt {} as yaml: {}",
                            self.id, err
                        ))
                    }
                };

                // Use feuilletage's FromContextValue implementation
                let mut tracker = feuilletage::ErrorTracker::new();
                match <Self as feuilletage::FromContextValue<
                    FeuilletageConfigSource,
                    FeuilletageConfigLevel,
                >>::from_context_value(&config_value, &mut tracker)
                {
                    Ok(prompt) => Ok(prompt),
                    Err(err) => Err(format!(
                        "failed to parse prompt {} from rendered template: {}",
                        self.id, err
                    )),
                }
            }
            Err(err) => Err(tera_render_error_message(err)),
        }
    }

    pub fn prompt(&self) -> bool {
        self.prompt_type.prompt(
            self.id.as_str(),
            self.prompt.as_str(),
            self.default.clone(),
            self.scope,
        )
    }
}

#[derive(Default, Debug, Serialize, Clone, Copy, feuilletage::Config)]
#[feuilletage(untagged, parse_as = "PromptScopeWire", skip_serialize)]
pub enum PromptScope {
    #[default]
    #[serde(rename = "repo", alias = "repository")]
    Repository,
    #[serde(rename = "org", alias = "organization")]
    Organization,
}

impl PromptScope {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Repository)
    }
}

#[derive(Default, Debug, Serialize, Clone, feuilletage::Config)]
#[feuilletage(untagged, parse_as = "PromptTypeWire", skip_serialize)]
#[serde(tag = "type")]
pub enum PromptType {
    #[default]
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "password")]
    Password,
    #[serde(rename = "confirm", alias = "boolean")]
    Confirm,
    #[serde(rename = "choice", alias = "select")]
    Choice { choices: PromptChoicesConfig },
    #[serde(rename = "multichoice", alias = "choices", alias = "multiselect")]
    MultiChoice { choices: PromptChoicesConfig },
    #[serde(rename = "int")]
    Int {
        #[serde(skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    #[serde(rename = "float")]
    Float {
        #[serde(skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
}

impl PromptType {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Text)
    }

    pub fn prompt(
        &self,
        id: &str,
        prompt: &str,
        default: FeuilletageValue,
        scope: PromptScope,
    ) -> bool {
        // Override the default value with the cached answer if there is one
        // for the current scope; otherwise, use the default value.
        // The cache returns serde_json::Value, so we convert it to FeuilletageValue.
        let default = match PromptsCache::get().answers(".").get(id) {
            Some(answer) => json_value_to_feuilletage_value(answer),
            None => default,
        };

        let question = match self {
            Self::Text => {
                let mut question = requestty::Question::input(id)
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(prompt);

                if !default.is_null() {
                    if let Some(default) = default.as_str().map(|s| s.to_string()) {
                        question = question.default(default);
                    }
                }

                question.build()
            }
            Self::Password => requestty::Question::password(id)
                .ask_if_answered(true)
                .on_esc(requestty::OnEsc::Terminate)
                .message(prompt)
                .build(),
            Self::Confirm => {
                let mut question = requestty::Question::confirm(id)
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(prompt);

                if !default.is_null() {
                    if let Some(default) = default.as_bool() {
                        question = question.default(default);
                    }
                }

                question.build()
            }
            Self::Choice { choices } => {
                let choices = match choices.choices() {
                    Ok(choices) => choices,
                    Err(err) => {
                        omni_warning!(format!(
                            "failed to parse choices for prompt {}: {}",
                            id, err
                        ));
                        return false;
                    }
                };

                let mut question = requestty::Question::select(id)
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(prompt)
                    .choices(choices.clone());

                if !default.is_null() {
                    if let Some(default) = default.as_i64() {
                        let default_index = default as usize;
                        if default_index < choices.len() {
                            question = question.default(default_index);
                        }
                    }

                    if let Some(default) = default.as_str().map(|s| s.to_string()) {
                        // Find the index of the default choice
                        if let Some(index) = choices.iter().position(|choice| choice.id == default)
                        {
                            question = question.default(index);
                        }
                    }
                }

                question.build()
            }
            Self::MultiChoice { choices } => {
                let choices = match choices.choices() {
                    Ok(choices) => choices,
                    Err(err) => {
                        omni_warning!(format!(
                            "failed to parse choices for prompt {}: {}",
                            id, err
                        ));
                        return false;
                    }
                };

                let mut choices_with_default = choices
                    .iter()
                    .map(|choice| (choice, false))
                    .collect::<Vec<_>>();

                if !default.is_null() {
                    let defaults: Vec<FeuilletageValue> = match default.clone() {
                        FeuilletageValue::Array(defaults) => defaults,
                        FeuilletageValue::String(_) => vec![default],
                        FeuilletageValue::Int(_) => vec![default],
                        _ => vec![],
                    };

                    for default in defaults {
                        if let Some(default) = default.as_i64() {
                            let default_index = default as usize;
                            if default_index < choices.len() {
                                choices_with_default[default_index].1 = true;
                                continue;
                            }
                        }

                        if let Some(default) = default.as_str().map(|s| s.to_string()) {
                            // Find the index of the default choice
                            if let Some(index) =
                                choices.iter().position(|choice| choice.id == default)
                            {
                                choices_with_default[index].1 = true;
                                continue;
                            }
                        }
                    }
                }

                requestty::Question::multi_select(id)
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(prompt)
                    .choices_with_default(choices_with_default)
                    .build()
            }
            Self::Int { min, max } => {
                let mut question = requestty::Question::int(id)
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(prompt);

                if !default.is_null() {
                    if let Some(default) = default.as_i64() {
                        question = question.default(default);
                    }
                }

                if min.is_some() || max.is_some() {
                    question = question.validate(|answer, _previous_answers| {
                        // Make sure that min and max are cloned since the
                        // closure will outlive the current block
                        #[allow(clippy::clone_on_copy)]
                        let min = min.clone();
                        #[allow(clippy::clone_on_copy)]
                        let max = max.clone();

                        let errmsg = match (min, max) {
                            (Some(min), Some(max)) => {
                                format!("Answer must be between {min} and {max}")
                            }
                            (Some(min), None) => {
                                format!("Answer must be greater than or equal to {min}")
                            }
                            (None, Some(max)) => {
                                format!("Answer must be lower than or equal to {max}")
                            }
                            _ => unreachable!(),
                        };

                        if let Some(min) = min {
                            if answer < min {
                                return Err(errmsg.clone());
                            }
                        }

                        if let Some(max) = max {
                            if answer > max {
                                return Err(errmsg.clone());
                            }
                        }

                        Ok(())
                    });
                }

                question.build()
            }
            Self::Float { min, max } => {
                let mut question = requestty::Question::float(id)
                    .ask_if_answered(true)
                    .on_esc(requestty::OnEsc::Terminate)
                    .message(prompt);

                if !default.is_null() {
                    if let Some(default) = default.as_f64() {
                        question = question.default(default);
                    }
                }

                if min.is_some() || max.is_some() {
                    question = question.validate(|answer, _previous_answers| {
                        // Make sure that min and max are cloned since the
                        // closure will outlive the current block
                        #[allow(clippy::clone_on_copy)]
                        let min = min.clone();
                        #[allow(clippy::clone_on_copy)]
                        let max = max.clone();

                        let errmsg = match (min, max) {
                            (Some(min), Some(max)) => {
                                format!("Answer must be between {min} and {max}")
                            }
                            (Some(min), None) => {
                                format!("Answer must be greater than or equal to {min}")
                            }
                            (None, Some(max)) => {
                                format!("Answer must be lower than or equal to {max}")
                            }
                            _ => unreachable!(),
                        };

                        if let Some(min) = min {
                            if answer < min {
                                return Err(errmsg.clone());
                            }
                        }

                        if let Some(max) = max {
                            if answer > max {
                                return Err(errmsg.clone());
                            }
                        }

                        Ok(())
                    });
                }

                question.build()
            }
        };

        let git = git_env(".");
        let (scope_org, scope_repo) = match git.url() {
            Some(url) => match (url.owner, url.name) {
                (Some(org), name) if !name.is_empty() => (
                    Some(org),
                    match scope {
                        PromptScope::Repository => Some(name),
                        PromptScope::Organization => None,
                    },
                ),
                _ => (None, None),
            },
            None => {
                // TODO: make it work for any workdir by storing for the workdir id
                //       instead of the org and repo
                omni_warning!("prompts are not available outside of a git repository");
                return false;
            }
        };

        let scope_org = match scope_org {
            Some(org) => org,
            None => {
                omni_warning!("unable to determine the organization of the repository");
                return false;
            }
        };

        // Create the answer value as serde_json::Value for cache storage
        let answer_value: serde_json::Value = match requestty::prompt_one(question) {
            Ok(answer) => match answer {
                requestty::Answer::String(answer) => serde_json::Value::String(answer),
                requestty::Answer::Bool(answer) => serde_json::Value::Bool(answer),
                requestty::Answer::Int(answer) => serde_json::json!(answer),
                requestty::Answer::Float(answer) => serde_json::json!(answer),
                requestty::Answer::ListItem(answer) => {
                    let choices = match self {
                        Self::Choice { choices } => match choices.choices() {
                            Ok(choices) => choices,
                            Err(_err) => return false,
                        },
                        _ => {
                            omni_warning!("invalid prompt type");
                            return false;
                        }
                    };

                    let selected_choice = match choices.get(answer.index) {
                        Some(choice) => choice.id.to_string(),
                        None => {
                            omni_warning!("invalid choice index");
                            return false;
                        }
                    };

                    serde_json::Value::String(selected_choice)
                }
                requestty::Answer::ListItems(answers) => {
                    let choices = match self {
                        Self::MultiChoice { choices } => match choices.choices() {
                            Ok(choices) => choices,
                            Err(_err) => return false,
                        },
                        _ => {
                            omni_warning!("invalid prompt type");
                            return false;
                        }
                    };

                    let selected_choices: Vec<serde_json::Value> = answers
                        .iter()
                        .filter_map(|answer| choices.get(answer.index))
                        .map(|choice| serde_json::Value::String(choice.id.to_string()))
                        .collect();

                    serde_json::Value::Array(selected_choices)
                }
                _ => unimplemented!(),
            },
            Err(err) => {
                println!("{}", format!("[✘] {err:?}").red());
                return false;
            }
        };

        if let Err(err) = PromptsCache::get().add_answer(id, scope_org, scope_repo, answer_value) {
            omni_warning!(format!("failed to update cache: {}", err));
            false
        } else {
            true
        }
    }
}

#[derive(Debug, Clone, feuilletage::Config)]
#[feuilletage(untagged, parse_as = "PromptChoicesWire", skip_serialize)]
pub enum PromptChoicesConfig {
    ChoicesAsArray(Vec<PromptChoiceConfig>),
    ChoicesAsString(String),
}

impl PromptChoicesConfig {
    pub fn choices(&self) -> Result<Vec<PromptChoiceConfig>, String> {
        match self {
            Self::ChoicesAsArray(choices) => Ok(choices.clone()),
            Self::ChoicesAsString(template) => {
                // Parse YAML using feuilletage
                let context = FeuilletageConfigContext::new(
                    FeuilletageConfigSource::Programmatic,
                    FeuilletageConfigLevel::Local,
                );
                let config_value = match feuilletage::loader::load_yaml(template, context) {
                    Ok(cv) => cv,
                    Err(err) => {
                        return Err(format!("failed to parse choices template as yaml: {err}"));
                    }
                };

                // Convert to Vec<PromptChoiceConfig> using feuilletage's FromContextValue
                let mut tracker = feuilletage::ErrorTracker::new();
                match <Vec<PromptChoiceConfig> as feuilletage::FromContextValue<
                    FeuilletageConfigSource,
                    FeuilletageConfigLevel,
                >>::from_context_value(&config_value, &mut tracker)
                {
                    Ok(choices) => {
                        if choices.is_empty() {
                            Err("choices template must be a non-empty array".to_string())
                        } else {
                            Ok(choices)
                        }
                    }
                    Err(_) => Err("choices template must be an array".to_string()),
                }
            }
        }
    }
}

impl Serialize for PromptChoicesConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        match self {
            Self::ChoicesAsArray(choices) => choices.serialize(serializer),
            Self::ChoicesAsString(template) => template.serialize(serializer),
        }
    }
}

#[derive(Debug, Clone, feuilletage::Config)]
#[feuilletage(scalar_as = "id")]
pub struct PromptChoiceConfig {
    #[feuilletage(fallback = "choice")]
    pub id: String,

    #[feuilletage(fallback = "id")]
    pub choice: String,
}

impl From<PromptChoiceConfig> for String {
    fn from(choice: PromptChoiceConfig) -> String {
        choice.choice
    }
}

impl From<&PromptChoiceConfig> for String {
    fn from(choice: &PromptChoiceConfig) -> String {
        choice.choice.clone()
    }
}

// ============================================================================
// FeuilletageValue helper functions
// ============================================================================

/// Helper function for serde skip_serializing_if
fn feuilletage_value_is_null(value: &FeuilletageValue) -> bool {
    matches!(value, FeuilletageValue::Null)
}

/// Convert serde_json::Value to feuilletage::Value
fn json_value_to_feuilletage_value(value: &serde_json::Value) -> FeuilletageValue {
    match value {
        serde_json::Value::Null => FeuilletageValue::Null,
        serde_json::Value::Bool(b) => FeuilletageValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                FeuilletageValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                FeuilletageValue::Float(f)
            } else {
                FeuilletageValue::Null
            }
        }
        serde_json::Value::String(s) => FeuilletageValue::String(s.clone()),
        serde_json::Value::Array(arr) => {
            let items: Vec<FeuilletageValue> =
                arr.iter().map(json_value_to_feuilletage_value).collect();
            FeuilletageValue::Array(items)
        }
        serde_json::Value::Object(map) => {
            let items: indexmap::IndexMap<String, FeuilletageValue> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_value_to_feuilletage_value(v)))
                .collect();
            FeuilletageValue::Object(items)
        }
    }
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(
    transparent,
    transform = "self::normalize_prompts_wire",
    skip_serialize,
    skip_deserialize
)]
struct PromptsConfigWire(Vec<PromptConfig>);

fn normalize_prompts_wire<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &mut feuilletage::ContextValue<S, L>,
    context: &feuilletage::Context<S, L>,
) -> Result<(), feuilletage::Error> {
    if value.is_null() {
        *value = feuilletage::ContextValue::array(Vec::new(), context.clone());
    }
    Ok(())
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(
    transform = "self::normalize_prompt_config_wire",
    skip_serialize,
    skip_deserialize
)]
struct PromptConfigWire {
    id: PromptRequiredStringWire,
    prompt: PromptRequiredStringWire,
    #[feuilletage(default)]
    default: Option<FeuilletageValue>,
    #[feuilletage(default)]
    scope: PromptScope,
    #[feuilletage(default, rename = "if")]
    if_condition: FeuilletageValue,
    #[feuilletage(flatten)]
    prompt_type: PromptType,
}

fn normalize_prompt_config_wire<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &mut feuilletage::ContextValue<S, L>,
    _context: &feuilletage::Context<S, L>,
) -> Result<(), feuilletage::Error> {
    let feuilletage::ContextValue::Object(table, _) = value else {
        return Ok(());
    };
    match table.get_mut("type") {
        Some(feuilletage::ContextValue::String(value, _)) => {
            *value = value.trim().to_lowercase();
            if !is_known_prompt_type(value) {
                table.shift_remove("type");
            }
        }
        Some(_) => {
            table.shift_remove("type");
        }
        None => {}
    }
    Ok(())
}

fn is_known_prompt_type(value: &str) -> bool {
    matches!(
        value,
        "text"
            | "password"
            | "confirm"
            | "boolean"
            | "choice"
            | "select"
            | "choices"
            | "multichoice"
            | "multiselect"
            | "int"
            | "float"
    )
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(transparent, skip_serialize, skip_deserialize)]
struct PromptRequiredStringWire(FeuilletageValue);

fn required_prompt_string(
    value: FeuilletageValue,
    field: &str,
    tracker: &mut feuilletage::ErrorTracker,
) -> Result<String, feuilletage::Error> {
    tracker.push_field(field);
    let result = match value {
        FeuilletageValue::String(value) => {
            let value = value.trim().to_string();
            if value.is_empty() {
                Err(feuilletage::Error::InvalidValue {
                    message: format!("{field} cannot be empty"),
                    path: tracker.current_path(),
                })
            } else {
                Ok(value)
            }
        }
        value => Err(feuilletage::Error::TypeMismatch {
            expected: "string".to_string(),
            actual: value.type_name().to_string(),
            path: tracker.current_path(),
        }),
    };
    tracker.pop();
    result
}

fn record_prompt_type_diagnostic<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    original: &feuilletage::ContextValue<S, L>,
    tracker: &mut feuilletage::ErrorTracker,
) {
    let feuilletage::ContextValue::Object(table, _) = original else {
        return;
    };
    match table.get("type") {
        None => {}
        Some(feuilletage::ContextValue::String(value, _)) => {
            let value = value.trim().to_lowercase();
            if value.is_empty() {
                tracker.push_field("type");
                tracker.record(feuilletage::Error::InvalidValue {
                    message: "type cannot be empty".to_string(),
                    path: tracker.current_path(),
                });
                tracker.pop();
            } else if !is_known_prompt_type(&value) {
                tracker.push_field("type");
                tracker.record(feuilletage::Error::InvalidValue {
                    message: format!(
                        "invalid type '{}': expected text, password, confirm, choice, multichoice, int, or float",
                        value
                    ),
                    path: tracker.current_path(),
                });
                tracker.pop();
            }
        }
        Some(value) => {
            tracker.push_field("type");
            tracker.record(feuilletage::Error::TypeMismatch {
                expected: "string".to_string(),
                actual: value.type_name().to_string(),
                path: tracker.current_path(),
            });
            tracker.pop();
        }
    }
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(
    transparent,
    post_process = "normalize_prompt_scope_wire",
    skip_serialize,
    skip_deserialize
)]
struct PromptScopeWire(String);

fn normalize_prompt_scope_wire<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    parsed: &mut PromptScopeWire,
    original: &feuilletage::ContextValue<S, L>,
    tracker: &mut feuilletage::ErrorTracker,
) -> Result<(), feuilletage::Error> {
    if !matches!(original, feuilletage::ContextValue::String(_, _)) {
        return Err(feuilletage::Error::TypeMismatch {
            expected: "string".to_string(),
            actual: original.type_name().to_string(),
            path: tracker.current_path(),
        });
    }
    parsed.0 = parsed.0.trim().to_lowercase();
    Ok(())
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(tag = "type", skip_serialize, skip_deserialize)]
enum PromptTypeWire {
    #[feuilletage(rename = "text", fallback)]
    Text,
    #[feuilletage(rename = "password")]
    Password,
    #[feuilletage(rename = "confirm", aliases = ["boolean"])]
    Confirm,
    #[feuilletage(rename = "choice", aliases = ["select"])]
    Choice { choices: PromptChoicesConfig },
    #[feuilletage(
        rename = "multichoice",
        aliases = ["choices", "multiselect"]
    )]
    MultiChoice { choices: PromptChoicesConfig },
    #[feuilletage(rename = "int")]
    Int {
        #[feuilletage(default)]
        min: FeuilletageValue,
        #[feuilletage(default)]
        max: FeuilletageValue,
    },
    #[feuilletage(rename = "float")]
    Float {
        #[feuilletage(default)]
        min: FeuilletageValue,
        #[feuilletage(default)]
        max: FeuilletageValue,
    },
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(
    transparent,
    transform = "self::normalize_prompt_choices_wire",
    skip_serialize,
    skip_deserialize
)]
struct PromptChoicesWire(Vec<PromptChoiceConfig>);

fn normalize_prompt_choices_wire<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &mut feuilletage::ContextValue<S, L>,
    context: &feuilletage::Context<S, L>,
) -> Result<(), feuilletage::Error> {
    match value {
        feuilletage::ContextValue::Array(_, _) => Ok(()),
        feuilletage::ContextValue::String(_, _) => {
            *value = feuilletage::ContextValue::array(Vec::new(), context.clone());
            Ok(())
        }
        _ => {
            *value = feuilletage::ContextValue::array(Vec::new(), context.clone());
            Ok(())
        }
    }
}

// The public types retain their custom serde shapes while these projections define parsing.

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>
    feuilletage::FromParsed<PromptsConfigWire, S, L> for PromptsConfig
{
    fn from_parsed(
        parsed: PromptsConfigWire,
        _original: &feuilletage::ContextValue<S, L>,
        _tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        Ok(Self { prompts: parsed.0 })
    }
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>
    feuilletage::FromParsed<PromptConfigWire, S, L> for PromptConfig
{
    fn from_parsed(
        parsed: PromptConfigWire,
        original: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        let id = required_prompt_string(parsed.id.0, "id", tracker)?;
        let prompt = required_prompt_string(parsed.prompt.0, "prompt", tracker)?;
        record_prompt_type_diagnostic(original, tracker);
        let if_condition = match parsed.if_condition {
            FeuilletageValue::String(value) => Some(value),
            FeuilletageValue::Bool(value) => Some(value.to_string()),
            FeuilletageValue::Int(value) => Some(value.to_string()),
            _ => None,
        };
        let has_default = matches!(
            original,
            feuilletage::ContextValue::Object(table, _) if table.contains_key("default")
        );

        Ok(Self {
            id,
            prompt,
            default: if has_default {
                parsed.default.unwrap_or(FeuilletageValue::Null)
            } else {
                FeuilletageValue::Null
            },
            prompt_type: parsed.prompt_type,
            scope: parsed.scope,
            if_condition,
        })
    }
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>
    feuilletage::FromParsed<PromptScopeWire, S, L> for PromptScope
{
    fn from_parsed(
        parsed: PromptScopeWire,
        _original: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        match parsed.0.as_str() {
            "repo" | "repository" => Ok(Self::Repository),
            "org" | "organization" => Ok(Self::Organization),
            scope => Err(feuilletage::Error::InvalidValue {
                message: format!("invalid scope '{scope}': expected 'repo' or 'org'"),
                path: tracker.current_path(),
            }),
        }
    }
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>
    feuilletage::FromParsed<PromptTypeWire, S, L> for PromptType
{
    fn from_parsed(
        parsed: PromptTypeWire,
        _original: &feuilletage::ContextValue<S, L>,
        _tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        Ok(match parsed {
            PromptTypeWire::Text => Self::Text,
            PromptTypeWire::Password => Self::Password,
            PromptTypeWire::Confirm => Self::Confirm,
            PromptTypeWire::Choice { choices } => Self::Choice { choices },
            PromptTypeWire::MultiChoice { choices } => Self::MultiChoice { choices },
            PromptTypeWire::Int { min, max } => Self::Int {
                min: match min {
                    FeuilletageValue::Int(value) => Some(value),
                    _ => None,
                },
                max: match max {
                    FeuilletageValue::Int(value) => Some(value),
                    _ => None,
                },
            },
            PromptTypeWire::Float { min, max } => Self::Float {
                min: match min {
                    FeuilletageValue::Float(value) => Some(value),
                    FeuilletageValue::Int(value) => Some(value as f64),
                    _ => None,
                },
                max: match max {
                    FeuilletageValue::Float(value) => Some(value),
                    FeuilletageValue::Int(value) => Some(value as f64),
                    _ => None,
                },
            },
        })
    }
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>
    feuilletage::FromParsed<PromptChoicesWire, S, L> for PromptChoicesConfig
{
    fn from_parsed(
        parsed: PromptChoicesWire,
        original: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        match original {
            feuilletage::ContextValue::Array(_, _) => {
                if parsed.0.is_empty() {
                    Err(feuilletage::Error::InvalidValue {
                        message: "choices cannot be empty".to_string(),
                        path: tracker.current_path(),
                    })
                } else {
                    Ok(Self::ChoicesAsArray(parsed.0))
                }
            }
            feuilletage::ContextValue::String(template, _) => {
                Ok(Self::ChoicesAsString(template.clone()))
            }
            value => Err(feuilletage::Error::TypeMismatch {
                expected: "array or template string".to_string(),
                actual: value.type_name().to_string(),
                path: tracker.current_path(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_prompts(yaml: &str) -> (PromptsConfig, Vec<feuilletage::Error>) {
        let mut config = feuilletage::Config::default();
        config.load_yaml(
            yaml,
            FeuilletageConfigContext::new(
                FeuilletageConfigSource::Programmatic,
                FeuilletageConfigLevel::Local,
            ),
        );
        let prompts = config.deserialize::<PromptsConfig>().unwrap();
        (prompts, config.get_errors().to_vec())
    }

    #[test]
    fn prompt_projection_preserves_aliases_bounds_and_coercions() {
        let (config, errors) = parse_prompts(
            r#"
- id: " integer "
  prompt: " Number "
  type: " INT "
  min: 1
  max: 3
  scope: repository
  if: true
- id: select
  prompt: Pick
  type: select
  choices: [one, { id: two, choice: Two }]
"#,
        );

        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(config.prompts.len(), 2);
        assert_eq!(config.prompts[0].id, "integer");
        assert_eq!(config.prompts[0].prompt, "Number");
        assert!(matches!(config.prompts[0].scope, PromptScope::Repository));
        assert_eq!(config.prompts[0].if_condition.as_deref(), Some("true"));
        assert!(matches!(
            config.prompts[0].prompt_type,
            PromptType::Int {
                min: Some(1),
                max: Some(3)
            }
        ));
        assert!(matches!(
            &config.prompts[1].prompt_type,
            PromptType::Choice {
                choices: PromptChoicesConfig::ChoicesAsArray(choices)
            } if choices.len() == 2 && choices[0].id == "one" && choices[1].choice == "Two"
        ));
    }

    #[test]
    fn prompt_projection_recovers_from_invalid_array_entries() {
        let (config, errors) = parse_prompts(
            r#"
- id: false
  prompt: Invalid
- id: valid
  prompt: Valid
"#,
        );

        assert_eq!(config.prompts.len(), 1, "{errors:?}");
        assert_eq!(config.prompts[0].id, "valid");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].location(), "0.id");
    }

    #[test]
    fn prompt_projection_defaults_invalid_scope_and_type() {
        let (config, errors) = parse_prompts(
            r#"
- id: scope
  prompt: Scope
  scope: elsewhere
- id: type
  prompt: Type
  type: unknown
- id: empty-type
  prompt: Empty type
  type: ""
- id: non-string-type
  prompt: Non-string type
  type: false
"#,
        );

        assert_eq!(config.prompts.len(), 4, "{errors:?}");
        assert!(matches!(config.prompts[0].scope, PromptScope::Repository));
        assert!(matches!(config.prompts[1].prompt_type, PromptType::Text));
        assert!(matches!(config.prompts[2].prompt_type, PromptType::Text));
        assert!(matches!(config.prompts[3].prompt_type, PromptType::Text));
        assert_eq!(errors.len(), 4, "{errors:?}");
        assert_eq!(errors[0].location(), "0.scope");
        assert_eq!(errors[1].location(), "1.type");
        assert_eq!(errors[2].location(), "2.type");
        assert_eq!(errors[3].location(), "3.type");
    }

    #[test]
    fn prompt_projection_preserves_serialized_shape() {
        let (config, errors) = parse_prompts(
            r#"
- id: choice
  prompt: Pick
  type: choice
  choices: [one]
"#,
        );

        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            serde_json::to_value(config).unwrap(),
            serde_json::json!([{
                "id": "choice",
                "prompt": "Pick",
                "type": "choice",
                "choices": [{"id": "one", "choice": "one"}]
            }])
        );
    }

    #[test]
    fn prompt_projection_accepts_null_as_an_empty_prompt_list() {
        let (config, errors) = parse_prompts("null");

        assert!(errors.is_empty(), "{errors:?}");
        assert!(config.prompts.is_empty());
    }

    #[test]
    fn prompt_projection_only_parses_choices_for_choice_types() {
        let (config, errors) = parse_prompts(
            r#"
- id: text
  prompt: Text
  type: text
  choices: false
- id: choice
  prompt: Choice
  type: choice
  choices: false
"#,
        );

        assert_eq!(config.prompts.len(), 1);
        assert_eq!(config.prompts[0].id, "text");
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].location(), "1.choices");
    }

    #[test]
    fn prompt_projection_keeps_valid_choices_when_siblings_are_invalid() {
        let (config, errors) = parse_prompts(
            r#"
- id: choice
  prompt: Choice
  type: choice
  choices: [valid, { other: false }]
"#,
        );

        assert_eq!(config.prompts.len(), 1);
        assert!(matches!(
            &config.prompts[0].prompt_type,
            PromptType::Choice {
                choices: PromptChoicesConfig::ChoicesAsArray(choices)
            } if choices.len() == 1 && choices[0].id == "valid"
        ));
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert_eq!(errors[0].location(), "0.choices.1", "{errors:?}");
        assert_eq!(errors[1].location(), "0.choices.1");
    }
}
