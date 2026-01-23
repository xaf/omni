use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils::Empty;
use crate::internal::commands::utils::abs_path_from_path;
use crate::internal::config::parser::github::StringFilter;

// Compote imports
use compote::Error as CompoteError;
use compote::ContextValue as CompoteConfigValue;
use compote::ErrorTracker as CompoteErrorTracker;
use compote::FromContextValue as CompoteFromConfigValue;
use compote::Level as CompoteConfigLevel;
use compote::Source as CompoteConfigSource;

// ============================================================================
// CheckPattern: A pattern string that captures its source context
// ============================================================================
//
// This type captures the source path and level from the ConfigValue during
// parsing, so that pattern resolution can happen correctly later.
//
// - source_path: Used to resolve relative patterns to absolute paths
// - is_global: Determines if the pattern is from system/user config (global)
//              vs workdir config (local), which affects path interpretation
//
// ============================================================================

/// A check pattern that captures its source context during parsing
#[derive(Debug, Clone)]
pub struct CheckPattern {
    /// The pattern string
    pub pattern: String,
    /// The path of the config file this pattern came from (for relative path resolution)
    pub source_path: Option<PathBuf>,
    /// Whether this is a global pattern (from system/user config, not workdir)
    pub is_global: bool,
}

impl CheckPattern {
    /// Convert to the resolved pattern string
    pub fn resolve(&self) -> String {
        match &self.source_path {
            Some(path) => {
                let parent = path.parent().unwrap_or(path);
                let parent_str = parent.to_string_lossy();
                path_pattern_from_str(&self.pattern, Some(&parent_str), self.is_global)
            }
            None => self.pattern.clone(),
        }
    }
}

impl CompoteFromConfigValue for CheckPattern {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteError> {
        // Parse the pattern string
        let pattern = String::from_config_value(value, tracker)?;

        // Extract source path from context
        let source_path = match &value.context().source {
            CompoteConfigSource::File(path) => Some(path.clone()),
            _ => None,
        };

        // Determine if this is a global pattern (not from Local/workdir level)
        let is_global = !matches!(value.context().level, CompoteConfigLevel::Local);

        Ok(Self {
            pattern,
            source_path,
            is_global,
        })
    }
}

// Serialize as just the pattern string
impl Serialize for CheckPattern {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.pattern.serialize(serializer)
    }
}

