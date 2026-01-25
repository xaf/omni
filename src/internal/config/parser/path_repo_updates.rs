use serde::Deserialize;
// Note: Serialize is no longer manually imported as compote::Config generates Serialize impls

use crate::internal::config::parser::StringFilter;
use crate::internal::env::shell_is_interactive;

// ============================================================================
// PathRepoUpdatesConfig - Now using compote::Config derive
// ============================================================================
//
// Note: We use compote::Config for the struct definition but keep a manual
// bridge `from_context_value` method for backwards compatibility with the
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

/// PathRepoUpdatesSelfUpdateEnum using compote::Config value_matched derive.
///
/// Accepts: Bool, String, or Int values
/// - Boolean: true -> True, false -> False
/// - String: "true"/"yes"/"y" -> True, "false"/"no"/"n" -> False, "nocheck" -> NoCheck, "ask" -> Ask
/// - Integer: 0 -> False, 1 -> True, other -> Ask (fallback)
#[derive(Debug, Clone, PartialEq, compote::Config)]
#[compote(value_matched)]
pub enum PathRepoUpdatesSelfUpdateEnum {
    #[compote(variant = true | "true" | "yes" | "y" | 1)]
    True,
    #[compote(variant = false | "false" | "no" | "n" | 0)]
    False,
    #[compote(variant = "nocheck")]
    NoCheck,
    #[compote(variant = "ask", fallback)]
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

// Serialize implementation is generated by compote::Config
// The value_matched derive serializes using the first match value:
// True -> "true", False -> "false", NoCheck -> "nocheck", Ask -> "ask"

// Manual Deserialize implementation for backwards compatibility with cache files
impl<'de> Deserialize<'de> for PathRepoUpdatesSelfUpdateEnum {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Bool(bool),
            String(String),
            Int(i64),
        }

        match Helper::deserialize(deserializer)? {
            Helper::Bool(b) => Ok(Self::from_bool(b)),
            Helper::String(s) => Ok(Self::from_str(&s)),
            Helper::Int(i) => Ok(Self::from_int(i)),
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

/// PathRepoUpdatesOnCommandNotFoundEnum using compote::Config value_matched derive.
///
/// Accepts: Bool, String, or Int values
/// - Boolean: true -> True, false -> False
/// - String: "true"/"yes"/"y" -> True, "false"/"no"/"n" -> False, "ask" -> Ask
/// - Integer: 0 -> False, 1 -> True, other -> Ask (fallback)
#[derive(Debug, Clone, PartialEq, Default, compote::Config)]
#[compote(value_matched)]
pub enum PathRepoUpdatesOnCommandNotFoundEnum {
    #[compote(variant = true | "true" | "yes" | "y" | 1)]
    True,
    #[compote(variant = false | "false" | "no" | "n" | 0)]
    False,
    #[default]
    #[compote(variant = "ask", fallback)]
    Ask,
}

// Serialize implementation is generated by compote::Config
// The value_matched derive serializes using the first match value:
// True -> "true", False -> "false", Ask -> "ask"

// Manual Deserialize implementation for backwards compatibility with cache files
impl<'de> Deserialize<'de> for PathRepoUpdatesOnCommandNotFoundEnum {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Bool(bool),
            String(String),
            Int(i64),
        }

        match Helper::deserialize(deserializer)? {
            Helper::Bool(b) => Ok(Self::from_bool(b)),
            Helper::String(s) => Ok(Self::from_str(&s)),
            Helper::Int(i) => Ok(Self::from_int(i)),
        }
    }
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

/// PathRepoUpdatesPerRepoConfig using compote::Config derive.
///
/// Note: The workdir_id field is treated as a direct field value here.
/// The special case where workdir_id comes from a map key is handled in the
/// parent struct's allow_map attribute.
#[derive(Debug, Clone, compote::Config)]
pub struct PathRepoUpdatesPerRepoConfig {
    #[compote(default)]
    pub workdir_id: StringFilter,

    #[compote(default = "true")]
    pub enabled: bool,

    #[compote(default = "branch")]
    pub ref_type: String,

    #[compote(default, skip_if_default)]
    pub ref_match: StringFilter,
}

// Serialize implementation is generated by compote::Config
// The derive handles skip_if_default for ref_match automatically

// Manual Deserialize implementation for backwards compatibility with cache files
impl<'de> Deserialize<'de> for PathRepoUpdatesPerRepoConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            #[serde(default)]
            workdir_id: StringFilter,
            #[serde(default = "default_enabled")]
            enabled: bool,
            #[serde(default = "default_ref_type")]
            ref_type: String,
            #[serde(default)]
            ref_match: StringFilter,
        }

        fn default_enabled() -> bool { true }
        fn default_ref_type() -> String { "branch".to_string() }

        let helper = Helper::deserialize(deserializer)?;
        Ok(PathRepoUpdatesPerRepoConfig {
            workdir_id: helper.workdir_id,
            enabled: helper.enabled,
            ref_type: helper.ref_type,
            ref_match: helper.ref_match,
        })
    }
}

// ============================================================================
// compote::FromContextValue implementations for enum types
// ============================================================================
// Now handled by the #[derive(compote::Config)] macro with #[compote(value_matched)]

// PathRepoUpdatesOnCommandNotFoundEnum FromContextValue is now handled by
// the #[derive(compote::Config)] macro with #[compote(value_matched)]
