use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils::Empty;
use crate::internal::commands::utils::abs_path_from_path;
use crate::internal::config::parser::github::StringFilter;

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

impl<S: compote::CustomSource, L: compote::CustomLevel> compote::FromContextValue<S, L> for CheckPattern {
    fn from_context_value(
        value: &compote::ContextValue<S, L>,
        tracker: &mut compote::ErrorTracker,
    ) -> Result<Self, compote::Error> {
        // Parse the pattern string
        let pattern = String::from_context_value(value, tracker)?;

        // Extract source path from context using CustomSource trait method
        let source_path = value.context().source.file_path().map(|p| p.to_path_buf());

        // Determine if this is a global pattern (not from Local/workdir level)
        // Using CustomLevel::name() method instead of pattern matching
        let is_global = value.context().level.name() != "local";

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

#[derive(Debug, Serialize, Clone, Default)]
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

impl<S: compote::CustomSource, L: compote::CustomLevel> compote::FromContextValue<S, L> for CheckConfig {
    fn from_context_value(
        value: &compote::ContextValue<S, L>,
        tracker: &mut compote::ErrorTracker,
    ) -> Result<Self, compote::Error> {
        let table = match value {
            compote::ContextValue::Object(map, _) => map,
            compote::ContextValue::Null(_) => return Ok(Self::default()),
            _ => {
                return Err(compote::Error::TypeMismatch {
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
                compote::ContextValue::String(_, _) => {
                    // Single pattern
                    let pattern = <CheckPattern as compote::FromContextValue<S, L>>::from_context_value(v, tracker)?;
                    patterns.push(pattern);
                }
                compote::ContextValue::Array(arr, _) => {
                    for (idx, item) in arr.iter().enumerate() {
                        tracker.push_index(idx);
                        match <CheckPattern as compote::FromContextValue<S, L>>::from_context_value(item, tracker) {
                            Ok(pattern) => patterns.push(pattern),
                            Err(e) => tracker.record(e),
                        }
                        tracker.pop();
                    }
                }
                _ => {
                    tracker.record(compote::Error::TypeMismatch {
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
            let result = parse_tags::<S, L>(v, tracker);
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
fn parse_string_array_to_hashset<S: compote::CustomSource, L: compote::CustomLevel>(
    value: &compote::ContextValue<S, L>,
    tracker: &mut compote::ErrorTracker,
) -> HashSet<String> {
    let mut result = HashSet::new();
    if let compote::ContextValue::Array(arr, _) = value {
        for (idx, item) in arr.iter().enumerate() {
            tracker.push_index(idx);
            if let compote::ContextValue::String(s, _) = item {
                result.insert(s.clone());
            } else {
                tracker.record(compote::Error::TypeMismatch {
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
fn parse_tags<S: compote::CustomSource, L: compote::CustomLevel>(
    value: &compote::ContextValue<S, L>,
    tracker: &mut compote::ErrorTracker,
) -> HashMap<String, StringFilter> {
    let mut tags = HashMap::new();

    match value {
        compote::ContextValue::Object(table, _) => {
            for (key, v) in table {
                tracker.push_field(key);
                match <StringFilter as compote::FromContextValue<S, L>>::from_context_value(v, tracker) {
                    Ok(filter) => {
                        tags.insert(key.clone(), filter);
                    }
                    Err(e) => tracker.record(e),
                }
                tracker.pop();
            }
        }
        compote::ContextValue::Array(arr, _) => {
            for (idx, item) in arr.iter().enumerate() {
                tracker.push_index(idx);
                match item {
                    compote::ContextValue::String(s, _) => {
                        tags.insert(s.clone(), StringFilter::default());
                    }
                    compote::ContextValue::Object(table, _) => {
                        for (key, v) in table {
                            tracker.push_field(key);
                            match <StringFilter as compote::FromContextValue<S, L>>::from_context_value(v, tracker) {
                                Ok(filter) => {
                                    tags.insert(key.clone(), filter);
                                }
                                Err(e) => tracker.record(e),
                            }
                            tracker.pop();
                        }
                    }
                    _ => {
                        tracker.record(compote::Error::TypeMismatch {
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
            tracker.record(compote::Error::TypeMismatch {
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
