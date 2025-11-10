use super::*;
use crate::Value;

#[derive(Clone)]
struct TestErrorHandler {
    expected: Option<Value>,
    actual: Option<Value>,
    index: Option<usize>,
    error_kind: Option<()>,
}

impl TestErrorHandler {
    fn new() -> Self {
        Self {
            expected: None,
            actual: None,
            index: None,
            error_kind: None,
        }
    }
}

impl ErrorHandler for TestErrorHandler {
    type ErrorKind = ();

    fn with_expected<V: Into<Value>>(mut self, expected: V) -> Self {
        self.expected = Some(expected.into());
        self
    }

    fn with_actual<S: crate::Source, C: crate::Scope>(
        mut self,
        actual: crate::ConfigValue<S, C>,
    ) -> Self {
        self.actual = Some(actual.unwrap());
        self
    }

    fn with_index(mut self, index: usize) -> Self {
        self.index = Some(index);
        self
    }

    fn error(mut self, kind: Self::ErrorKind) {
        self.error_kind = Some(kind);
    }
}

#[test]
fn test_with_expected_string() {
    let handler = TestErrorHandler::new();
    let handler = handler.with_expected("string");

    assert_eq!(handler.expected, Some(Value::String("string".to_string())));
}

#[test]
fn test_with_expected_vec_of_strings() {
    let handler = TestErrorHandler::new();
    let handler = handler.with_expected(vec!["string", "array of strings"]);

    assert_eq!(
        handler.expected,
        Some(Value::Sequence(vec![
            Value::String("string".to_string()),
            Value::String("array of strings".to_string())
        ]))
    );
}

#[test]
fn test_with_expected_value() {
    let handler = TestErrorHandler::new();
    let handler = handler.with_expected(Value::Bool(true));

    assert_eq!(handler.expected, Some(Value::Bool(true)));
}

#[test]
fn test_with_expected_multiple_types() {
    // Test i64
    let handler = TestErrorHandler::new().with_expected(42i64);
    assert_eq!(handler.expected, Some(Value::Integer(42)));

    // Test bool
    let handler = TestErrorHandler::new().with_expected(true);
    assert_eq!(handler.expected, Some(Value::Bool(true)));

    // Test f64
    let handler = TestErrorHandler::new().with_expected(3.14f64);
    assert_eq!(handler.expected, Some(Value::Float(3.14)));
}
