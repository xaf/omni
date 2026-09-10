use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug, Clone, Serialize, PartialEq)]
pub enum UpError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("execution error: {0}")]
    Exec(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("cache error: {0}")]
    Cache(String),
    #[error("tap in use")]
    HomebrewTapInUse,
    #[error("{}", match .1 {
        Some((step, total)) => format!("step {}/{} '{}' failed", step, total, .0),
        None => format!("step '{}' failed", .0)
    })]
    StepFailed(String, Option<(usize, usize)>),
    #[error("I/O error: {0}")]
    IOError(String),
}

impl From<std::io::Error> for UpError {
    fn from(error: std::io::Error) -> Self {
        UpError::IOError(error.to_string())
    }
}

impl UpError {
    /// Add context to an error without repeating its variant prefix.
    ///
    /// Interpolating an `UpError` into the message of another `UpError`
    /// double-prefixes, because `Display` prepends the variant text:
    /// `Exec` renders as `execution error: {0}`, so an `Exec` wrapped in an
    /// `Exec` reads `execution error: ... execution error: ...`. Composing
    /// through `message()` keeps exactly one prefix.
    ///
    /// The variant is preserved, so a timeout stays a timeout.
    pub fn with_context(self, context: impl std::fmt::Display) -> Self {
        // StepFailed's payload is a step name, not a message, and it already
        // means "rendered, carry identity upward" -- adding prose to it would
        // corrupt the name it renders.
        if matches!(self, UpError::StepFailed(_, _) | UpError::HomebrewTapInUse) {
            return self;
        }

        let message = format!("{context}: {}", self.message());
        match self {
            UpError::Config(_) => UpError::Config(message),
            UpError::Exec(_) => UpError::Exec(message),
            UpError::Timeout(_) => UpError::Timeout(message),
            UpError::Cache(_) => UpError::Cache(message),
            UpError::IOError(_) => UpError::IOError(message),
            UpError::HomebrewTapInUse | UpError::StepFailed(_, _) => unreachable!("handled above"),
        }
    }

    pub fn message(&self) -> String {
        match self {
            UpError::Config(message) => message.clone(),
            UpError::Exec(message) => message.clone(),
            UpError::Timeout(message) => message.clone(),
            UpError::Cache(message) => message.clone(),
            UpError::HomebrewTapInUse => "tap in use".to_string(),
            UpError::StepFailed(message, _) => message.clone(),
            UpError::IOError(message) => message.clone(),
        }
    }
}

#[cfg(test)]
mod with_context_tests {
    use super::*;

    /// The reported node failure read
    /// "execution error: failed to install packages: execution error: process
    /// exited with status 1" -- one prefix per layer of wrapping.
    #[test]
    fn adding_context_does_not_repeat_the_variant_prefix() {
        let inner = UpError::Exec("process exited with status 1".to_string());
        let outer = inner.with_context("failed to install packages");

        assert_eq!(
            outer.to_string(),
            "execution error: failed to install packages: process exited with status 1"
        );
        assert_eq!(outer.to_string().matches("execution error").count(), 1);
    }

    /// What the code did before: interpolating Display into a new Exec.
    #[test]
    fn interpolating_display_is_what_double_prefixed() {
        let inner = UpError::Exec("process exited with status 1".to_string());
        let doubled = UpError::Exec(format!("failed to install packages: {inner}"));

        assert_eq!(doubled.to_string().matches("execution error").count(), 2);
    }

    #[test]
    fn the_variant_is_preserved() {
        let timeout = UpError::Timeout("waited 60s".to_string()).with_context("installing node");
        assert!(matches!(timeout, UpError::Timeout(_)));
        assert_eq!(timeout.to_string(), "timeout: installing node: waited 60s");
    }

    #[test]
    fn context_stacks_without_accumulating_prefixes() {
        let err = UpError::Exec("boom".to_string())
            .with_context("inner")
            .with_context("outer");

        assert_eq!(err.to_string(), "execution error: outer: inner: boom");
        assert_eq!(err.to_string().matches("execution error").count(), 1);
    }

    /// StepFailed already means "rendered, carry identity upward"; its
    /// payload is a step name that gets formatted into the message, so
    /// prefixing it would corrupt the rendered name.
    #[test]
    fn step_failed_is_left_alone() {
        let step = UpError::StepFailed("node".to_string(), Some((2, 6)));
        let contextual = step.clone().with_context("should not appear");

        assert_eq!(contextual, step);
        assert_eq!(contextual.to_string(), "step 2/6 'node' failed");
    }

    #[test]
    fn tap_in_use_has_no_message_to_extend() {
        let err = UpError::HomebrewTapInUse.with_context("nope");
        assert_eq!(err, UpError::HomebrewTapInUse);
    }
}
