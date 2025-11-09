use super::*;
use crate::source::DefaultSource;
use crate::scope::DefaultScope;

#[test]
fn test_new_null() {
    let value: ConfigValue<DefaultSource, DefaultScope> = ConfigValue::new_null(
        DefaultSource,
        DefaultScope,
    );
    assert!(value.is_null());
}

#[test]
fn test_empty() {
    let value: ConfigValue<DefaultSource, DefaultScope> = ConfigValue::empty(
        DefaultSource,
        DefaultScope,
    );
    assert!(value.is_table());
    assert_eq!(value.as_table().unwrap().len(), 0);
}

#[test]
fn test_from_str() {
    let yaml = r#"
key: value
number: 42
"#;
    let value: ConfigValue<DefaultSource, DefaultScope> = ConfigValue::from_str(
        DefaultSource,
        DefaultScope,
        yaml,
    ).unwrap();

    assert!(value.is_table());
    assert_eq!(value.get("key").unwrap().as_str(), Some("value".to_string()));
    assert_eq!(value.get("number").unwrap().as_integer(), Some(42));
}

#[test]
fn test_dig() {
    let yaml = r#"
a:
  b:
    c: deep_value
"#;
    let value: ConfigValue<DefaultSource, DefaultScope> = ConfigValue::from_str(
        DefaultSource,
        DefaultScope,
        yaml,
    ).unwrap();

    let deep = value.dig(vec!["a", "b", "c"]).unwrap();
    assert_eq!(deep.as_str(), Some("deep_value".to_string()));
}

#[test]
fn test_extend_append() {
    let mut base: ConfigValue<DefaultSource, DefaultScope> = ConfigValue::from_str(
        DefaultSource,
        DefaultScope,
        "items: [1, 2]",
    ).unwrap();

    let other: ConfigValue<DefaultSource, DefaultScope> = ConfigValue::from_str(
        DefaultSource,
        DefaultScope,
        "items__toappend: [3, 4]",
    ).unwrap();

    base.extend(other, ExtendOptions::default(), vec![]);

    let items = base.get("items").unwrap().as_array().unwrap();
    assert_eq!(items.len(), 4);
}

#[test]
fn test_extend_keep() {
    let mut base: ConfigValue<DefaultSource, DefaultScope> = ConfigValue::from_str(
        DefaultSource,
        DefaultScope,
        "key: original",
    ).unwrap();

    let other: ConfigValue<DefaultSource, DefaultScope> = ConfigValue::from_str(
        DefaultSource,
        DefaultScope,
        "key__ifnone: replacement",
    ).unwrap();

    base.extend(other, ExtendOptions::default(), vec![]);

    // Should keep original value
    assert_eq!(base.get("key").unwrap().as_str(), Some("original".to_string()));
}

#[test]
fn test_unwrap() {
    let yaml = r#"
name: test
count: 42
nested:
  items: [1, 2, 3]
"#;
    let config_value: ConfigValue<DefaultSource, DefaultScope> = ConfigValue::from_str(
        DefaultSource,
        DefaultScope,
        yaml,
    ).unwrap();

    let value = config_value.unwrap();

    // Check it's a mapping
    assert!(value.is_mapping());
    let mapping = value.as_mapping().unwrap();

    // Check primitive values
    assert_eq!(mapping.get("name").unwrap().as_str(), Some("test"));
    assert_eq!(mapping.get("count").unwrap().as_i64(), Some(42));

    // Check nested mapping
    let nested = mapping.get("nested").unwrap().as_mapping().unwrap();
    let items = nested.get("items").unwrap().as_sequence().unwrap();
    assert_eq!(items.len(), 3);
}

#[test]
fn test_as_yaml() {
    let yaml = r#"
z_key: last
a_key: first
m_key: middle
"#;
    let config_value: ConfigValue<DefaultSource, DefaultScope> = ConfigValue::from_str(
        DefaultSource,
        DefaultScope,
        yaml,
    ).unwrap();

    let yaml_output = config_value.as_yaml();

    // Check that keys are sorted
    let lines: Vec<&str> = yaml_output.lines().collect();
    assert!(lines[0].starts_with("a_key:"));
    assert!(lines[1].starts_with("m_key:"));
    assert!(lines[2].starts_with("z_key:"));
}

#[test]
fn test_as_json() {
    let yaml = r#"
z_key: last
a_key: first
m_key: middle
"#;
    let config_value: ConfigValue<DefaultSource, DefaultScope> = ConfigValue::from_str(
        DefaultSource,
        DefaultScope,
        yaml,
    ).unwrap();

    let json_output = config_value.as_json();

    // Check it's valid JSON
    assert!(json_output.contains("\"a_key\""));
    assert!(json_output.contains("\"m_key\""));
    assert!(json_output.contains("\"z_key\""));

    // Parse to verify it's valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&json_output).unwrap();
    assert!(parsed.is_object());
}
