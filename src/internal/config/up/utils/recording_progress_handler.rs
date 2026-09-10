use std::sync::Arc;
use std::sync::Mutex;

use crate::internal::config::up::utils::ProgressHandler;

/// What a `ProgressHandler` was asked to render, in order.
///
/// Only the terminal calls carry meaning for duplicate-render tests, but
/// progress is recorded too so that a downgraded end call -- what a
/// subhandler does with `allow_ending == false` -- stays visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedCall {
    Progress(String),
    Success(Option<String>),
    Error(Option<String>),
}

/// A `ProgressHandler` that records rather than prints, so tests can assert
/// on how many times a single failure was rendered.
#[derive(Debug, Clone, Default)]
pub struct RecordingProgressHandler {
    calls: Arc<Mutex<Vec<RecordedCall>>>,
}

impl RecordingProgressHandler {
    pub fn new() -> Self {
        Self::default()
    }

    /// A handle onto the same log, so a test can still read the calls after
    /// the handler has been boxed into an `UpProgressHandler`.
    pub fn log(&self) -> Arc<Mutex<Vec<RecordedCall>>> {
        Arc::clone(&self.calls)
    }

    fn record(&self, call: RecordedCall) {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(call);
        }
    }
}

impl ProgressHandler for RecordingProgressHandler {
    fn println(&self, _message: String) {}

    fn progress(&self, message: String) {
        self.record(RecordedCall::Progress(message));
    }

    fn success(&self) {
        self.record(RecordedCall::Success(None));
    }

    fn success_with_message(&self, message: String) {
        self.record(RecordedCall::Success(Some(message)));
    }

    fn error(&self) {
        self.record(RecordedCall::Error(None));
    }

    fn error_with_message(&self, message: String) {
        self.record(RecordedCall::Error(Some(message)));
    }

    fn hide(&self) {}

    fn show(&self) {}
}
