use super::*;

#[test]
fn test_value_as_str() {
    let v = Value::String("hello".to_string());
    assert_eq!(v.as_str(), Some("hello"));

    let v = Value::Integer(42);
    assert_eq!(v.as_str(), None);
}

#[test]
fn test_value_as_bool() {
    let v = Value::Bool(true);
    assert_eq!(v.as_bool(), Some(true));

    let v = Value::String("true".to_string());
    assert_eq!(v.as_bool(), None);
}

#[test]
fn test_value_as_i64() {
    let v = Value::Integer(42);
    assert_eq!(v.as_i64(), Some(42));

    let v = Value::UnsignedInteger(100);
    assert_eq!(v.as_i64(), Some(100));

    let v = Value::Float(3.14);
    assert_eq!(v.as_i64(), None);
}

#[test]
fn test_value_as_u64() {
    let v = Value::UnsignedInteger(42);
    assert_eq!(v.as_u64(), Some(42));

    let v = Value::Integer(100);
    assert_eq!(v.as_u64(), Some(100));

    let v = Value::Integer(-5);
    assert_eq!(v.as_u64(), None);
}

#[test]
fn test_value_as_f64() {
    let v = Value::Float(3.14);
    assert_eq!(v.as_f64(), Some(3.14));

    let v = Value::Integer(42);
    assert_eq!(v.as_f64(), Some(42.0));

    let v = Value::UnsignedInteger(100);
    assert_eq!(v.as_f64(), Some(100.0));
}

#[test]
fn test_as_str_forced() {
    // String -> String
    let v = Value::String("hello".to_string());
    assert_eq!(v.as_str_forced(), Some("hello".to_string()));

    // Bool -> String
    let v = Value::Bool(true);
    assert_eq!(v.as_str_forced(), Some("true".to_string()));

    // Integer -> String
    let v = Value::Integer(42);
    assert_eq!(v.as_str_forced(), Some("42".to_string()));

    // Float -> String
    let v = Value::Float(3.14);
    assert_eq!(v.as_str_forced(), Some("3.14".to_string()));

    // Null -> None
    let v = Value::Null;
    assert_eq!(v.as_str_forced(), None);
}

#[test]
fn test_as_bool_forced() {
    // Bool -> Bool
    let v = Value::Bool(true);
    assert_eq!(v.as_bool_forced(), Some(true));

    // String "true" -> Bool
    let v = Value::String("true".to_string());
    assert_eq!(v.as_bool_forced(), Some(true));

    let v = Value::String("yes".to_string());
    assert_eq!(v.as_bool_forced(), Some(true));

    let v = Value::String("false".to_string());
    assert_eq!(v.as_bool_forced(), Some(false));

    let v = Value::String("no".to_string());
    assert_eq!(v.as_bool_forced(), Some(false));

    // Integer -> Bool
    let v = Value::Integer(0);
    assert_eq!(v.as_bool_forced(), Some(false));

    let v = Value::Integer(1);
    assert_eq!(v.as_bool_forced(), Some(true));

    let v = Value::Integer(-5);
    assert_eq!(v.as_bool_forced(), Some(true));

    // Float -> Bool
    let v = Value::Float(0.0);
    assert_eq!(v.as_bool_forced(), Some(false));

    let v = Value::Float(3.14);
    assert_eq!(v.as_bool_forced(), Some(true));

    // Invalid string
    let v = Value::String("maybe".to_string());
    assert_eq!(v.as_bool_forced(), None);
}

#[test]
fn test_as_i64_forced() {
    // Integer -> Integer
    let v = Value::Integer(42);
    assert_eq!(v.as_i64_forced(), Some(42));

    // UnsignedInteger -> Integer
    let v = Value::UnsignedInteger(100);
    assert_eq!(v.as_i64_forced(), Some(100));

    // Float -> Integer (truncate)
    let v = Value::Float(3.14);
    assert_eq!(v.as_i64_forced(), Some(3));

    // String -> Integer (parse)
    let v = Value::String("42".to_string());
    assert_eq!(v.as_i64_forced(), Some(42));

    // Bool -> Integer
    let v = Value::Bool(true);
    assert_eq!(v.as_i64_forced(), Some(1));

    let v = Value::Bool(false);
    assert_eq!(v.as_i64_forced(), Some(0));

    // Invalid string
    let v = Value::String("not a number".to_string());
    assert_eq!(v.as_i64_forced(), None);
}

#[test]
fn test_as_f64_forced() {
    // Float -> Float
    let v = Value::Float(3.14);
    assert_eq!(v.as_f64_forced(), Some(3.14));

    // Integer -> Float
    let v = Value::Integer(42);
    assert_eq!(v.as_f64_forced(), Some(42.0));

    // String -> Float (parse)
    let v = Value::String("3.14".to_string());
    assert_eq!(v.as_f64_forced(), Some(3.14));

    // Bool -> Float
    let v = Value::Bool(true);
    assert_eq!(v.as_f64_forced(), Some(1.0));

    let v = Value::Bool(false);
    assert_eq!(v.as_f64_forced(), Some(0.0));
}

#[test]
fn test_from_serde_yaml() {
    let yaml_null = serde_yaml::Value::Null;
    assert_eq!(Value::from(yaml_null), Value::Null);

    let yaml_bool = serde_yaml::Value::Bool(true);
    assert_eq!(Value::from(yaml_bool), Value::Bool(true));

    let yaml_string = serde_yaml::Value::String("test".to_string());
    assert_eq!(Value::from(yaml_string), Value::String("test".to_string()));
}

#[test]
fn test_to_serde_yaml() {
    let v = Value::Null;
    let yaml: serde_yaml::Value = v.into();
    assert!(yaml.is_null());

    let v = Value::Bool(true);
    let yaml: serde_yaml::Value = v.into();
    assert_eq!(yaml.as_bool(), Some(true));

    let v = Value::String("test".to_string());
    let yaml: serde_yaml::Value = v.into();
    assert_eq!(yaml.as_str(), Some("test"));
}
