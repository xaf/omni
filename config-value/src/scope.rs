use std::hash::Hash;

/// Trait for configuration scope tracking
///
/// Implement this trait to define what scope a configuration value applies to.
/// The scope type is generic, allowing applications to define their own
/// scope hierarchies (e.g., system, user, workspace, project).
pub trait Scope:
    Clone + std::fmt::Debug + PartialEq + Hash + Eq + Ord + PartialOrd + Default
{
    /// Returns a human-readable description of this scope
    fn description(&self) -> String {
        format!("{:?}", self)
    }
}

/// Default scope implementation for cases where scope tracking isn't needed
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Default)]
pub struct DefaultScope;

impl Scope for DefaultScope {
    fn description(&self) -> String {
        "default".to_string()
    }
}
