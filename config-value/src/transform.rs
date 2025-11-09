use crate::value::ConfigValue;
use crate::Source;
use crate::Scope;

/// Function signature for transforming configuration values
///
/// Transform functions can modify configuration values based on their keypath.
/// For example, path resolution can convert relative paths to absolute paths.
///
/// # Arguments
/// * `value` - The configuration value to transform
/// * `keypath` - The path to this value in the configuration tree (e.g., ["path", "append", "0"])
///
/// # Returns
/// The transformed configuration value
pub type TransformFn<S, C> = fn(&mut ConfigValue<S, C>, &[String]);

/// A collection of transform functions that can be applied to configuration values
pub struct TransformPipeline<S: Source, C: Scope> {
    transforms: Vec<TransformFn<S, C>>,
}

impl<S: Source, C: Scope> Default for TransformPipeline<S, C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Source, C: Scope> TransformPipeline<S, C> {
    /// Create a new empty transform pipeline
    pub fn new() -> Self {
        Self {
            transforms: Vec::new(),
        }
    }

    /// Add a transform function to the pipeline
    pub fn add(&mut self, transform: TransformFn<S, C>) {
        self.transforms.push(transform);
    }

    /// Apply all transforms in the pipeline to a value
    pub fn apply(&self, value: &mut ConfigValue<S, C>, keypath: &[String]) {
        for transform in &self.transforms {
            transform(value, keypath);
        }
    }

    /// Check if the pipeline has any transforms
    pub fn is_empty(&self) -> bool {
        self.transforms.is_empty()
    }
}
