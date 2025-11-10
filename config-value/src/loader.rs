use std::collections::HashMap;

use crate::extend_strategy::ExtendStrategy;
use crate::scope::Scope;
use crate::source::Source;
use crate::transform::TransformFn;
use crate::value::ConfigValue;

/// Options for configuration operations
#[derive(Debug, Clone)]
pub struct Options {
    /// The extend/merge strategy to use
    pub extend_strategy: ExtendStrategy,
    /// Whether to apply transforms
    pub transform: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            extend_strategy: ExtendStrategy::Default,
            transform: true,
        }
    }
}

impl Options {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_extend_strategy(mut self, strategy: ExtendStrategy) -> Self {
        self.extend_strategy = strategy;
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
/// - Default extend strategy
/// - Key-specific extend strategies
/// - Transform functions
///
/// Configure once and reuse for multiple merges.
pub struct ConfigLoader<S: Source, C: Scope> {
    /// Default extend strategy
    default_extend_strategy: ExtendStrategy,
    /// Whether transforms are enabled
    transform_enabled: bool,
    /// Map of absolute keypaths to their extend strategies
    extend_strategy_overrides: HashMap<String, ExtendStrategy>,
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
            default_extend_strategy: ExtendStrategy::Default,
            transform_enabled: true,
            extend_strategy_overrides: HashMap::new(),
            transform_fn: None,
        }
    }

    /// Set the default extend strategy
    pub fn with_default_extend_strategy(mut self, strategy: ExtendStrategy) -> Self {
        self.default_extend_strategy = strategy;
        self
    }

    /// Enable or disable transforms
    pub fn with_transform_enabled(mut self, enabled: bool) -> Self {
        self.transform_enabled = enabled;
        self
    }

    /// Override the extend strategy for a specific keypath
    ///
    /// # Arguments
    /// * `keypath` - Dot-separated absolute path (e.g., "path.append")
    /// * `strategy` - The strategy to use for this keypath
    ///
    /// # Example
    /// ```ignore
    /// loader.with_extend_strategy_override("path.append", ExtendStrategy::Append);
    /// ```
    pub fn with_extend_strategy_override(mut self, keypath: &str, strategy: ExtendStrategy) -> Self {
        self.extend_strategy_overrides
            .insert(keypath.to_string(), strategy);
        self
    }

    /// Set the transform function to apply to values
    pub fn with_transform(mut self, transform: TransformFn<S, C>) -> Self {
        self.transform_fn = Some(transform);
        self
    }

    /// Get the extend strategy override for a keypath
    pub fn get_extend_strategy(&self, keypath: &[String]) -> Option<&ExtendStrategy> {
        let keypath_str = keypath.join(".");
        self.extend_strategy_overrides.get(&keypath_str)
    }

    /// Apply transform to a value if configured
    pub fn apply_transform(&self, value: &mut ConfigValue<S, C>, keypath: &[String]) {
        if self.transform_enabled {
            if let Some(transform) = self.transform_fn {
                transform(value, keypath);
            }
        }
    }

    /// Merge another ConfigValue into the base using this loader's configuration
    ///
    /// # Arguments
    /// * `base` - The base configuration to merge into
    /// * `other` - The new configuration to merge
    pub fn merge(
        &self,
        base: &mut ConfigValue<S, C>,
        other: ConfigValue<S, C>,
    ) {
        // Perform the merge
        base.extend(other, self.default_extend_strategy.clone());

        // Apply transforms if enabled
        if self.transform_enabled && self.transform_fn.is_some() {
            self.apply_transforms_recursive(base, &vec![]);
        }
    }

    /// Recursively apply transforms to a ConfigValue tree
    fn apply_transforms_recursive(&self, value: &mut ConfigValue<S, C>, keypath: &Vec<String>) {
        // Apply transform to current value
        self.apply_transform(value, keypath);

        // Recursively apply to children
        if let Some(mapping) = value.as_table() {
            for (key, _) in mapping {
                let mut child_keypath = keypath.clone();
                child_keypath.push(key.clone());

                if let Some(child) = value.get_mut(&key) {
                    self.apply_transforms_recursive(child, &child_keypath);
                }
            }
        } else if let Some(array) = value.as_array() {
            for index in 0..array.len() {
                let mut child_keypath = keypath.clone();
                child_keypath.push(index.to_string());

                if let Some(child) = value.as_array_mut().and_then(|arr| arr.get_mut(index)) {
                    self.apply_transforms_recursive(child, &child_keypath);
                }
            }
        }
    }

    /// Create Options from this loader's configuration
    pub fn options(&self) -> Options {
        Options {
            extend_strategy: self.default_extend_strategy.clone(),
            transform: self.transform_enabled,
        }
    }
}
