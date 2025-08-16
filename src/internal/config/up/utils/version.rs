use std::str::FromStr;

use node_semver::Range as semverRange;
use node_semver::Version as semverVersion;
use serde::Deserialize;
use serde::Serialize;

pub struct VersionParserOptions {
    pub complete_version: bool,
}

impl Default for VersionParserOptions {
    fn default() -> Self {
        Self {
            complete_version: true,
        }
    }
}

impl VersionParserOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn complete_version(mut self, complete_version: bool) -> Self {
        self.complete_version = complete_version;
        self
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct VersionParser {
    original: String,
    prefix: Option<String>,
    version: semverVersion,
}

impl std::fmt::Display for VersionParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.original)
    }
}

impl PartialOrd for VersionParser {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VersionParser {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.prefix.cmp(&other.prefix) {
            std::cmp::Ordering::Equal => self.version.cmp(&other.version),
            ordering => ordering,
        }
    }
}

impl VersionParser {
    const MAJOR_MINOR_PATCH_REGEX: &'static str =
        r"^(?P<major>\d+)(?:\.(?P<minor>\d+)(?:\.(?P<patch>\d+))?)?(?P<suffix>.*)?$";

    pub fn compare(a_str: &str, b_str: &str) -> std::cmp::Ordering {
        match (VersionParser::parse(a_str), VersionParser::parse(b_str)) {
            (Some(a_version), Some(b_version)) => a_version.cmp(&b_version),
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (None, None) => a_str.cmp(b_str),
        }
    }

    pub fn parse(version_string: &str) -> Option<Self> {
        Self::parse_with_options(version_string, &VersionParserOptions::default())
    }

    pub fn parse_with_options(
        version_string: &str,
        options: &VersionParserOptions,
    ) -> Option<Self> {
        // Find the first digit in the version string
        let first_digit = version_string.chars().position(|c| c.is_ascii_digit())?;

        // If the first digit is not at the beginning of the string,
        // then the prefix is the part of the string before the first digit
        let (prefix, parseable_version_string) = match first_digit {
            0 => (None, version_string.to_string()),
            _ => (
                Some(version_string[..first_digit].to_string()),
                version_string[first_digit..].to_string(),
            ),
        };

        let parseable_version_string = if options.complete_version {
            // Complete the version if needed
            let reg = regex::Regex::new(Self::MAJOR_MINOR_PATCH_REGEX).unwrap();
            let captures = reg.captures(&parseable_version_string)?;

            format!(
                "{}.{}.{}{}",
                match captures.name("major") {
                    Some(major) => major.as_str(),
                    None => "0",
                },
                match captures.name("minor") {
                    Some(minor) => minor.as_str(),
                    None => "0",
                },
                match captures.name("patch") {
                    Some(patch) => patch.as_str(),
                    None => "0",
                },
                match captures.name("suffix") {
                    Some(suffix) => suffix.as_str(),
                    None => "",
                },
            )
        } else {
            parseable_version_string
        };

        // Try parsing the version with the node_semver::Version object
        let version = match semverVersion::from_str(&parseable_version_string) {
            Ok(version) => version,
            Err(_err) => return None,
        };

        Some(Self {
            original: version_string.to_string(),
            prefix,
            version,
        })
    }

    pub fn has_build(&self) -> bool {
        !self.version.build.is_empty()
    }

    pub fn has_pre_release(&self) -> bool {
        !self.version.pre_release.is_empty()
    }

    pub fn has_prefix(&self) -> bool {
        self.prefix.is_some()
    }

    pub fn satisfies(&self, requirements: &semverRange, options: &VersionSatisfyOptions) -> bool {
        if (!options.prefix && self.has_prefix())
            || (!options.build && self.has_build())
            || (!options.prerelease && self.has_pre_release())
        {
            return false;
        }

        if self.version.satisfies(requirements) {
            return true;
        }

        let clear_prerelease = options.prerelease && self.has_pre_release();
        let clear_build = options.build && self.has_build();
        if !clear_prerelease && !clear_build {
            return false;
        }

        let mut version = self.version.clone();
        version.pre_release = vec![];
        version.build = vec![];
        version.satisfies(requirements)
    }

    pub fn major(&self) -> u64 {
        self.version.major
    }

