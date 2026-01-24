use serde::ser::SerializeMap;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils::Empty;

// Compote imports
use crate::internal::config::CompoteError;
use crate::internal::config::CompoteConfigValue;
use crate::internal::config::CompoteErrorTracker;
use crate::internal::config::CompoteFromConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct GithubConfig {
    #[serde(default, rename = "auth", skip_serializing_if = "Vec::is_empty")]
    auth_list: Vec<GithubAuthConfigWithFilters>,
}

impl Empty for GithubConfig {
    fn is_empty(&self) -> bool {
        self.auth_list.is_empty()
    }
}

impl compote::IsEmpty for GithubConfig {
    fn is_empty(&self) -> bool {
        Empty::is_empty(self)
    }
}

impl GithubConfig {
    pub fn auth_for(&self, repo: &str, api_hostname: &str) -> GithubAuthConfig {
        self.auth_list
            .iter()
            .find(|auth| auth.matches(repo, api_hostname))
            .map(|auth| auth.auth.clone())
            .unwrap_or(GithubAuthConfig::default())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct GithubAuthConfigWithFilters {
    #[serde(
        default,
        with = "serde_yaml::with::singleton_map",
        skip_serializing_if = "StringFilter::is_default"
    )]
    pub repo: StringFilter,
    #[serde(
        default,
        with = "serde_yaml::with::singleton_map",
        skip_serializing_if = "StringFilter::is_default"
    )]
    pub hostname: StringFilter,
    #[serde(flatten)]
    pub auth: GithubAuthConfig,
}

impl GithubAuthConfigWithFilters {
    pub fn matches(&self, repo: &str, api_hostname: &str) -> bool {
        self.repo.matches(repo) && self.hostname.matches(api_hostname)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GithubAuthConfig {
    Token(String),
    TokenEnvVar(String),
    #[serde(rename = "gh")]
    GhCli {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hostname: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user: Option<String>,
    },
    Skip(bool),
}

impl Default for GithubAuthConfig {
    fn default() -> Self {
        GithubAuthConfig::GhCli {
            hostname: None,
            user: None,
        }
    }
}

impl GithubAuthConfig {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StringFilter {
    Contains(String),
    StartsWith(String),
    EndsWith(String),
    Regex(String),
    Glob(String),
    Exact(String),
    #[default]
    Any,
}

impl Serialize for StringFilter {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        // Serialize `any` as null, and `glob` as a string;
        // the rest is going to be a key: value pair in a map
        match self {
            StringFilter::Any => serializer.serialize_none(),
            StringFilter::Glob(pattern) => serializer.serialize_str(pattern),
            _ => {
                let mut map = serializer.serialize_map(Some(1))?;
                match self {
                    StringFilter::Contains(pattern) => {
                        map.serialize_entry("contains", pattern)?;
                    }
                    StringFilter::StartsWith(pattern) => {
                        map.serialize_entry("starts_with", pattern)?;
                    }
                    StringFilter::EndsWith(pattern) => {
                        map.serialize_entry("ends_with", pattern)?;
                    }
                    StringFilter::Regex(pattern) => {
                        map.serialize_entry("regex", pattern)?;
                    }
                    StringFilter::Exact(pattern) => {
                        map.serialize_entry("exact", pattern)?;
                    }
                    StringFilter::Any | StringFilter::Glob(_) => unreachable!(),
                }
                map.end()
            }
        }
    }
}

impl std::fmt::Display for StringFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            StringFilter::Contains(pattern) => write!(f, "contains \'{pattern}\'"),
            StringFilter::StartsWith(pattern) => write!(f, "start with \'{pattern}\'"),
            StringFilter::EndsWith(pattern) => write!(f, "end with \'{pattern}\'"),
            StringFilter::Regex(pattern) => write!(f, "match regex \'{pattern}\'"),
            StringFilter::Glob(pattern) => write!(f, "match \'{pattern}\'"),
            StringFilter::Exact(pattern) => write!(f, "be \'{pattern}\'"),
            StringFilter::Any => write!(f, "be any value"),
        }
    }
}

impl StringFilter {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    pub fn matches(&self, value: &str) -> bool {
        match self {
            StringFilter::Any => true,
            StringFilter::Contains(pattern) => {
                value.to_lowercase().contains(&pattern.to_lowercase())
            }
            StringFilter::StartsWith(pattern) => {
                value.to_lowercase().starts_with(&pattern.to_lowercase())
            }
            StringFilter::EndsWith(pattern) => {
                value.to_lowercase().ends_with(&pattern.to_lowercase())
            }
            StringFilter::Regex(pattern) => match regex::Regex::new(pattern) {
                Ok(regex) => regex.is_match(value),
                Err(_) => false,
            },
            StringFilter::Glob(pattern) => match globset::Glob::new(&pattern.to_lowercase()) {
                Ok(glob) => glob.compile_matcher().is_match(value.to_lowercase()),
                Err(_) => false,
            },
            StringFilter::Exact(pattern) => value.to_lowercase() == pattern.to_lowercase(),
        }
    }
}