// Deserialize from string (for cache compatibility)
impl<'de> Deserialize<'de> for CheckPattern {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let pattern = String::deserialize(deserializer)?;
        // When deserializing from cache, we don't have source context
        // This is fine because patterns should already be resolved when cached
        Ok(Self {
            pattern,
            source_path: None,
            is_global: false,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CheckConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    patterns: Vec<CheckPattern>,
    #[serde(skip_serializing_if = "HashSet::is_empty")]
    pub ignore: HashSet<String>,
    #[serde(skip_serializing_if = "HashSet::is_empty")]
    pub select: HashSet<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub tags: HashMap<String, StringFilter>,
}

impl Empty for CheckConfig {
    fn is_empty(&self) -> bool {
        self.patterns.is_empty() && self.ignore.is_empty() && self.select.is_empty()
    }
}

impl compote::IsEmpty for CheckConfig {
    fn is_empty(&self) -> bool {
        Empty::is_empty(self)
    }
}

impl CheckConfig {
    pub fn patterns(&self) -> Vec<String> {
        self.patterns.iter().map(|p| p.resolve()).collect()
    }
}

pub fn path_pattern_from_str(pattern: &str, location: Option<&str>, global: bool) -> String {
    let (negative, pattern) = if let Some(pattern) = pattern.strip_prefix('!') {
        (true, pattern)
    } else {
        (false, pattern)
    };

    // If global pattern, we allow to specify absolute paths, otherwise
    // absolute paths are from the provided location
    let pattern = if global {
        pattern
    } else {
        pattern.trim_start_matches("/")
    };

    // If the pattern starts with '/' or '*', it's an absolute path
    // or a glob pattern, so we don't need to prepend the location.
    if pattern.starts_with('/') || pattern.starts_with("**/") || pattern == "**" {
        return format!("{}{}", if negative { "!" } else { "" }, pattern);
    }

    // If we get here, convert into an absolute path
    let abs_pattern = abs_path_from_path(PathBuf::from(pattern), location.map(PathBuf::from));

    // Return the absolute path with the negation prefix if needed
    format!(
        "{}{}",
        if negative { "!" } else { "" },
        abs_pattern.to_string_lossy()
    )
}

// ============================================================================
// Compote FromConfigValue for CheckConfig
// ============================================================================

impl CompoteFromConfigValue for CheckConfig {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteError> {
        let table = match value {
            CompoteConfigValue::Object(map, _) => map,
            CompoteConfigValue::Null(_) => return Ok(Self::default()),
            _ => {
                return Err(CompoteError::TypeMismatch {
                    expected: "table".to_string(),
                    actual: value.type_name().to_string(),
                    path: tracker.current_path(),
                });
            }
        };

        // Parse patterns - can be a single string or array of strings
        let mut patterns = Vec::new();
        if let Some(v) = table.get("patterns") {
            tracker.push_field("patterns");
            match v {
                CompoteConfigValue::String(_, _) => {
                    // Single pattern
                    let pattern = <CheckPattern as CompoteFromConfigValue>::from_config_value(v, tracker)?;
                    patterns.push(pattern);
                }
                CompoteConfigValue::Array(arr, _) => {
                    for (idx, item) in arr.iter().enumerate() {
                        tracker.push_index(idx);
                        match <CheckPattern as CompoteFromConfigValue>::from_config_value(item, tracker) {
                            Ok(pattern) => patterns.push(pattern),
                            Err(e) => tracker.record(e),
                        }
                        tracker.pop();
                    }
                }
                _ => {
                    tracker.record(CompoteError::TypeMismatch {
                        expected: "string or array of strings".to_string(),
                        actual: v.type_name().to_string(),
                        path: tracker.current_path(),
                    });
                }
            }
            tracker.pop();
        }

        // Parse ignore - array of strings to HashSet
        let ignore = if let Some(v) = table.get("ignore") {
            tracker.push_field("ignore");
            let result = parse_string_array_to_hashset(v, tracker);
            tracker.pop();
            result
        } else {
            HashSet::new()
        };

        // Parse select - array of strings to HashSet
        let select = if let Some(v) = table.get("select") {
            tracker.push_field("select");
            let result = parse_string_array_to_hashset(v, tracker);
            tracker.pop();
            result
        } else {
            HashSet::new()
        };

        // Parse tags - can be table or array
        let tags = if let Some(v) = table.get("tags") {
            tracker.push_field("tags");
            let result = parse_tags(v, tracker);
            tracker.pop();
            result
        } else {
            HashMap::new()
        };

        Ok(Self {
            patterns,
            ignore,
            select,
            tags,
        })
    }
}

/// Parse an array of strings into a HashSet
fn parse_string_array_to_hashset(
    value: &CompoteConfigValue,
    tracker: &mut CompoteErrorTracker,
) -> HashSet<String> {
    let mut result = HashSet::new();
    if let CompoteConfigValue::Array(arr, _) = value {
        for (idx, item) in arr.iter().enumerate() {
            tracker.push_index(idx);
            if let CompoteConfigValue::String(s, _) = item {
                result.insert(s.clone());
            } else {
                tracker.record(CompoteError::TypeMismatch {
                    expected: "string".to_string(),
                    actual: item.type_name().to_string(),
                    path: tracker.current_path(),
                });
            }
            tracker.pop();
        }
    }
    result
}

/// Parse tags - can be table or array of strings/tables
fn parse_tags(
    value: &CompoteConfigValue,
    tracker: &mut CompoteErrorTracker,
) -> HashMap<String, StringFilter> {
    let mut tags = HashMap::new();

    match value {
        CompoteConfigValue::Object(table, _) => {
            for (key, v) in table {
                tracker.push_field(key);
                match <StringFilter as CompoteFromConfigValue>::from_config_value(v, tracker) {
                    Ok(filter) => {
                        tags.insert(key.clone(), filter);
                    }
                    Err(e) => tracker.record(e),
                }
                tracker.pop();
            }
        }
        CompoteConfigValue::Array(arr, _) => {
            for (idx, item) in arr.iter().enumerate() {
                tracker.push_index(idx);
                match item {
                    CompoteConfigValue::String(s, _) => {
                        tags.insert(s.clone(), StringFilter::default());
                    }
                    CompoteConfigValue::Object(table, _) => {
                        for (key, v) in table {
                            tracker.push_field(key);
                            match <StringFilter as CompoteFromConfigValue>::from_config_value(v, tracker) {
                                Ok(filter) => {
                                    tags.insert(key.clone(), filter);
                                }
                                Err(e) => tracker.record(e),
                            }
                            tracker.pop();
                        }
                    }
                    _ => {
                        tracker.record(CompoteError::TypeMismatch {
                            expected: "string or table".to_string(),
                            actual: item.type_name().to_string(),
                            path: tracker.current_path(),
                        });
                    }
                }
                tracker.pop();
            }
        }
        _ => {
            tracker.record(CompoteError::TypeMismatch {
                expected: "table or array".to_string(),
                actual: value.type_name().to_string(),
                path: tracker.current_path(),
            });
        }
    }

    tags
}

#[cfg(test)]
#[path = "check_test.rs"]
mod tests;
