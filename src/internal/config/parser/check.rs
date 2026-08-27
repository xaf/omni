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

fn check_pattern_is_global<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    ctx: &feuilletage::Context<S, L>,
) -> bool {
    ctx.level.name() != "local"
}

/// A check pattern that captures its source context during parsing
#[derive(Debug, Clone, feuilletage::Config)]
#[feuilletage(scalar_as = "pattern", skip_serialize, skip_deserialize)]
pub struct CheckPattern {
    #[feuilletage(default)]
    pub pattern: String,
    #[feuilletage(from_context = "source.file_path")]
    pub source_path: Option<PathBuf>,
    #[feuilletage(from_context_fn = "check_pattern_is_global")]
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

#[derive(Debug, Clone, Default, feuilletage::Config)]
pub struct CheckConfig {
    #[feuilletage(default, allow_single, skip_if_empty)]
    patterns: Vec<CheckPattern>,
    #[feuilletage(default, skip_if_empty)]
    pub ignore: HashSet<String>,
    #[feuilletage(default, skip_if_empty)]
    pub select: HashSet<String>,
    #[feuilletage(default, allow_list, skip_if_empty)]
    pub tags: HashMap<String, StringFilter>,
}

impl Empty for CheckConfig {
    fn is_empty(&self) -> bool {
        self.patterns.is_empty() && self.ignore.is_empty() && self.select.is_empty()
    }
}

impl feuilletage::IsEmpty for CheckConfig {
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

#[cfg(test)]
#[path = "check_test.rs"]
mod tests;