    pub fn pre_release(&self) -> Vec<node_semver::Identifier> {
        self.version.pre_release.clone()
    }

    pub fn prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct VersionSatisfyOptions {
    prerelease: bool,
    build: bool,
    prefix: bool,
}

impl VersionSatisfyOptions {
    pub fn prerelease(&mut self, allow: bool) -> &mut Self {
        self.prerelease = allow;
        self
    }

    pub fn build(&mut self, allow: bool) -> &mut Self {
        self.build = allow;
        self
    }

    pub fn prefix(&mut self, allow: bool) -> &mut Self {
        self.prefix = allow;
        self
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct VersionMatcher {
    expected_version: String,
    allow_prerelease: bool,
    allow_build: bool,
    allow_prefix: bool,
}

impl VersionMatcher {
    pub fn new(expected_version: &str) -> Self {
        // To convert different formats of versions to support matching, we need to:
        // - Replace commas and semicolons by spaces
        // - Replace quotes by nothing
        let expected_version = expected_version
            .replace([',', ';'], " ")
            .replace(['"', '\''], "");

        Self {
            expected_version,
            ..Self::default()
        }
    }

    pub fn prerelease(&mut self, allow: bool) -> &mut Self {
        self.allow_prerelease = allow;
        self
    }

    pub fn build(&mut self, allow: bool) -> &mut Self {
        self.allow_build = allow;
        self
    }

    pub fn prefix(&mut self, allow: bool) -> &mut Self {
        self.allow_prefix = allow;
        self
    }

    pub fn matches(&self, version: &str) -> bool {
        // Matches versions against `latest` or `*`
        if self.expected_version == "latest" || self.expected_version == "*" {
            if let Some(version) = VersionParser::parse(version) {
                return (self.allow_build || !version.has_build())
                    && (self.allow_prerelease || !version.has_pre_release())
                    && (self.allow_prefix || !version.has_prefix());
            }

            let chars = version.chars().collect::<Vec<char>>();
            return self.validate_version_chars(&chars);
        }

        // Matches versions against the exact match; if the version passed is
        // exactly the expected version, we can stop here
        if self.expected_version == version {
            return true;
        }

        // If the parameter can be matched against a semver range in the node
        // format, and if the version can be matched using the VersionParser,
        // let's just use the VersionParser's satisfies method
        if let (Ok(requirements), Some(version)) = (
            semverRange::from_str(&self.expected_version),
            VersionParser::parse(version),
        ) {
            let mut options = VersionSatisfyOptions::default();
            options.prerelease(self.allow_prerelease);
            options.build(self.allow_build);
            options.prefix(self.allow_prefix);

            return version.satisfies(&requirements, &options);
        }

        // Otherwise, default to prefix matching; this is useful for cases where
        // the version is prefixed with a string and we still want to handle the
        // rest of the string to make sure it fits with the semver format
        if let Some(rest_of_line) = version.strip_prefix(&self.expected_version) {
            let chars = rest_of_line.chars().collect::<Vec<char>>();

            let is_prerelease = self.allow_prerelease && chars[0] == '-';
            let is_build = self.allow_build && chars[0] == '+';

            if chars[0] != '.' && !is_prerelease && !is_build {
                return false;
            }

            if is_prerelease || is_build {
                return chars.len() > 1 && chars[1].is_alphanumeric();
            }

            let chars = chars[1..].to_vec();
            return self.validate_version_chars(&chars);
        }

        false
    }

    fn validate_version_chars(&self, chars: &[char]) -> bool {
        let mut prev = '.';
        let mut any = false;
        let lastidx = chars.len() - 1;
        for (idx, c) in chars.iter().enumerate() {
            let c = *c;
            if !c.is_ascii_digit() {
                if c == '.' {
                    if !prev.is_alphanumeric() {
                        return false;
                    }
                } else if c == '-' {
                    if !self.allow_prerelease || idx == lastidx || !prev.is_alphanumeric() {
                        return false;
                    }
                    any = true;
                } else if c == '+' {
                    if !self.allow_build || idx == lastidx || !prev.is_alphanumeric() {
                        return false;
                    }
                    any = true;
                } else if any {
                    if !c.is_alphanumeric() && c != '_' {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            prev = c;
        }
        true
    }
}

#[cfg(test)]
#[path = "version_test.rs"]
mod tests;
