/// Trait for handling configuration errors with context
///
/// Allows applications to implement custom error reporting strategies
/// while maintaining a consistent API for error context collection.
pub trait ErrorHandler: Clone {
    /// Error kind/type (application-specific)
    type ErrorKind;

    /// Set the expected type/value
    ///
    /// Accepts anything that can be converted to a Value, allowing flexible usage:
    /// - Single string: `with_expected("string")`
    /// - Multiple options: `with_expected(vec!["string", "array of strings"])`
    /// - Any Value: `with_expected(Value::String("foo".to_string()))`
    fn with_expected<V: Into<crate::Value>>(self, expected: V) -> Self;

    /// Set the actual value that was found
    fn with_actual<S: crate::Source, C: crate::Scope>(
        self,
        actual: crate::ConfigValue<S, C>,
    ) -> Self;

    /// Set an index (for array element errors)
    fn with_index(self, index: usize) -> Self;

    /// Report the error
    fn error(self, kind: Self::ErrorKind);
}

/// A no-op error handler that does nothing
///
/// Useful for cases where you want the validation behavior
/// but don't need error reporting.
#[derive(Clone)]
pub struct NoOpErrorHandler;

impl ErrorHandler for NoOpErrorHandler {
    type ErrorKind = ();

    fn with_expected<V: Into<crate::Value>>(self, _expected: V) -> Self {
        self
    }

    fn with_actual<S: crate::Source, C: crate::Scope>(
        self,
        _actual: crate::ConfigValue<S, C>,
    ) -> Self {
        self
    }

    fn with_index(self, _index: usize) -> Self {
        self
    }

    fn error(self, _kind: Self::ErrorKind) {
        // No-op
    }
}

#[cfg(test)]
#[path = "error_handler_test.rs"]
mod tests;