// ============================================================================
// Compote FromConfigValue implementations
// ============================================================================

impl CompoteFromConfigValue for StringFilter {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteError> {
        match value {
            CompoteConfigValue::String(s, _) => {
                // If a string is provided, use it as a glob pattern by default
                Ok(StringFilter::Glob(s.clone()))
            }
            CompoteConfigValue::Null(_) => Ok(StringFilter::Any),
            CompoteConfigValue::Object(table, _) => {
                if let Some(entry) = table.get("contains") {
                    tracker.push_field("contains");
                    let result = if let CompoteConfigValue::String(s, _) = entry {
                        Ok(StringFilter::Contains(s.clone()))
                    } else {
                        tracker.record_type_mismatch("string", entry.type_name());
                        Ok(Self::default())
                    };
                    tracker.pop();
                    return result;
                }

                if let Some(entry) = table.get("starts_with") {
                    tracker.push_field("starts_with");
                    let result = if let CompoteConfigValue::String(s, _) = entry {
                        Ok(StringFilter::StartsWith(s.clone()))
                    } else {
                        tracker.record_type_mismatch("string", entry.type_name());
                        Ok(Self::default())
                    };
                    tracker.pop();
                    return result;
                }

                if let Some(entry) = table.get("ends_with") {
                    tracker.push_field("ends_with");
                    let result = if let CompoteConfigValue::String(s, _) = entry {
                        Ok(StringFilter::EndsWith(s.clone()))
                    } else {
                        tracker.record_type_mismatch("string", entry.type_name());
                        Ok(Self::default())
                    };
                    tracker.pop();
                    return result;
                }

                if let Some(entry) = table.get("regex") {
                    tracker.push_field("regex");
                    let result = if let CompoteConfigValue::String(s, _) = entry {
                        Ok(StringFilter::Regex(s.clone()))
                    } else {
                        tracker.record_type_mismatch("string", entry.type_name());
                        Ok(Self::default())
                    };
                    tracker.pop();
                    return result;
                }

                if let Some(entry) = table.get("glob") {
                    tracker.push_field("glob");
                    let result = if let CompoteConfigValue::String(s, _) = entry {
                        Ok(StringFilter::Glob(s.clone()))
                    } else {
                        tracker.record_type_mismatch("string", entry.type_name());
                        Ok(Self::default())
                    };
                    tracker.pop();
                    return result;
                }

                if let Some(entry) = table.get("exact") {
                    tracker.push_field("exact");
                    let result = if let CompoteConfigValue::String(s, _) = entry {
                        Ok(StringFilter::Exact(s.clone()))
                    } else {
                        tracker.record_type_mismatch("string", entry.type_name());
                        Ok(Self::default())
                    };
                    tracker.pop();
                    return result;
                }

                if let Some(entry) = table.get("any") {
                    tracker.push_field("any");
                    let result = match entry {
                        CompoteConfigValue::Null(_) => Ok(StringFilter::Any),
                        CompoteConfigValue::Bool(true, _) => Ok(StringFilter::Any),
                        _ => {
                            tracker.record_type_mismatch("null or bool(true)", entry.type_name());
                            Ok(Self::default())
                        }
                    };
                    tracker.pop();
                    return result;
                }

                // No recognized key found
                tracker.record_invalid_value("expected one of: contains, starts_with, ends_with, regex, glob, exact, any");
                Ok(Self::default())
            }
            _ => {
                tracker.record_type_mismatch("string, object, or null", value.type_name());
                Ok(Self::default())
            }
        }
    }
}

