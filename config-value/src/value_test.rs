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
