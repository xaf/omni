use std::collections::HashMap;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::utils::parse_duration_or_default;
use crate::internal::config::ConfigValue;
use crate::internal::env::shell_is_interactive;
use crate::internal::git::update_git_repo;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PathRepoUpdatesConfig {
    pub enabled: bool,
    pub self_update: PathRepoUpdatesSelfUpdateEnum,
    pub on_command_not_found: PathRepoUpdatesOnCommandNotFoundEnum,
    pub pre_auth: bool,
    pub pre_auth_timeout: u64,
    pub background_updates: bool,
    pub background_updates_timeout: u64,
    pub interval: u64,
    pub ref_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_match: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub per_repo_config: HashMap<String, PathRepoUpdatesPerRepoConfig>,
}

impl Default for PathRepoUpdatesConfig {
    fn default() -> Self {
        Self {
            enabled: Self::DEFAULT_ENABLED,
            self_update: PathRepoUpdatesSelfUpdateEnum::default(),
            on_command_not_found: PathRepoUpdatesOnCommandNotFoundEnum::default(),
            pre_auth: Self::DEFAULT_PRE_AUTH,
            pre_auth_timeout: Self::DEFAULT_PRE_AUTH_TIMEOUT,
            background_updates: Self::DEFAULT_BACKGROUND_UPDATES,
            background_updates_timeout: Self::DEFAULT_BACKGROUND_UPDATES_TIMEOUT,
            interval: Self::DEFAULT_INTERVAL,
            ref_type: Self::DEFAULT_REF_TYPE.to_string(),
            ref_match: None,
            per_repo_config: HashMap::new(),
        }
    }
}

impl PathRepoUpdatesConfig {
    const DEFAULT_ENABLED: bool = true;
    const DEFAULT_PRE_AUTH: bool = true;
    const DEFAULT_PRE_AUTH_TIMEOUT: u64 = 120; // 2 minutes
    const DEFAULT_BACKGROUND_UPDATES: bool = true;
    const DEFAULT_BACKGROUND_UPDATES_TIMEOUT: u64 = 3600; // 1 hour
    const DEFAULT_INTERVAL: u64 = 43200; // 12 hours
    const DEFAULT_REF_TYPE: &'static str = "branch";

    pub(super) fn from_config_value(
        config_value: Option<ConfigValue>,
        errors: &mut Vec<ConfigErrorKind>,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let mut per_repo_config = HashMap::new();
        if let Some(value) = config_value.get("per_repo_config") {
            for (key, value) in value.as_table().unwrap() {
                per_repo_config.insert(
                    key.to_string(),
                    PathRepoUpdatesPerRepoConfig::from_config_value(&value),
                );
            }
        };

        let pre_auth_timeout = parse_duration_or_default(
            config_value.get("pre_auth_timeout").as_ref(),
            Self::DEFAULT_PRE_AUTH_TIMEOUT,
            "path_repo_updates.pre_auth_timeout",
            errors,
        );
        let background_updates_timeout = parse_duration_or_default(
            config_value.get("background_updates_timeout").as_ref(),
            Self::DEFAULT_BACKGROUND_UPDATES_TIMEOUT,
            "path_repo_updates.background_updates_timeout",
            errors,
        );
        let interval = parse_duration_or_default(
            config_value.get("interval").as_ref(),
            Self::DEFAULT_INTERVAL,
            "path_repo_updates.interval",
            errors,
        );

        let self_update = if let Some(value) = config_value.get("self_update") {
            if let Some(value) = value.as_bool() {
                PathRepoUpdatesSelfUpdateEnum::from_bool(value)
            } else if let Some(value) = value.as_str() {
                // TODO: handle errors here ?
                PathRepoUpdatesSelfUpdateEnum::from_str(&value)
            } else if let Some(value) = value.as_integer() {
                PathRepoUpdatesSelfUpdateEnum::from_int(value)
            } else {
                errors.push(ConfigErrorKind::ValueType {
                    key: "path_repo_updates.self_update".to_string(),
                    expected: "boolean, string, or integer".to_string(),
                    found: value.as_serde_yaml(),
                });
                PathRepoUpdatesSelfUpdateEnum::default()
            }
        } else {
            PathRepoUpdatesSelfUpdateEnum::default()
        };

        let on_command_not_found = if let Some(value) = config_value.get("on_command_not_found") {
            if let Some(value) = value.as_bool() {
                PathRepoUpdatesOnCommandNotFoundEnum::from_bool(value)
            } else if let Some(value) = value.as_str() {
                // TODO: handle errors here ?
                PathRepoUpdatesOnCommandNotFoundEnum::from_str(&value)
            } else if let Some(value) = value.as_integer() {
                PathRepoUpdatesOnCommandNotFoundEnum::from_int(value)
            } else {
                errors.push(ConfigErrorKind::ValueType {
                    key: "path_repo_updates.on_command_not_found".to_string(),
                    expected: "boolean, string, or integer".to_string(),
                    found: value.as_serde_yaml(),
                });
                PathRepoUpdatesOnCommandNotFoundEnum::default()
            }
        } else {
            PathRepoUpdatesOnCommandNotFoundEnum::default()
        };

        let ref_type = if let Some(value) = config_value.get("ref_type") {
            if let Some(value) = value.as_str() {
                value.to_string()
            } else {
                errors.push(ConfigErrorKind::ValueType {
                    key: "path_repo_updates.ref_type".to_string(),
                    expected: "string".to_string(),
                    found: value.as_serde_yaml(),
                });
                Self::DEFAULT_REF_TYPE.to_string()
            }
        } else {
            Self::DEFAULT_REF_TYPE.to_string()
        };

        let ref_match = if let Some(value) = config_value.get("ref_match") {
            if let Some(value) = value.as_str() {
                Some(value.to_string())
            } else {
                errors.push(ConfigErrorKind::ValueType {
                    key: "path_repo_updates.ref_match".to_string(),
                    expected: "string".to_string(),
                    found: value.as_serde_yaml(),
                });
                None
            }
        } else {
            None
        };

        Self {
            enabled: config_value.get_as_bool_or_default(
                "enabled",
                Self::DEFAULT_ENABLED,
                "path_repo_updates.enabled",
                errors,
            ),
            self_update,
            on_command_not_found,
            pre_auth: config_value.get_as_bool_or_default(
                "pre_auth",
                Self::DEFAULT_PRE_AUTH,
                "path_repo_updates.pre_auth",
                errors,
            ),
            pre_auth_timeout,
            background_updates: config_value.get_as_bool_or_default(
                "background_updates",
                Self::DEFAULT_BACKGROUND_UPDATES,
                "path_repo_updates.background_updates",
                errors,
            ),
            background_updates_timeout,
            interval,
            ref_type,
            ref_match,
            per_repo_config,
        }
    }

