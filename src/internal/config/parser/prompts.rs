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

#[derive(Default, Debug, Clone)]
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

#[derive(Debug, Serialize, Clone)]
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
                    &self.id, err
                ))
            }
        };

        let mut template = Tera::default();
        register_partial_resolve_placeholder(&mut template);
        let prompt_key = format!("prompt.{}", &self.id);
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
                            &self.id, err
                        ))
                    }
                };

                // Use feuilletage's FromContextValue implementation
                let mut tracker = feuilletage::ErrorTracker::new();
                match <Self as feuilletage::FromContextValue<FeuilletageConfigSource, FeuilletageConfigLevel>>::from_context_value(&config_value, &mut tracker) {
                    Ok(prompt) => Ok(prompt),
                    Err(err) => Err(format!(
                        "failed to parse prompt {} from rendered template: {}",
                        &self.id, err
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

#[derive(Default, Debug, Serialize, Clone, Copy)]
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

#[derive(Default, Debug, Serialize, Clone)]
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

        if let Err(err) =
            PromptsCache::get().add_answer(id, scope_org, scope_repo, answer_value)
        {
            omni_warning!(format!("failed to update cache: {}", err));
            false
        } else {
            true
        }
    }
}

#[derive(Debug, Clone)]
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
                match <Vec<PromptChoiceConfig> as feuilletage::FromContextValue<FeuilletageConfigSource, FeuilletageConfigLevel>>::from_context_value(
                    &config_value,
                    &mut tracker,
                ) {
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
            let items: Vec<FeuilletageValue> = arr
                .iter()
                .map(json_value_to_feuilletage_value)
                .collect();
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

// ============================================================================
// Feuilletage FromContextValue implementations
// ============================================================================
//
// These manual implementations CANNOT be converted to derive macro due to technical limitations:
//
// 1. PromptsConfig - Custom serialization behavior
//    - Serializes as bare array, not as struct with "prompts" field
//    - The derive macro always generates struct-style serialization
//
// 2. PromptConfig - Complex deserialization requirements
//    - Requires parent-object extraction pattern (e.g., PromptScope reads "scope" field
//      from the same object, PromptType reads "type" field from the same object)
//    - These "sibling field" patterns require custom handling
//
// 3. PromptScope - Extracts from parent object's "scope" field
//    - Not a simple value-matched enum (reads "scope" key from parent table)
//    - Also applies case-insensitive matching
//
// 4. PromptType - Complex internally-tagged enum
//    - Extracts "type" from parent object and reads sibling fields (choices, min, max)
//    - Has multiple aliases mapping to same variant (e.g., "choices"/"multichoice"/"multiselect")
//    - Applies case-insensitive, trimmed matching on type string
//
// 5. PromptChoicesConfig - Union type (array or string)
//    - Can be Vec<PromptChoiceConfig> OR String (template)
//    - This pattern would need #[feuilletage(untagged)] but serialization is also custom
//
// Note: PromptChoiceConfig was converted to use the derive macro with:
//   - #[feuilletage(scalar_as = "id")] - handles string input
//   - #[feuilletage(fallback = "choice")] on id - id falls back to choice
//   - #[feuilletage(fallback = "id")] on choice - choice falls back to id

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel> feuilletage::FromContextValue<S, L>
    for PromptsConfig
{
    fn from_context_value(
        value: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        match value {
            feuilletage::ContextValue::Array(arr, _) => {
                let mut prompts = Vec::new();
                for (idx, item) in arr.iter().enumerate() {
                    tracker.push_index(idx);
                    match <PromptConfig as feuilletage::FromContextValue<S, L>>::from_context_value(
                        item, tracker,
                    ) {
                        Ok(prompt) => prompts.push(prompt),
                        Err(e) => tracker.record(e),
                    }
                    tracker.pop();
                }
                Ok(Self { prompts })
            }
            feuilletage::ContextValue::Null(_) => Ok(Self::default()),
            _ => Err(feuilletage::Error::TypeMismatch {
                expected: "array".to_string(),
                actual: value.type_name().to_string(),
                path: tracker.current_path(),
            }),
        }
    }
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel> feuilletage::FromContextValue<S, L>
    for PromptConfig
{
    fn from_context_value(
        value: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        let table = match value {
            feuilletage::ContextValue::Object(map, _) => map,
            _ => {
                return Err(feuilletage::Error::TypeMismatch {
                    expected: "table".to_string(),
                    actual: value.type_name().to_string(),
                    path: tracker.current_path(),
                });
            }
        };

        // Required: id
        let id = if let Some(v) = table.get("id") {
            tracker.push_field("id");
            let result = match v {
                feuilletage::ContextValue::String(s, _) => {
                    let trimmed = s.trim().to_string();
                    if trimmed.is_empty() {
                        Err(feuilletage::Error::InvalidValue {
                            message: "id cannot be empty".to_string(),
                            path: tracker.current_path(),
                        })
                    } else {
                        Ok(trimmed)
                    }
                }
                _ => Err(feuilletage::Error::TypeMismatch {
                    expected: "string".to_string(),
                    actual: v.type_name().to_string(),
                    path: tracker.current_path(),
                }),
            };
            tracker.pop();
            result?
        } else {
            tracker.push_field("id");
            let path = tracker.current_path();
            tracker.pop();
            return Err(feuilletage::Error::MissingField { path });
        };

        // Required: prompt
        let prompt = if let Some(v) = table.get("prompt") {
            tracker.push_field("prompt");
            let result = match v {
                feuilletage::ContextValue::String(s, _) => {
                    let trimmed = s.trim().to_string();
                    if trimmed.is_empty() {
                        Err(feuilletage::Error::InvalidValue {
                            message: "prompt cannot be empty".to_string(),
                            path: tracker.current_path(),
                        })
                    } else {
                        Ok(trimmed)
                    }
                }
                _ => Err(feuilletage::Error::TypeMismatch {
                    expected: "string".to_string(),
                    actual: v.type_name().to_string(),
                    path: tracker.current_path(),
                }),
            };
            tracker.pop();
            result?
        } else {
            tracker.push_field("prompt");
            let path = tracker.current_path();
            tracker.pop();
            return Err(feuilletage::Error::MissingField { path });
        };

        // Optional: type (defaults to text)
        let prompt_type =
            <PromptType as feuilletage::FromContextValue<S, L>>::from_context_value(value, tracker)?;

        // Optional: default
        let default = if let Some(v) = table.get("default") {
            FeuilletageValue::from(v)
        } else {
            FeuilletageValue::Null
        };

        // Optional: scope
        let scope =
            <PromptScope as feuilletage::FromContextValue<S, L>>::from_context_value(value, tracker)?;

        // Optional: if
        let if_condition = if let Some(v) = table.get("if") {
            match v {
                feuilletage::ContextValue::String(s, _) => Some(s.clone()),
                feuilletage::ContextValue::Bool(b, _) => Some(b.to_string()),
                feuilletage::ContextValue::Int(i, _) => Some(i.to_string()),
                _ => None,
            }
        } else {
            None
        };

        Ok(Self {
            id,
            prompt,
            default,
            prompt_type,
            scope,
            if_condition,
        })
    }
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel> feuilletage::FromContextValue<S, L>
    for PromptScope
{
    fn from_context_value(
        value: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        let table = match value {
            feuilletage::ContextValue::Object(map, _) => map,
            _ => return Ok(Self::default()),
        };

        let scope_value = match table.get("scope") {
            Some(v) => v,
            None => return Ok(Self::default()),
        };

        match scope_value {
            feuilletage::ContextValue::String(s, _) => {
                let scope = s.trim().to_lowercase();
                match scope.as_str() {
                    "repo" | "repository" => Ok(Self::Repository),
                    "org" | "organization" => Ok(Self::Organization),
                    _ => {
                        tracker.push_field("scope");
                        tracker.record(feuilletage::Error::InvalidValue {
                            message: format!(
                                "invalid scope '{}': expected 'repo' or 'org'",
                                scope
                            ),
                            path: tracker.current_path(),
                        });
                        tracker.pop();
                        Ok(Self::default())
                    }
                }
            }
            _ => {
                tracker.push_field("scope");
                tracker.record(feuilletage::Error::TypeMismatch {
                    expected: "string".to_string(),
                    actual: scope_value.type_name().to_string(),
                    path: tracker.current_path(),
                });
                tracker.pop();
                Ok(Self::default())
            }
        }
    }
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel> feuilletage::FromContextValue<S, L>
    for PromptType
{
    fn from_context_value(
        value: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        let table = match value {
            feuilletage::ContextValue::Object(map, _) => map,
            _ => return Ok(Self::default()),
        };

        let type_value = match table.get("type") {
            Some(v) => v,
            None => return Ok(Self::default()),
        };

        let type_str = match type_value {
            feuilletage::ContextValue::String(s, _) => s.trim().to_lowercase(),
            _ => {
                tracker.push_field("type");
                tracker.record(feuilletage::Error::TypeMismatch {
                    expected: "string".to_string(),
                    actual: type_value.type_name().to_string(),
                    path: tracker.current_path(),
                });
                tracker.pop();
                return Ok(Self::default());
            }
        };

        if type_str.is_empty() {
            tracker.push_field("type");
            tracker.record(feuilletage::Error::InvalidValue {
                message: "type cannot be empty".to_string(),
                path: tracker.current_path(),
            });
            tracker.pop();
            return Ok(Self::default());
        }

        match type_str.as_str() {
            "text" => Ok(Self::Text),
            "password" => Ok(Self::Password),
            "confirm" | "boolean" => Ok(Self::Confirm),
            "choice" | "select" | "choices" | "multichoice" | "multiselect" => {
                if let Some(choices_value) = table.get("choices") {
                    tracker.push_field("choices");
                    let choices = <PromptChoicesConfig as feuilletage::FromContextValue<S, L>>::from_context_value(
                        choices_value,
                        tracker,
                    )?;
                    tracker.pop();

                    match type_str.as_str() {
                        "choice" | "select" => Ok(Self::Choice { choices }),
                        _ => Ok(Self::MultiChoice { choices }),
                    }
                } else {
                    tracker.push_field("choices");
                    let path = tracker.current_path();
                    tracker.pop();
                    Err(feuilletage::Error::MissingField { path })
                }
            }
            "int" => {
                let min = if let Some(v) = table.get("min") {
                    match v {
                        feuilletage::ContextValue::Int(i, _) => Some(*i),
                        _ => None,
                    }
                } else {
                    None
                };
                let max = if let Some(v) = table.get("max") {
                    match v {
                        feuilletage::ContextValue::Int(i, _) => Some(*i),
                        _ => None,
                    }
                } else {
                    None
                };
                Ok(Self::Int { min, max })
            }
            "float" => {
                let min = if let Some(v) = table.get("min") {
                    match v {
                        feuilletage::ContextValue::Float(f, _) => Some(*f),
                        feuilletage::ContextValue::Int(i, _) => Some(*i as f64),
                        _ => None,
                    }
                } else {
                    None
                };
                let max = if let Some(v) = table.get("max") {
                    match v {
                        feuilletage::ContextValue::Float(f, _) => Some(*f),
                        feuilletage::ContextValue::Int(i, _) => Some(*i as f64),
                        _ => None,
                    }
                } else {
                    None
                };
                Ok(Self::Float { min, max })
            }
            _ => {
                tracker.push_field("type");
                tracker.record(feuilletage::Error::InvalidValue {
                    message: format!(
                        "invalid type '{}': expected text, password, confirm, choice, multichoice, int, or float",
                        type_str
                    ),
                    path: tracker.current_path(),
                });
                tracker.pop();
                Ok(Self::default())
            }
        }
    }
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel> feuilletage::FromContextValue<S, L>
    for PromptChoicesConfig
{
    fn from_context_value(
        value: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        match value {
            feuilletage::ContextValue::Array(arr, _) => {
                let mut choices = Vec::new();
                for (idx, item) in arr.iter().enumerate() {
                    tracker.push_index(idx);
                    match <PromptChoiceConfig as feuilletage::FromContextValue<S, L>>::from_context_value(
                        item, tracker,
                    ) {
                        Ok(choice) => choices.push(choice),
                        Err(e) => tracker.record(e),
                    }
                    tracker.pop();
                }

                if choices.is_empty() {
                    Err(feuilletage::Error::InvalidValue {
                        message: "choices cannot be empty".to_string(),
                        path: tracker.current_path(),
                    })
                } else {
                    Ok(Self::ChoicesAsArray(choices))
                }
            }
            feuilletage::ContextValue::String(s, _) => Ok(Self::ChoicesAsString(s.clone())),
            _ => Err(feuilletage::Error::TypeMismatch {
                expected: "array or template string".to_string(),
                actual: value.type_name().to_string(),
                path: tracker.current_path(),
            }),
        }
    }
}
