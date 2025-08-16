use super::*;

#[test]
fn test_basic_matching() {
    let patterns = vec![
        "!example1.com/org/forbidden".to_string(),
        "example1.com/org/*".to_string(),
    ];

    // Should match with or without protocol
    assert!(check_url_allowed("example1.com/org/allowed", &patterns));
    assert!(check_url_allowed(
        "https://example1.com/org/allowed",
        &patterns
    ));
    assert!(check_url_allowed(
        "http://example1.com/org/allowed",
        &patterns
    ));

    // No protocol in URL should match any of the two
    assert!(!check_url_allowed("example1.com/org/forbidden", &patterns));
    assert!(!check_url_allowed(
        "http://example1.com/org/forbidden",
        &patterns
    ));

    // Should not match because different address than allowed
    assert!(!check_url_allowed("example2.com/org/repo", &patterns));
}

#[test]
fn test_protocol_matching() {
    let patterns = vec![
        "https://example1.com/*".to_string(),
        "!http://example1.com/*".to_string(),
    ];

    assert!(check_url_allowed(
        "https://example1.com/org/repo",
        &patterns
    ));
    assert!(!check_url_allowed(
        "http://example1.com/org/repo",
        &patterns
    ));

    // No protocol in URL should not match any of the two, hence
    // match since ending with a deny pattern
    assert!(check_url_allowed("example1.com/org/repo", &patterns));
}

#[test]
fn test_port_matching() {
    let patterns = vec![
        "!example1.com:8123/*".to_string(),
        "example1.com:8*/*".to_string(),
        "example2.com:*/*".to_string(),
    ];

    assert!(check_url_allowed("example1.com:8080/repo", &patterns));
    assert!(check_url_allowed("example1.com:80/repo", &patterns));
    assert!(check_url_allowed("example2.com:1234/repo", &patterns));
    assert!(check_url_allowed("example2.com/repo", &patterns));
    assert!(!check_url_allowed("example1.com:8123/repo", &patterns));
    assert!(!check_url_allowed("example1.com:9090/repo", &patterns));
}

#[test]
fn test_auth_matching() {
    let patterns = vec![
        "user@example1.com/*".to_string(),
        "user:pass@example2.com/*".to_string(),
        "!baduser@example1.com/*".to_string(),
        "!*:*@example2.com/*".to_string(),
        "*".to_string(),
    ];

    assert!(check_url_allowed("user@example1.com/repo", &patterns));
    assert!(check_url_allowed("user:pass@example2.com/repo", &patterns));
    assert!(!check_url_allowed(
        "user:otherpass@example2.com/repo",
        &patterns
    ));
    assert!(check_url_allowed("example1.com/repo", &patterns)); // No auth specified
    assert!(!check_url_allowed("baduser@example1.com/repo", &patterns));
}

#[test]
fn test_path_matching() {
    let patterns = vec![
        "example1.com/org/*/src".to_string(),
        "example1.com/org/repo/**/test".to_string(),
        "!example1.com/org/*/docs".to_string(),
    ];

    assert!(check_url_allowed("example1.com/org/repo/src", &patterns));
    assert!(check_url_allowed(
        "example1.com/org/repo/deep/test",
        &patterns
    ));
    assert!(!check_url_allowed("example1.com/org/repo/docs", &patterns));
}

#[test]
fn test_query_matching() {
    let patterns = vec![
        "example1.com/*?branch=main".to_string(),
        "example2.com/*?branch=*".to_string(),
        "!example1.com/*?branch=dev".to_string(),
    ];

    assert!(check_url_allowed(
        "example1.com/repo?branch=main",
        &patterns
    ));
    assert!(check_url_allowed(
        "example2.com/repo?branch=anything",
        &patterns
    ));
    assert!(check_url_allowed("example2.com/repo", &patterns)); // No query specified
    assert!(!check_url_allowed(
        "example1.com/repo?branch=dev",
        &patterns
    ));
}

#[test]
fn test_fragment_matching() {
    let patterns = vec![
        "example1.com/*#readme".to_string(),
        "example2.com/*#*".to_string(),
        "!*.com/*#private".to_string(),
    ];

    assert!(check_url_allowed("example1.com/repo#readme", &patterns));
    assert!(check_url_allowed("example2.com/repo#anything", &patterns));
    assert!(check_url_allowed("example2.com/repo", &patterns)); // No fragment specified
    assert!(check_url_allowed("example2.com/repo#private", &patterns));
    assert!(!check_url_allowed("example1.com/repo#private", &patterns));
}

#[test]
fn test_default_behavior() {
    // Empty pattern list
    assert!(check_url_allowed("example1.com/repo", &[]));

    // Last pattern determines default
    let allow_patterns = vec![
        "example1.com/allowed/*".to_string(),
        "example2.com/*".to_string(),
    ];
    assert!(!check_url_allowed("example3.org/repo", &allow_patterns));

    let deny_patterns = vec![
        "example1.com/allowed/*".to_string(),
        "!example2.com/*".to_string(),
    ];
    assert!(check_url_allowed("example3.org/repo", &deny_patterns));
}

#[test]
fn test_invalid_urls() {
    let patterns = vec!["example1.com/*".to_string()];

    assert!(!check_url_allowed("not a url", &patterns));
    assert!(!check_url_allowed("http://", &patterns));
    assert!(!check_url_allowed("://invalid", &patterns));
}

#[test]
fn test_invalid_patterns() {
    let patterns = vec!["not a url".to_string(), "example1.com/*".to_string()];

    // Should ignore invalid pattern and match against valid one
    assert!(check_url_allowed("example1.com/repo", &patterns));
}