impl CompoteFromConfigValue for GithubAuthConfig {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteError> {
        match value {
            CompoteConfigValue::Null(_) => Ok(Self::default()),
            CompoteConfigValue::String(s, _) => {
                match s.as_str() {
                    "skip" => Ok(Self::Skip(true)),
                    "gh" => Ok(Self::default()),
                    _ => {
                        // If all caps and underscores, consider it's an environment variable
                        if s.chars().all(|c| c.is_uppercase() || c == '_') {
                            Ok(Self::TokenEnvVar(s.clone()))
                        } else {
                            Ok(Self::Token(s.clone()))
                        }
                    }
                }
            }
            CompoteConfigValue::Object(table, _) => {
                // Check for skip
                if let Some(skip_value) = table.get("skip") {
                    tracker.push_field("skip");
                    match skip_value {
                        CompoteConfigValue::Bool(true, _) => {
                            tracker.pop();
                            return Ok(Self::Skip(true));
                        }
                        CompoteConfigValue::Bool(false, _) => {
                            // Continue checking other fields
                        }
                        _ => {
                            tracker.record_type_mismatch("bool", skip_value.type_name());
                        }
                    }
                    tracker.pop();
                }

                // Check for token_env_var
                if let Some(token_env_var_value) = table.get("token_env_var") {
                    tracker.push_field("token_env_var");
                    if let CompoteConfigValue::String(s, _) = token_env_var_value {
                        tracker.pop();
                        return Ok(Self::TokenEnvVar(s.clone()));
                    } else {
                        tracker.record_type_mismatch("string", token_env_var_value.type_name());
                    }
                    tracker.pop();
                }

                // Check for token
                if let Some(token_value) = table.get("token") {
                    tracker.push_field("token");
                    if let CompoteConfigValue::String(s, _) = token_value {
                        tracker.pop();
                        return Ok(Self::Token(s.clone()));
                    } else {
                        tracker.record_type_mismatch("string", token_value.type_name());
                    }
                    tracker.pop();
                }

                // Check for gh
                if let Some(gh_value) = table.get("gh") {
                    tracker.push_field("gh");
                    let mut hostname = None;
                    let mut user = None;

                    match gh_value {
                        CompoteConfigValue::Object(gh_table, _) => {
                            if let Some(hostname_value) = gh_table.get("hostname") {
                                if let CompoteConfigValue::String(s, _) = hostname_value {
                                    hostname = Some(s.clone());
                                }
                            }
                            if let Some(user_value) = gh_table.get("user") {
                                if let CompoteConfigValue::String(s, _) = user_value {
                                    user = Some(s.clone());
                                }
                            }
                        }
                        CompoteConfigValue::String(s, _) => {
                            hostname = Some(s.clone());
                        }
                        _ => {
                            tracker.record_type_mismatch("string or object", gh_value.type_name());
                        }
                    }
                    tracker.pop();
                    return Ok(Self::GhCli { hostname, user });
                }

                // Default
                Ok(Self::default())
            }
            _ => {
                tracker.record_type_mismatch("string or object", value.type_name());
                Ok(Self::default())
            }
        }
    }
}

impl CompoteFromConfigValue for GithubAuthConfigWithFilters {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteError> {
        match value {
            CompoteConfigValue::Null(_) => Ok(Self {
                repo: StringFilter::default(),
                hostname: StringFilter::default(),
                auth: GithubAuthConfig::default(),
            }),
            CompoteConfigValue::Object(table, _) => {
                // Parse repo filter
                let repo = if let Some(repo_value) = table.get("repo") {
                    tracker.push_field("repo");
                    let result = <StringFilter as CompoteFromConfigValue>::from_config_value(repo_value, tracker)?;
                    tracker.pop();
                    result
                } else {
                    StringFilter::default()
                };

                // Parse hostname filter
                let hostname = if let Some(hostname_value) = table.get("hostname") {
                    tracker.push_field("hostname");
                    let result = <StringFilter as CompoteFromConfigValue>::from_config_value(hostname_value, tracker)?;
                    tracker.pop();
                    result
                } else {
                    StringFilter::default()
                };

                // Parse auth (from the same object, flattened)
                let auth = <GithubAuthConfig as CompoteFromConfigValue>::from_config_value(value, tracker)?;

                Ok(Self { repo, hostname, auth })
            }
            _ => {
                tracker.record_type_mismatch("object", value.type_name());
                Ok(Self {
                    repo: StringFilter::default(),
                    hostname: StringFilter::default(),
                    auth: GithubAuthConfig::default(),
                })
            }
        }
    }
}

impl CompoteFromConfigValue for GithubConfig {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteError> {
        match value {
            CompoteConfigValue::Null(_) => Ok(Self::default()),
            CompoteConfigValue::Object(table, _) => {
                let auth_list = if let Some(auth_value) = table.get("auth") {
                    tracker.push_field("auth");
                    let result = parse_auth_list(auth_value, tracker);
                    tracker.pop();
                    result
                } else {
                    Vec::new()
                };

                Ok(Self { auth_list })
            }
            _ => {
                tracker.record_type_mismatch("object", value.type_name());
                Ok(Self::default())
            }
        }
    }
}

/// Helper function to parse auth list which can be a single item or an array
fn parse_auth_list(
    value: &CompoteConfigValue,
    tracker: &mut CompoteErrorTracker,
) -> Vec<GithubAuthConfigWithFilters> {
    match value {
        CompoteConfigValue::Array(arr, _) => {
            let mut result = Vec::new();
            for (idx, item) in arr.iter().enumerate() {
                tracker.push_index(idx);
                match <GithubAuthConfigWithFilters as CompoteFromConfigValue>::from_config_value(item, tracker) {
                    Ok(auth) => result.push(auth),
                    Err(e) => tracker.record(e),
                }
                tracker.pop();
            }
            result
        }
        CompoteConfigValue::Object(_, _) => {
            // Single item, treat as a single-element list
            match <GithubAuthConfigWithFilters as CompoteFromConfigValue>::from_config_value(value, tracker) {
                Ok(auth) => vec![auth],
                Err(e) => {
                    tracker.record(e);
                    Vec::new()
                }
            }
        }
        CompoteConfigValue::Null(_) => Vec::new(),
        _ => {
            tracker.record_type_mismatch("array or object", value.type_name());
            Vec::new()
        }
    }
}
