use std::collections::HashMap;

use crate::extend_strategy::ExtendStrategy;
use crate::scope::Scope;
use crate::source::Source;
use crate::transform::TransformFn;
use crate::value::ConfigValue;

/// Options for extending/merging configuration values
#[derive(Debug, Clone)]
pub struct ExtendOptions {
    /// The merge strategy to use
    pub strategy: ExtendStrategy,
    /// Whether to apply transforms during merge
    pub transform: bool,
}

impl Default for ExtendOptions {
    fn default() -> Self {
        Self {
            strategy: ExtendStrategy::Default,
            transform: true,
        }
    }
}

impl ExtendOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_strategy(mut self, strategy: ExtendStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn with_transform(mut self, transform: bool) -> Self {
        self.transform = transform;
        self
    }
}

/// Builder for loading and configuring ConfigValue behavior
///
/// Allows customization of:
/// - Key-specific merge strategies
/// - Transform functions
pub struct ConfigLoader<S: Source, C: Scope> {
    /// Map of absolute keypaths to their merge strategies
    strategy_overrides: HashMap<String, ExtendStrategy>,
    /// Transform function to apply to values
    transform_fn: Option<TransformFn<S, C>>,
}

impl<S: Source, C: Scope> Default for ConfigLoader<S, C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Source, C: Scope> ConfigLoader<S, C> {
    /// Create a new ConfigLoader with default settings
    pub fn new() -> Self {
        Self {
            strategy_overrides: HashMap::new(),
            transform_fn: None,
        }
    }

    /// Override the merge strategy for a specific keypath
    ///
    /// # Arguments
    /// * `keypath` - Dot-separated absolute path (e.g., "path.append")
    /// * `strategy` - The strategy to use for this keypath
    ///
    /// # Example
    /// ```ignore
    /// loader.with_strategy_override("path.append", ExtendStrategy::Append);
    /// ```
    pub fn with_strategy_override(mut self, keypath: &str, strategy: ExtendStrategy) -> Self {
        self.strategy_overrides
            .insert(keypath.to_string(), strategy);
        self
    }

    /// Set the transform function to apply to values
    pub fn with_transform(mut self, transform: TransformFn<S, C>) -> Self {
        self.transform_fn = Some(transform);
        self
    }

    /// Get the strategy override for a keypath
    pub fn get_strategy(&self, keypath: &[String]) -> Option<&ExtendStrategy> {
        let keypath_str = keypath.join(".");
        self.strategy_overrides.get(&keypath_str)
    }

    /// Apply transform to a value if configured
    pub fn apply_transform(&self, value: &mut ConfigValue<S, C>, keypath: &[String]) {
        if let Some(transform) = self.transform_fn {
            transform(value, keypath);
        }
    }
}
