use super::*;

#[test]
fn test_from_key_append() {
    let (key, strategy) = ExtendStrategy::from_key("mykey__toappend");
    assert_eq!(key, "mykey");
    assert_eq!(strategy, Some(ExtendStrategy::Append));
}

#[test]
fn test_from_key_prepend() {
    let (key, strategy) = ExtendStrategy::from_key("mykey__toprepend");
    assert_eq!(key, "mykey");
    assert_eq!(strategy, Some(ExtendStrategy::Prepend));
}

#[test]
fn test_from_key_replace() {
    let (key, strategy) = ExtendStrategy::from_key("mykey__toreplace");
    assert_eq!(key, "mykey");
    assert_eq!(strategy, Some(ExtendStrategy::Replace));
}

#[test]
fn test_from_key_keep() {
    let (key, strategy) = ExtendStrategy::from_key("mykey__ifnone");
    assert_eq!(key, "mykey");
    assert_eq!(strategy, Some(ExtendStrategy::Keep));
}

#[test]
fn test_from_key_no_suffix() {
    let (key, strategy) = ExtendStrategy::from_key("mykey");
    assert_eq!(key, "mykey");
    assert_eq!(strategy, None);
}
