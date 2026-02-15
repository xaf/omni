use serde::Serialize;

use crate::internal::cache::utils::Empty;

// Compote imports - no longer needed, using compote:: directly

#[derive(Debug, Clone, Default, compote::Config)]
pub struct GithubConfig {
    #[compote(default, rename = "auth", allow_single, skip_if_empty)]
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

#[derive(Debug, Clone, PartialEq, compote::Config)]
pub struct GithubAuthConfigWithFilters {
    #[compote(default, skip_if_default)]
    pub repo: StringFilter,
    #[compote(default, skip_if_default)]
    pub hostname: StringFilter,
    #[compote(flatten)]
    pub auth: GithubAuthConfig,
}

impl GithubAuthConfigWithFilters {
    pub fn matches(&self, repo: &str, api_hostname: &str) -> bool {
        self.repo.matches(repo) && self.hostname.matches(api_hostname)
    }
}

#[derive(Debug, Serialize, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq, Default, compote::Config)]
#[compote(external_tag, rename_all = "snake_case")]
pub enum StringFilter {
    Contains(String),
    StartsWith(String),
    EndsWith(String),
    Regex(String),
    #[compote(variant = any_string)]
    Glob(String),
    Exact(String),
    #[default]
    #[compote(variant = null)]
    Any,
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
    // Used by serde's skip_serializing_if attribute
    #[allow(dead_code)]
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
// Compote FromContextValue implementations
// ============================================================================
//
// StringFilter now uses #[derive(compote::Config)] with external_tag, which generates
// FromContextValue, Serialize, AND Deserialize (via the `deserialize` flag).
//
// GithubAuthConfig requires manual FromContextValue due to heuristic-based string parsing
// that can't be expressed with derive macro attributes:
//    - Uses custom logic: all-caps strings -> TokenEnvVar, others -> Token
//    - "skip" and "gh" are special string values
//    - Object can have multiple keys (skip, token, token_env_var, gh)
//    - The gh variant accepts string (hostname only) or object {hostname, user}

impl<S: compote::CustomSource, L: compote::CustomLevel> compote::FromContextValue<S, L> for GithubAuthConfig {
    fn from_context_value(
        value: &compote::ContextValue<S, L>,
        tracker: &mut compote::ErrorTracker,
    ) -> Result<Self, compote::Error> {
        match value {
            compote::ContextValue::Null(_) => Ok(Self::default()),
            compote::ContextValue::String(s, _) => {
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
            compote::ContextValue::Object(table, _) => {
                // Check for skip
                if let Some(skip_value) = table.get("skip") {
                    tracker.push_field("skip");
                    match skip_value {
                        compote::ContextValue::Bool(true, _) => {
                            tracker.pop();
                            return Ok(Self::Skip(true));
                        }
                        compote::ContextValue::Bool(false, _) => {
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
                    if let compote::ContextValue::String(s, _) = token_env_var_value {
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
                    if let compote::ContextValue::String(s, _) = token_value {
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
                        compote::ContextValue::Object(gh_table, _) => {
                            if let Some(hostname_value) = gh_table.get("hostname") {
                                if let compote::ContextValue::String(s, _) = hostname_value {
                                    hostname = Some(s.clone());
                                }
                            }
                            if let Some(user_value) = gh_table.get("user") {
                                if let compote::ContextValue::String(s, _) = user_value {
                                    user = Some(s.clone());
                                }
                            }
                        }
                        compote::ContextValue::String(s, _) => {
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

// Manual impl replaced by derive macro:
// impl<S: compote::CustomSource, L: compote::CustomLevel> compote::FromContextValue<S, L> for GithubAuthConfigWithFilters {
//     fn from_context_value(
//         value: &compote::ContextValue<S, L>,
//         tracker: &mut compote::ErrorTracker,
//     ) -> Result<Self, compote::Error> {
//         match value {
//             compote::ContextValue::Null(_) => Ok(Self {
//                 repo: StringFilter::default(),
//                 hostname: StringFilter::default(),
//                 auth: GithubAuthConfig::default(),
//             }),
//             compote::ContextValue::Object(table, _) => {
//                 // Parse repo filter
//                 let repo = if let Some(repo_value) = table.get("repo") {
//                     tracker.push_field("repo");
//                     let result = <StringFilter as compote::FromContextValue<S, L>>::from_context_value(repo_value, tracker)?;
//                     tracker.pop();
//                     result
//                 } else {
//                     StringFilter::default()
//                 };
//
//                 // Parse hostname filter
//                 let hostname = if let Some(hostname_value) = table.get("hostname") {
//                     tracker.push_field("hostname");
//                     let result = <StringFilter as compote::FromContextValue<S, L>>::from_context_value(hostname_value, tracker)?;
//                     tracker.pop();
//                     result
//                 } else {
//                     StringFilter::default()
//                 };
//
//                 // Parse auth (from the same object, flattened)
//                 let auth = <GithubAuthConfig as compote::FromContextValue<S, L>>::from_context_value(value, tracker)?;
//
//                 Ok(Self { repo, hostname, auth })
//             }
//             _ => {
//                 tracker.record_type_mismatch("object", value.type_name());
//                 Ok(Self {
//                     repo: StringFilter::default(),
//                     hostname: StringFilter::default(),
//                     auth: GithubAuthConfig::default(),
//                 })
//             }
//         }
//     }
// }

// Manual impl replaced by derive macro:
// impl<S: compote::CustomSource, L: compote::CustomLevel> compote::FromContextValue<S, L> for GithubConfig {
//     fn from_context_value(
//         value: &compote::ContextValue<S, L>,
//         tracker: &mut compote::ErrorTracker,
//     ) -> Result<Self, compote::Error> {
//         match value {
//             compote::ContextValue::Null(_) => Ok(Self::default()),
//             compote::ContextValue::Object(table, _) => {
//                 let auth_list = if let Some(auth_value) = table.get("auth") {
//                     tracker.push_field("auth");
//                     let result = parse_auth_list::<S, L>(auth_value, tracker);
//                     tracker.pop();
//                     result
//                 } else {
//                     Vec::new()
//                 };
//
//                 Ok(Self { auth_list })
//             }
//             _ => {
//                 tracker.record_type_mismatch("object", value.type_name());
//                 Ok(Self::default())
//             }
//         }
//     }
// }
//
// /// Helper function to parse auth list which can be a single item or an array
// fn parse_auth_list<S: compote::CustomSource, L: compote::CustomLevel>(
//     value: &compote::ContextValue<S, L>,
//     tracker: &mut compote::ErrorTracker,
// ) -> Vec<GithubAuthConfigWithFilters> {
//     match value {
//         compote::ContextValue::Array(arr, _) => {
//             let mut result = Vec::new();
//             for (idx, item) in arr.iter().enumerate() {
//                 tracker.push_index(idx);
//                 match <GithubAuthConfigWithFilters as compote::FromContextValue<S, L>>::from_context_value(item, tracker) {
//                     Ok(auth) => result.push(auth),
//                     Err(e) => tracker.record(e),
//                 }
//                 tracker.pop();
//             }
//             result
//         }
//         compote::ContextValue::Object(_, _) => {
//             // Single item, treat as a single-element list
//             match <GithubAuthConfigWithFilters as compote::FromContextValue<S, L>>::from_context_value(value, tracker) {
//                 Ok(auth) => vec![auth],
//                 Err(e) => {
//                     tracker.record(e);
//                     Vec::new()
//                 }
//             }
//         }
//         compote::ContextValue::Null(_) => Vec::new(),
//         _ => {
//             tracker.record_type_mismatch("array or object", value.type_name());
//             Vec::new()
//         }
//     }
// }
