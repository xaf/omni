use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils::Empty;
use crate::internal::commands::utils::abs_path_from_path;
use crate::internal::config::parser::errors::ConfigErrorHandler;
use crate::internal::config::parser::errors::ConfigErrorKind;
use crate::internal::config::parser::github::StringFilter;
use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CheckConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    patterns: Vec<ConfigValue>,
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

impl CheckConfig {
    pub(super) fn from_config_value(
        config_value: Option<ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        if !config_value.is_table() {
            error_handler
                .with_expected("table")
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);

            return Self::default();
        }

        let mut patterns = Vec::new();
        if let Some(value) = config_value.get("patterns") {
            if value.as_str_forced().is_some() {
                patterns.push(value.clone());
            } else if let Some(array) = value.as_array() {
                for (idx, value) in array.iter().enumerate() {
                    if value.as_str_forced().is_some() {
                        patterns.push(value.clone());
                    } else {
                        error_handler
                            .with_key("patterns")
                            .with_index(idx)
                            .with_expected("string")
                            .with_actual(value)
                            .error(ConfigErrorKind::InvalidValueType);
                    }
                }
            } else {
                error_handler
                    .with_key("patterns")
                    .with_expected("string or array of strings")
                    .with_actual(value)
                    .error(ConfigErrorKind::InvalidValueType);
            }
        }

        let ignore = config_value
            .get_as_str_array("ignore", &error_handler.with_key("ignore"))
            .into_iter()
            .collect();
        let select = config_value
            .get_as_str_array("select", &error_handler.with_key("select"))
            .into_iter()
            .collect();

        let tags = if let Some(value) = config_value.get("tags") {
            if let Some(table) = value.as_table() {
                table
                    .into_iter()
                    .map(|(key, value)| {
                        let filter = StringFilter::from_config_value(
                            Some(value),
                            &error_handler.with_key("tags").with_key(&key),
                        );
                        (key.clone(), filter)
                    })
                    .collect()
            } else if let Some(array) = value.as_array() {
                let mut tags = HashMap::new();
                for (idx, value) in array.iter().enumerate() {
                    if let Some(value) = value.as_str_forced() {
                        tags.insert(value.to_string(), StringFilter::default());
                    } else if let Some(table) = value.as_table() {
                        for (key, value) in table {
                            let filter = StringFilter::from_config_value(
                                Some(value),
                                &error_handler
                                    .with_key("tags")
                                    .with_index(idx)
                                    .with_key(&key),
                            );
                            tags.insert(key.clone(), filter);
                        }
                    } else {
                        error_handler
                            .with_key("tags")
                            .with_index(idx)
                            .with_expected(vec!["string", "table"])
                            .with_actual(value)
                            .error(ConfigErrorKind::InvalidValueType);
                    }
                }
                tags
            } else {
                error_handler
                    .with_key("tags")
                    .with_expected(vec!["table", "array"])
                    .with_actual(value)
                    .error(ConfigErrorKind::InvalidValueType);

                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        Self {
            patterns,
            ignore,
            select,
            tags,
        }
    }

    pub fn patterns(&self) -> Vec<String> {
        self.patterns
            .iter()
            .map(path_pattern_from_config_value)
            .collect()
    }
}

fn path_pattern_from_config_value(value: &ConfigValue) -> String {
    let pattern = value.as_str_forced().expect("value should be a string");
    match value.get_source().path() {
        Some(path) => {
            let as_path = PathBuf::from(path);
            let parent = as_path.parent().unwrap_or(&as_path);
            let as_str = parent.to_string_lossy();

            path_pattern_from_str(
                &pattern,
                Some(&as_str),
                !matches!(value.get_scope(), ConfigScope::Workdir),
            )
        }
        None => pattern.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    mod path_pattern_from_str {
        use super::*;

        #[test]
        fn test_global_patterns() {
            // Test absolute paths with global flag
            assert_eq!(
                path_pattern_from_str("/some/path", None, true),
                "/some/path"
            );

            // Test relative paths with global flag
            let current_dir = std::env::current_dir().expect("failed to get current dir");
            assert_eq!(
                path_pattern_from_str("some/path", None, true),
                current_dir.join("some/path").to_string_lossy().to_string(),
            );
        }

        #[test]
        fn test_negative_patterns() {
            // Test negative absolute path
            assert_eq!(
                path_pattern_from_str("!/some/path", None, true),
                "!/some/path"
            );

            // Test negative relative path with location
            assert_eq!(
                path_pattern_from_str("!relative/path", Some("/base/dir"), false),
                "!/base/dir/relative/path"
            );
        }

        #[test]
        fn test_glob_patterns() {
            // Test basic glob pattern
            assert_eq!(
                path_pattern_from_str("**/file.txt", None, false),
                "**/file.txt"
            );

            // Test double-star pattern
            assert_eq!(path_pattern_from_str("**", None, false), "**");

            // Test negative glob pattern
            assert_eq!(
                path_pattern_from_str("!**/file.txt", None, false),
                "!**/file.txt"
            );
        }

        #[test]
        fn test_relative_paths() {
            // Test relative path with location
            assert_eq!(
                path_pattern_from_str("relative/path", Some("/base/dir"), false),
                "/base/dir/relative/path"
            );

            // Test relative path without location (should use current dir)
            let current_dir = std::env::current_dir().expect("failed to get current dir");
            assert_eq!(
                path_pattern_from_str("relative/path", None, false),
                current_dir
                    .join("relative/path")
                    .to_string_lossy()
                    .to_string(),
            );
        }

        #[test]
        fn test_trim_leading_slash() {
            // Test that leading slashes are trimmed for non-global patterns
            assert_eq!(
                path_pattern_from_str("/path/to/file", Some("/base/dir"), false),
                "/base/dir/path/to/file"
            );
        }

        #[test]
        fn test_edge_cases() {
            // Test empty pattern
            assert_eq!(
                path_pattern_from_str("", Some("/base/dir"), false),
                "/base/dir"
            );

            // Test single slash
            assert_eq!(path_pattern_from_str("/", None, true), "/");

            // Test negative empty pattern
            assert_eq!(
                path_pattern_from_str("!", Some("/base/dir"), false),
                "!/base/dir"
            );
        }
    }
}
