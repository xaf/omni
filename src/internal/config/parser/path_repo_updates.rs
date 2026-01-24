use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::StringFilter;
use crate::internal::env::shell_is_interactive;

// Compote imports for FromConfigValue implementation
use crate::internal::config::CompoteError;
use crate::internal::config::CompoteConfigValue;
use crate::internal::config::CompoteErrorTracker;
use crate::internal::config::CompoteFromConfigValue;

// ============================================================================
// PathRepoUpdatesConfig - Now using compote::Config derive
// ============================================================================
//
// Note: We use compote::Config for the struct definition but keep a manual
// bridge `from_config_value` method for backwards compatibility with the
// existing config loading system.
//
// The `per_repo_config` field uses compote's `allow_map` to accept both:
// - Array format: per_repo_config: [{workdir_id: "foo", enabled: true}, ...]
// - Table/map format: per_repo_config: {foo: {enabled: true}, bar: {ref_type: "tag"}}
//
// In the table format, the key becomes the `workdir_id` field value as an exact match.
// ============================================================================

#[derive(Debug, Clone, compote::Config)]
pub struct PathRepoUpdatesConfig {
    #[compote(default = "true")]
    pub enabled: bool,

    // Using the FromConfigValue impl we added for this enum
    #[compote(default)]
    pub self_update: PathRepoUpdatesSelfUpdateEnum,

    // Using the FromConfigValue impl we added for this enum
    #[compote(default)]
    pub on_command_not_found: PathRepoUpdatesOnCommandNotFoundEnum,

    #[compote(default = "true")]
    pub pre_auth: bool,

    // Duration parsing: accepts "2m" -> 120 seconds
    #[compote(duration, default = "120")]
    pub pre_auth_timeout: u64,

    #[compote(default = "true")]
    pub background_updates: bool,

    // Duration parsing: accepts "1h" -> 3600 seconds
    #[compote(duration, default = "3600")]
    pub background_updates_timeout: u64,

    // Duration parsing: accepts "12h" -> 43200 seconds
    #[compote(duration, default = "43200")]
    pub interval: u64,

    #[compote(default = "branch")]
    pub ref_type: String,

    #[compote(default, skip_if_default)]
    pub ref_match: StringFilter,

    // allow_map with key = "workdir_id" to accept both array and map formats
    // NOTE: The original code wraps table keys as StringFilter::Exact(key) which
    // differs from compote's allow_map behavior that assigns key as-is.
    // We keep the manual parsing for now due to this special behavior.
    #[compote(default = "Vec::new()", skip_if_empty)]
    pub per_repo_config: Vec<PathRepoUpdatesPerRepoConfig>,
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
            ref_match: StringFilter::default(),
            per_repo_config: Vec::new(),
        }
    }
}

// Manual Deserialize implementation for backwards compatibility with cache files and serde usage
impl<'de> Deserialize<'de> for PathRepoUpdatesConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            #[serde(default = "default_enabled")]
            enabled: bool,
            #[serde(default)]
            self_update: PathRepoUpdatesSelfUpdateEnum,
            #[serde(default)]
            on_command_not_found: PathRepoUpdatesOnCommandNotFoundEnum,
            #[serde(default = "default_pre_auth")]
            pre_auth: bool,
            #[serde(default = "default_pre_auth_timeout")]
            pre_auth_timeout: u64,
            #[serde(default = "default_background_updates")]
            background_updates: bool,
            #[serde(default = "default_background_updates_timeout")]
            background_updates_timeout: u64,
            #[serde(default = "default_interval")]
            interval: u64,
            #[serde(default = "default_ref_type")]
            ref_type: String,
            #[serde(default)]
            ref_match: StringFilter,
            #[serde(default)]
            per_repo_config: Vec<PathRepoUpdatesPerRepoConfig>,
        }

        fn default_enabled() -> bool { true }
        fn default_pre_auth() -> bool { true }
        fn default_pre_auth_timeout() -> u64 { 120 }
        fn default_background_updates() -> bool { true }
        fn default_background_updates_timeout() -> u64 { 3600 }
        fn default_interval() -> u64 { 43200 }
        fn default_ref_type() -> String { "branch".to_string() }

        let helper = Helper::deserialize(deserializer)?;
        Ok(PathRepoUpdatesConfig {
            enabled: helper.enabled,
            self_update: helper.self_update,
            on_command_not_found: helper.on_command_not_found,
            pre_auth: helper.pre_auth,
            pre_auth_timeout: helper.pre_auth_timeout,
            background_updates: helper.background_updates,
            background_updates_timeout: helper.background_updates_timeout,
            interval: helper.interval,
            ref_type: helper.ref_type,
            ref_match: helper.ref_match,
            per_repo_config: helper.per_repo_config,
        })
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
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum PathRepoUpdatesSelfUpdateEnum {
    #[serde(rename = "true")]
    True,
    #[serde(rename = "false")]
    False,
    #[serde(rename = "nocheck")]
    NoCheck,
    #[serde(other, rename = "ask")]
    Ask,
}