    pub fn update_config(&self, repo_id: &str) -> (bool, String, Option<String>) {
        match self.per_repo_config.get(repo_id) {
            Some(value) => (
                value.enabled,
                value.ref_type.clone(),
                value.ref_match.clone(),
            ),
            None => (self.enabled, self.ref_type.clone(), self.ref_match.clone()),
        }
    }

    pub fn update(&self, repo_id: &str) -> bool {
        let (enabled, ref_type, ref_match) = self.update_config(repo_id);

        if !enabled {
            return false;
        }

        update_git_repo(repo_id, ref_type, ref_match, None, None).unwrap_or(false)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum PathRepoUpdatesSelfUpdateEnum {
    #[serde(rename = "true")]
    True,
    #[serde(rename = "false")]
    False,
    #[serde(rename = "nocheck")]
    NoCheck,
    #[default]
    #[serde(other, rename = "ask")]
    Ask,
}

impl PathRepoUpdatesSelfUpdateEnum {
    pub fn from_bool(value: bool) -> Self {
        if value {
            Self::True
        } else {
            Self::False
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "true" | "yes" | "y" => Self::True,
            "false" | "no" | "n" => Self::False,
            "nocheck" => Self::NoCheck,
            "ask" => Self::Ask,
            _ => Self::default(),
        }
    }

    pub fn from_int(value: i64) -> Self {
        match value {
            0 => Self::False,
            1 => Self::True,
            _ => Self::Ask,
        }
    }

    pub fn do_not_check(&self) -> bool {
        matches!(self, PathRepoUpdatesSelfUpdateEnum::NoCheck)
    }

    pub fn is_false(&self) -> bool {
        match self {
            Self::False => true,
            Self::Ask => !shell_is_interactive(),
            _ => false,
        }
    }

    pub fn is_ask(&self) -> bool {
        match self {
            Self::Ask => shell_is_interactive(),
            _ => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum PathRepoUpdatesOnCommandNotFoundEnum {
    #[serde(rename = "true")]
    True,
    #[serde(rename = "false")]
    False,
    #[default]
    #[serde(other, rename = "ask")]
    Ask,
}

impl PathRepoUpdatesOnCommandNotFoundEnum {
    pub fn from_bool(value: bool) -> Self {
        if value {
            Self::True
        } else {
            Self::False
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value.to_lowercase().as_str() {
            "true" | "yes" | "y" => Self::True,
            "false" | "no" | "n" => Self::False,
            "ask" => Self::Ask,
            _ => Self::default(),
        }
    }

    pub fn from_int(value: i64) -> Self {
        match value {
            0 => Self::False,
            1 => Self::True,
            _ => Self::default(),
        }
    }

    pub fn is_false(&self) -> bool {
        match self {
            Self::False => true,
            Self::Ask => !shell_is_interactive(),
            _ => false,
        }
    }

    pub fn is_ask(&self) -> bool {
        match self {
            Self::Ask => shell_is_interactive(),
            _ => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PathRepoUpdatesPerRepoConfig {
    pub enabled: bool,
    pub ref_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_match: Option<String>,
}

impl PathRepoUpdatesPerRepoConfig {
    pub(super) fn from_config_value(config_value: &ConfigValue) -> Self {
        Self {
            enabled: match config_value.get("enabled") {
                Some(value) => value.as_bool().unwrap(),
                None => true,
            },
            ref_type: match config_value.get("ref_type") {
                Some(value) => value.as_str().unwrap().to_string(),
                None => "branch".to_string(),
            },
            ref_match: config_value
                .get("ref_match")
                .map(|value| value.as_str().unwrap().to_string()),
        }
    }
}
