/// Strategy for extending/merging configuration values
///
/// These strategies control how configuration values are merged when
/// multiple sources provide values for the same key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtendStrategy {
    /// Default merging behavior:
    /// - Mappings: merge recursively
    /// - Sequences: replace
    /// - Values: replace
    Default,

    /// Append to existing sequences, merge mappings recursively
    /// For sequences, new values are appended (duplicates removed)
    Append,

    /// Prepend to existing sequences, merge mappings recursively
    /// For sequences, new values are prepended (duplicates removed)
    Prepend,

    /// Replace existing value completely
    Replace,

    /// Keep existing value, ignore new value
    Keep,

    /// Raw mode: disable all automatic strategy detection from key suffixes
    Raw,
}

impl Default for ExtendStrategy {
    fn default() -> Self {
        Self::Default
    }
}

impl ExtendStrategy {
    /// Parse strategy from key suffix
    ///
    /// Recognizes standard key suffixes:
    /// - `key__toappend` -> Append
    /// - `key__toprepend` -> Prepend
    /// - `key__toreplace` -> Replace
    /// - `key__ifnone` -> Keep
    ///
    /// Returns (real_key, strategy) where real_key has suffix removed if found
    pub fn from_key(key: &str) -> (String, Option<ExtendStrategy>) {
        if let Some(real_key) = key.strip_suffix("__toappend") {
            (real_key.to_string(), Some(ExtendStrategy::Append))
        } else if let Some(real_key) = key.strip_suffix("__toprepend") {
            (real_key.to_string(), Some(ExtendStrategy::Prepend))
        } else if let Some(real_key) = key.strip_suffix("__toreplace") {
            (real_key.to_string(), Some(ExtendStrategy::Replace))
        } else if let Some(real_key) = key.strip_suffix("__ifnone") {
            (real_key.to_string(), Some(ExtendStrategy::Keep))
        } else {
            (key.to_string(), None)
        }
    }
}

#[cfg(test)]
#[path = "extend_strategy_test.rs"]
mod tests;