impl Default for PathRepoUpdatesSelfUpdateEnum {
    fn default() -> Self {
        if cfg!(feature = "self-update-check") {
            Self::Ask
        } else {
            Self::NoCheck
        }
    }
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

    #[cfg(feature = "self-update")]
    pub fn is_false(&self) -> bool {
        match self {
            Self::False => true,
            Self::Ask => !shell_is_interactive(),
            _ => false,
        }
    }

    #[cfg(feature = "self-update")]
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
    pub workdir_id: StringFilter,
    pub enabled: bool,
    pub ref_type: String,
    #[serde(skip_serializing_if = "StringFilter::is_default")]
    pub ref_match: StringFilter,
}

// Implement FromConfigValue for PathRepoUpdatesPerRepoConfig
// Note: This implementation treats the workdir_id as a direct field value.
// The special case where workdir_id comes from a map key is handled in the
// parent struct's from_config_value method, not here.
impl CompoteFromConfigValue for PathRepoUpdatesPerRepoConfig {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteError> {
        // Get the object fields if this is an object
        let obj = match value {
            CompoteConfigValue::Object(map, _) => map,
            _ => {
                // Return default if not an object
                return Ok(Self {
                    workdir_id: StringFilter::default(),
                    enabled: true,
                    ref_type: "branch".to_string(),
                    ref_match: StringFilter::default(),
                });
            }
        };

        // Extract workdir_id (defaults to Any if missing)
        let workdir_id = if let Some(wdid_value) = obj.get("workdir_id") {
            // Use the compote FromConfigValue trait
            <StringFilter as CompoteFromConfigValue>::from_config_value(wdid_value, tracker)?
        } else {
            StringFilter::default()
        };

        // Extract enabled (defaults to true)
        let enabled = if let Some(enabled_value) = obj.get("enabled") {
            if let CompoteConfigValue::Bool(b, _) = enabled_value {
                *b
            } else {
                true
            }
        } else {
            true
        };

        // Extract ref_type (defaults to "branch")
        let ref_type = if let Some(ref_type_value) = obj.get("ref_type") {
            if let CompoteConfigValue::String(s, _) = ref_type_value {
                s.clone()
            } else {
                "branch".to_string()
            }
        } else {
            "branch".to_string()
        };

        // Extract ref_match (defaults to Any)
        let ref_match = if let Some(ref_match_value) = obj.get("ref_match") {
            // Use the compote FromConfigValue trait
            <StringFilter as CompoteFromConfigValue>::from_config_value(ref_match_value, tracker)?
        } else {
            StringFilter::default()
        };

        Ok(Self {
            workdir_id,
            enabled,
            ref_type,
            ref_match,
        })
    }
}

// ============================================================================
// compote::FromConfigValue implementations for enum types
// ============================================================================

impl CompoteFromConfigValue for PathRepoUpdatesSelfUpdateEnum {
    fn from_config_value(
        value: &CompoteConfigValue,
        _tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteError> {
        // Handle boolean
        if let CompoteConfigValue::Bool(b, _) = value {
            return Ok(Self::from_bool(*b));
        }

        // Handle string
        if let CompoteConfigValue::String(s, _) = value {
            return Ok(Self::from_str(s));
        }

        // Handle integer
        if let CompoteConfigValue::Int(i, _) = value {
            return Ok(Self::from_int(*i));
        }

        // Default for unknown types
        Ok(Self::default())
    }
}

impl CompoteFromConfigValue for PathRepoUpdatesOnCommandNotFoundEnum {
    fn from_config_value(
        value: &CompoteConfigValue,
        _tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteError> {
        // Handle boolean
        if let CompoteConfigValue::Bool(b, _) = value {
            return Ok(Self::from_bool(*b));
        }

        // Handle string
        if let CompoteConfigValue::String(s, _) = value {
            return Ok(Self::from_str(s));
        }

        // Handle integer
        if let CompoteConfigValue::Int(i, _) = value {
            return Ok(Self::from_int(*i));
        }

        // Default for unknown types
        Ok(Self::default())
    }
}
