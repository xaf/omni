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

#[derive(Debug, Clone, PartialEq, Default, Serialize, compote::Config)]
#[compote(scalar_as = "hostname", skip_serialize)]
pub struct GhCliConfig {
    #[compote(default)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[compote(default)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Debug, Serialize, Clone, PartialEq, compote::Config)]
#[compote(external_tag, skip_serialize)]
#[serde(rename_all = "snake_case")]
pub enum GithubAuthConfig {
    #[compote(rename = "skip", variant = "skip", variant_value = true)]
    Skip(bool),

    #[compote(rename = "token_env_var", variant = predicate("is_all_caps_string"))]
    TokenEnvVar(String),

    #[compote(rename = "token", variant = any_string)]
    Token(String),

    #[compote(rename = "gh", variant = "gh", variant_default)]
    #[serde(rename = "gh")]
    GhCli(GhCliConfig),
}

impl Default for GithubAuthConfig {
    fn default() -> Self {
        GithubAuthConfig::GhCli(GhCliConfig::default())
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
// Both StringFilter and GithubAuthConfig now use #[derive(compote::Config)] with
// external_tag, which generates FromContextValue automatically.
//
// GithubAuthConfig uses:
//   - variant = "skip" + variant_value = true: string "skip" -> Skip(true)
//   - variant = predicate("is_all_caps_string"): ALL_CAPS strings -> TokenEnvVar
//   - variant = any_string: other strings -> Token (wildcard, last priority)
//   - variant = "gh" + variant_default: string "gh" -> GhCli(default)
//   - scalar_as = "hostname" on GhCliConfig: {gh: "host"} -> GhCli { hostname: Some("host") }
//   - external_tag map dispatch: {token: x}, {token_env_var: x}, {gh: {...}}, {skip: b}

fn is_all_caps_string<S: compote::CustomSource, L: compote::CustomLevel>(
    value: &compote::ContextValue<S, L>,
) -> bool {
    if let compote::ContextValue::String(s, _) = value {
        !s.is_empty() && s.chars().all(|c| c.is_uppercase() || c == '_')
    } else {
        false
    }
}

