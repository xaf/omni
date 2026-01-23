use compote::Value as CompoteValue;
use serde::Deserialize;
use serde::Serialize;

use tera::Tera;

use crate::internal::cache::utils::Empty;
use crate::internal::cache::PromptsCache;
use crate::internal::config::template::config_template_context;
use crate::internal::config::template::render_config_template;
use crate::internal::config::template::tera_render_error_message;
use crate::internal::git_env;
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

#[derive(Default, Debug, Deserialize, Clone)]
pub struct PromptsConfig {
    #[serde(flatten)]
    pub prompts: Vec<PromptConfig>,
}

impl Empty for PromptsConfig {
    fn is_empty(&self) -> bool {
        self.prompts.is_empty()
    }
}

impl compote::IsEmpty for PromptsConfig {
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptConfig {
    pub id: String,
    pub prompt: String,
    #[serde(
        skip_serializing_if = "compote_value_is_null",
        default = "compote_value_null"
    )]
    pub default: CompoteValue,
    #[serde(
        flatten,
        skip_serializing_if = "PromptType::is_default",
        default = "PromptType::default"
    )]
    pub prompt_type: PromptType,
    #[serde(
        skip_serializing_if = "PromptScope::is_default",
        default = "PromptScope::default"
    )]
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
        let prompt_key = format!("prompt.{}", &self.id);
        if let Err(err) = template.add_raw_template(&prompt_key, yaml.as_str()) {
            return Err(tera_render_error_message(err));
        }

        match render_config_template(&template, &template_context) {
            Ok(value) => {
                // Parse YAML using compote
                let context = CompoteConfigContext::new(
                    CompoteConfigSource::Programmatic,
                    CompoteConfigLevel::Local,
                );
                let config_value = match compote::loader::load_yaml(&value, context) {
                    Ok(cv) => cv,
                    Err(err) => {
                        return Err(format!(
                            "failed to parse prompt {} as yaml: {}",
                            &self.id, err
                        ))
                    }
                };

                // Use compote's FromConfigValue implementation
                let mut tracker = CompoteErrorTracker::new();
                match <Self as CompoteFromConfigValue>::from_config_value(&config_value, &mut tracker) {
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

#[derive(Default, Debug, Serialize, Deserialize, Clone, Copy)]
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

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
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
        default: CompoteValue,
        scope: PromptScope,
    ) -> bool {
        // Override the default value with the cached answer if there is one
        // for the current scope; otherwise, use the default value.
        // The cache returns serde_json::Value, so we convert it to CompoteValue.
        let default = match PromptsCache::get().answers(".").get(id) {
            Some(answer) => json_value_to_compote_value(answer),
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
                    let defaults: Vec<CompoteValue> = match default.clone() {
                        CompoteValue::Array(defaults) => defaults,
                        CompoteValue::String(_) => vec![default],
                        CompoteValue::Int(_) => vec![default],
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

#[derive(Debug, Deserialize, Clone)]
pub enum PromptChoicesConfig {
    ChoicesAsArray(Vec<PromptChoiceConfig>),
    ChoicesAsString(String),
}

impl PromptChoicesConfig {
    pub fn choices(&self) -> Result<Vec<PromptChoiceConfig>, String> {
        match self {
            Self::ChoicesAsArray(choices) => Ok(choices.clone()),
            Self::ChoicesAsString(template) => {
                // Parse YAML using compote
                let context = CompoteConfigContext::new(
                    CompoteConfigSource::Programmatic,
                    CompoteConfigLevel::Local,
                );
                let config_value = match compote::loader::load_yaml(template, context) {
                    Ok(cv) => cv,
                    Err(err) => {
                        return Err(format!("failed to parse choices template as yaml: {err}"));
                    }
                };

                // Convert to Vec<PromptChoiceConfig> using compote's FromConfigValue
                let mut tracker = CompoteErrorTracker::new();
                match <Vec<PromptChoiceConfig> as CompoteFromConfigValue>::from_config_value(
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PromptChoiceConfig {
    pub id: String,
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
// CompoteValue helper functions
// ============================================================================

/// Helper function for serde skip_serializing_if
fn compote_value_is_null(value: &CompoteValue) -> bool {
    matches!(value, CompoteValue::Null)
}

/// Helper function for serde default
fn compote_value_null() -> CompoteValue {
    CompoteValue::Null
}

/// Convert serde_json::Value to compote::ContextValue
fn json_value_to_compote_context_value(value: &serde_json::Value) -> compote::ContextValue {
    let ctx = compote::Context::new(
        compote::Source::Programmatic,
        compote::Level::Local,
    );
    match value {
        serde_json::Value::Null => compote::ContextValue::null(ctx),
        serde_json::Value::Bool(b) => compote::ContextValue::bool(*b, ctx),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                compote::ContextValue::int(i, ctx)
            } else if let Some(f) = n.as_f64() {
                compote::ContextValue::float(f, ctx)
            } else {
                compote::ContextValue::null(ctx)
            }
        }
        serde_json::Value::String(s) => compote::ContextValue::string(s.clone(), ctx),
        serde_json::Value::Array(arr) => {
            let items: Vec<compote::ContextValue> = arr
                .iter()
                .map(json_value_to_compote_context_value)
                .collect();
            compote::ContextValue::array(items, ctx)
        }
        serde_json::Value::Object(map) => {
            let items: indexmap::IndexMap<String, compote::ContextValue> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_value_to_compote_context_value(v)))
                .collect();
            compote::ContextValue::object(items, ctx)
        }
    }
}

/// Convert serde_json::Value to compote::Value
fn json_value_to_compote_value(value: &serde_json::Value) -> CompoteValue {
    match value {
        serde_json::Value::Null => CompoteValue::Null,
        serde_json::Value::Bool(b) => CompoteValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                CompoteValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                CompoteValue::Float(f)
            } else {
                CompoteValue::Null
            }
        }
        serde_json::Value::String(s) => CompoteValue::String(s.clone()),
        serde_json::Value::Array(arr) => {
            let items: Vec<CompoteValue> = arr
                .iter()
                .map(json_value_to_compote_value)
                .collect();
            CompoteValue::Array(items)
        }
        serde_json::Value::Object(map) => {
            let items: indexmap::IndexMap<String, CompoteValue> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_value_to_compote_value(v)))
                .collect();
            CompoteValue::Object(items)
        }
    }
}

// Helper trait to add methods to CompoteValue (compote::Value)
// Note: as_str() is already provided by Value natively, so not included here
trait CompoteValueExt {
    fn is_null(&self) -> bool;
    fn as_bool(&self) -> Option<bool>;
    fn as_i64(&self) -> Option<i64>;
    fn as_f64(&self) -> Option<f64>;
}

impl CompoteValueExt for CompoteValue {
    fn is_null(&self) -> bool {
        matches!(self, CompoteValue::Null)
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            CompoteValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    fn as_i64(&self) -> Option<i64> {
        match self {
            CompoteValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            CompoteValue::Float(f) => Some(*f),
            CompoteValue::Int(i) => Some(*i as f64),
            _ => None,
        }
    }
}

// ============================================================================
// Compote FromConfigValue implementations
// ============================================================================

impl CompoteFromConfigValue for PromptsConfig {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteConfigError> {
        match value {
            CompoteConfigValue::Array(arr, _) => {
                let mut prompts = Vec::new();
                for (idx, item) in arr.iter().enumerate() {
                    tracker.push_index(idx);
                    match <PromptConfig as CompoteFromConfigValue>::from_config_value(item, tracker) {
                        Ok(prompt) => prompts.push(prompt),
                        Err(e) => tracker.record(e),
                    }
                    tracker.pop();
                }
                Ok(Self { prompts })
            }
            CompoteConfigValue::Null(_) => Ok(Self::default()),
            _ => Err(CompoteConfigError::TypeMismatch {
                expected: "array".to_string(),
                actual: value.type_name().to_string(),
                path: tracker.current_path(),
            }),
        }
    }
}

impl CompoteFromConfigValue for PromptConfig {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteConfigError> {
        let table = match value {
            CompoteConfigValue::Object(map, _) => map,
            _ => {
                return Err(CompoteConfigError::TypeMismatch {
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
                CompoteConfigValue::String(s, _) => {
                    let trimmed = s.trim().to_string();
                    if trimmed.is_empty() {
                        Err(CompoteConfigError::InvalidValue {
                            message: "id cannot be empty".to_string(),
                            path: tracker.current_path(),
                        })
                    } else {
                        Ok(trimmed)
                    }
                }
                _ => Err(CompoteConfigError::TypeMismatch {
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
            return Err(CompoteConfigError::MissingField { path });
        };

        // Required: prompt
        let prompt = if let Some(v) = table.get("prompt") {
            tracker.push_field("prompt");
            let result = match v {
                CompoteConfigValue::String(s, _) => {
                    let trimmed = s.trim().to_string();
                    if trimmed.is_empty() {
                        Err(CompoteConfigError::InvalidValue {
                            message: "prompt cannot be empty".to_string(),
                            path: tracker.current_path(),
                        })
                    } else {
                        Ok(trimmed)
                    }
                }
                _ => Err(CompoteConfigError::TypeMismatch {
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
            return Err(CompoteConfigError::MissingField { path });
        };

        // Optional: type (defaults to text)
        let prompt_type = <PromptType as CompoteFromConfigValue>::from_config_value(value, tracker)?;

        // Optional: default
        let default = if let Some(v) = table.get("default") {
            CompoteValue::from(v)
        } else {
            CompoteValue::Null
        };

        // Optional: scope
        let scope = <PromptScope as CompoteFromConfigValue>::from_config_value(value, tracker)?;

        // Optional: if
        let if_condition = if let Some(v) = table.get("if") {
            match v {
                CompoteConfigValue::String(s, _) => Some(s.clone()),
                CompoteConfigValue::Bool(b, _) => Some(b.to_string()),
                CompoteConfigValue::Int(i, _) => Some(i.to_string()),
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

impl CompoteFromConfigValue for PromptScope {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteConfigError> {
        let table = match value {
            CompoteConfigValue::Object(map, _) => map,
            _ => return Ok(Self::default()),
        };

        let scope_value = match table.get("scope") {
            Some(v) => v,
            None => return Ok(Self::default()),
        };

        match scope_value {
            CompoteConfigValue::String(s, _) => {
                let scope = s.trim().to_lowercase();
                match scope.as_str() {
                    "repo" | "repository" => Ok(Self::Repository),
                    "org" | "organization" => Ok(Self::Organization),
                    _ => {
                        tracker.push_field("scope");
                        tracker.record(CompoteConfigError::InvalidValue {
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
                tracker.record(CompoteConfigError::TypeMismatch {
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

impl CompoteFromConfigValue for PromptType {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteConfigError> {
        let table = match value {
            CompoteConfigValue::Object(map, _) => map,
            _ => return Ok(Self::default()),
        };

        let type_value = match table.get("type") {
            Some(v) => v,
            None => return Ok(Self::default()),
        };

        let type_str = match type_value {
            CompoteConfigValue::String(s, _) => s.trim().to_lowercase(),
            _ => {
                tracker.push_field("type");
                tracker.record(CompoteConfigError::TypeMismatch {
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
            tracker.record(CompoteConfigError::InvalidValue {
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
                    let choices = <PromptChoicesConfig as CompoteFromConfigValue>::from_config_value(
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
                    Err(CompoteConfigError::MissingField { path })
                }
            }
            "int" => {
                let min = if let Some(v) = table.get("min") {
                    match v {
                        CompoteConfigValue::Int(i, _) => Some(*i),
                        _ => None,
                    }
                } else {
                    None
                };
                let max = if let Some(v) = table.get("max") {
                    match v {
                        CompoteConfigValue::Int(i, _) => Some(*i),
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
                        CompoteConfigValue::Float(f, _) => Some(*f),
                        CompoteConfigValue::Int(i, _) => Some(*i as f64),
                        _ => None,
                    }
                } else {
                    None
                };
                let max = if let Some(v) = table.get("max") {
                    match v {
                        CompoteConfigValue::Float(f, _) => Some(*f),
                        CompoteConfigValue::Int(i, _) => Some(*i as f64),
                        _ => None,
                    }
                } else {
                    None
                };
                Ok(Self::Float { min, max })
            }
            _ => {
                tracker.push_field("type");
                tracker.record(CompoteConfigError::InvalidValue {
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

impl CompoteFromConfigValue for PromptChoicesConfig {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteConfigError> {
        match value {
            CompoteConfigValue::Array(arr, _) => {
                let mut choices = Vec::new();
                for (idx, item) in arr.iter().enumerate() {
                    tracker.push_index(idx);
                    match <PromptChoiceConfig as CompoteFromConfigValue>::from_config_value(
                        item, tracker,
                    ) {
                        Ok(choice) => choices.push(choice),
                        Err(e) => tracker.record(e),
                    }
                    tracker.pop();
                }

                if choices.is_empty() {
                    Err(CompoteConfigError::InvalidValue {
                        message: "choices cannot be empty".to_string(),
                        path: tracker.current_path(),
                    })
                } else {
                    Ok(Self::ChoicesAsArray(choices))
                }
            }
            CompoteConfigValue::String(s, _) => Ok(Self::ChoicesAsString(s.clone())),
            _ => Err(CompoteConfigError::TypeMismatch {
                expected: "array or template string".to_string(),
                actual: value.type_name().to_string(),
                path: tracker.current_path(),
            }),
        }
    }
}

impl CompoteFromConfigValue for PromptChoiceConfig {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteConfigError> {
        match value {
            CompoteConfigValue::Object(table, _) => {
                let id = table.get("id").and_then(|v| match v {
                    CompoteConfigValue::String(s, _) => Some(s.clone()),
                    _ => None,
                });
                let choice = table.get("choice").and_then(|v| match v {
                    CompoteConfigValue::String(s, _) => Some(s.clone()),
                    _ => None,
                });

                match (id, choice) {
                    (Some(id), Some(choice)) => Ok(Self { id, choice }),
                    (Some(id), None) => Ok(Self {
                        id: id.clone(),
                        choice: id,
                    }),
                    (None, Some(choice)) => Ok(Self {
                        id: choice.clone(),
                        choice,
                    }),
                    (None, None) => Err(CompoteConfigError::InvalidValue {
                        message: "choice must have 'id' or 'choice' field".to_string(),
                        path: tracker.current_path(),
                    }),
                }
            }
            CompoteConfigValue::String(s, _) => Ok(Self {
                id: s.clone(),
                choice: s.clone(),
            }),
            _ => Err(CompoteConfigError::TypeMismatch {
                expected: "table or string".to_string(),
                actual: value.type_name().to_string(),
                path: tracker.current_path(),
            }),
        }
    }
}
