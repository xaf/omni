/// Trait for configuration source tracking
///
/// Implement this trait to define where configuration values come from.
/// The source type is generic, allowing applications to define their own
/// source hierarchies (e.g., default, file, environment, CLI args).
pub trait Source: Clone + std::fmt::Debug + PartialEq {
    /// Returns the priority of this source for merge resolution
    ///
    /// Higher priority values override lower priority values when merging.
    /// For example:
    /// - Default: 0
    /// - File: 10
    /// - Environment: 20
    /// - CLI: 30
    fn priority(&self) -> u32;

    /// Returns a human-readable description of this source
    fn description(&self) -> String {
        format!("{:?}", self)
    }
}

/// Default source implementation for cases where source tracking isn't needed
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DefaultSource;

impl Source for DefaultSource {
    fn priority(&self) -> u32 {
        0
    }

    fn description(&self) -> String {
        "default".to_string()
    }
}
