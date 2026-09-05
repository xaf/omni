use std::fs;
use std::os::unix::fs::PermissionsExt;

pub fn is_executable(path: &std::path::Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Check if a value is allowed by a list of glob patterns.
///
/// The patterns are checked in order, and if no patterns match, the
/// last pattern's deny status is returned (e.g. if the last pattern is
/// a deny pattern, the default is to allow).
///
/// If the list of patterns is empty, all values are allowed.
/// If a pattern starts with `!`, it is a deny pattern.
/// If a pattern does not start with `!`, it is an allow pattern.
pub fn check_allowed(value: &str, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return true; // No patterns means allow all
    }

    for pattern in patterns {
        let is_deny = pattern.starts_with('!');
        let pattern_str = if is_deny { &pattern[1..] } else { pattern };

        let matches = glob::Pattern::new(pattern_str).is_ok_and(|pat| pat.matches(value));
        if matches {
            return !is_deny; // Early return on match
        }

        if pattern_str.ends_with('*') {
            continue;
        }

        // If the pattern does not end with a wildcard, try checking
        // for the pattern as a directory that should prefix the file
        let dir_pattern = format!("{}/**", pattern_str.trim_end_matches('/'));
        let matches = glob::Pattern::new(&dir_pattern).is_ok_and(|pat| pat.matches(value));
        if matches {
            return !is_deny; // Early return on match
        }
    }

    // Get the last pattern's deny status (if any) for the default case
    let default = patterns.last().is_none_or(|p| p.starts_with('!'));
    default
}

#[cfg(test)]
#[path = "utils_test.rs"]
mod tests;
