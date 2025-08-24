use super::*;

#[test]
fn match_latest() {
    test_123_matches("latest");
}

#[test]
fn match_star() {
    test_123_matches("*");
}

#[test]
fn match_1() {
    test_123_matches("1");
}

#[test]
fn match_1_2() {
    test_123_matches("1.2");
}

#[test]
fn match_1_2_3() {
    test_123_matches("1.2.3");
}

#[test]
fn prefix_v() {
    let matcher = VersionMatcher::new("v1");
    assert!(matcher.matches("1.2.3"));
}

#[test]
fn prefix_any() {
    let matcher = VersionMatcher::new("jq-1");
    assert!(matcher.matches("jq-1.2.3"), "jq-1 should match jq-1.2.3");

    let mut matcher = VersionMatcher::new("1");
    matcher.prefix(true);
    assert!(
        matcher.matches("jq-1.2.3"),
        "1 should match jq-1.2.3 with matcher with prefix enabled"
    );
}

#[test]
fn exact_match() {
    let matcher = VersionMatcher::new("meerkat");
    assert!(matcher.matches("meerkat"));
}

#[test]
fn exact_match_with_build_and_no_build_matcher() {
    let matcher = VersionMatcher::new("1.2+build");
    assert!(matcher.matches("1.2+build"));
}

#[test]
fn exact_match_with_prerelease_and_no_prerelease_matcher() {
    let matcher = VersionMatcher::new("1.2-alpha");
    assert!(matcher.matches("1.2-alpha"));
}

#[test]
fn carot() {
    let matcher = VersionMatcher::new("^1.2.3");
    assert!(matcher.matches("1.2.3"), "^1.2.3 should match 1.2.3");
    assert!(matcher.matches("1.2.4"), "^1.2.3 should match 1.2.4");
    assert!(matcher.matches("1.3.0"), "^1.2.3 should match 1.3.0");
    assert!(!matcher.matches("2.0.0"), "^1.2.3 should NOT match 2.0.0");
}

#[test]
fn tilde() {
    let matcher = VersionMatcher::new("~1.2.3");
    assert!(matcher.matches("1.2.3"), "~1.2.3 should match 1.2.3");
    assert!(matcher.matches("1.2.4"), "~1.2.3 should match 1.2.4");
    assert!(!matcher.matches("1.3.0"), "~1.2.3 should NOT match 1.3.0");
    assert!(!matcher.matches("2.0.0"), "~1.2.3 should NOT match 2.0.0");
}

#[test]
fn gt() {
    let matcher = VersionMatcher::new(">1.2.3");
    assert!(!matcher.matches("1.2.3"), ">1.2.3 should NOT match 1.2.3");
    assert!(matcher.matches("1.2.4"), ">1.2.3 should match 1.2.4");
    assert!(matcher.matches("1.3.0"), ">1.2.3 should match 1.3.0");
    assert!(matcher.matches("2.0.0"), ">1.2.3 should match 2.0.0");
}

#[test]
fn gte() {
    let matcher = VersionMatcher::new(">=1.2.3");
    assert!(matcher.matches("1.2.3"), ">=1.2.3 should match 1.2.3");
    assert!(matcher.matches("1.2.4"), ">=1.2.3 should match 1.2.4");
    assert!(matcher.matches("1.3.0"), ">=1.2.3 should match 1.3.0");
    assert!(matcher.matches("2.0.0"), ">=1.2.3 should match 2.0.0");
}

#[test]
fn lt() {
    let matcher = VersionMatcher::new("<1.2.3");
    assert!(!matcher.matches("1.2.3"), "<1.2.3 should NOT match 1.2.3");
    assert!(!matcher.matches("1.2.4"), "<1.2.3 should NOT match 1.2.4");
    assert!(!matcher.matches("1.3.0"), "<1.2.3 should NOT match 1.3.0");
    assert!(matcher.matches("1.2.2"), "<1.2.3 should match 1.2.2");
    assert!(
        !matcher.matches("1.2.2-alpha"),
        "<1.2.3 should NOT match 1.2.2-alpha when prerelease is not allowed"
    );

    let mut matcher = VersionMatcher::new("<1.2.3");
    matcher.prerelease(true);
    assert!(
        matcher.matches("1.2.2-alpha"),
        "<1.2.3 should match 1.2.2-alpha when prerelease is allowed"
    );
}

#[test]
fn lte() {
    let matcher = VersionMatcher::new("<=1.2.3");
    assert!(matcher.matches("1.2.3"), "<=1.2.3 should match 1.2.3");
    assert!(matcher.matches("1.2.2"), "<=1.2.3 should match 1.2.2");
    assert!(matcher.matches("1.1.0"), "<=1.2.3 should match 1.1.0");
    assert!(!matcher.matches("1.3.0"), "<=1.2.3 should NOT match 1.3.0");
}

#[test]
fn match_1_x() {
    let matcher = VersionMatcher::new("1.x");
    assert!(matcher.matches("1.2.3"), "1.x should match 1.2.3");
    assert!(matcher.matches("1.3.0"), "1.x should match 1.3.0");
    assert!(!matcher.matches("2.0.0"), "1.x should NOT match 2.0.0");
}

#[test]
fn match_1_x_x() {
    let matcher = VersionMatcher::new("1.x.x");
    assert!(matcher.matches("1.2.3"), "1.x.x should match 1.2.3");
    assert!(matcher.matches("1.3.0"), "1.x.x should match 1.3.0");
    assert!(!matcher.matches("2.0.0"), "1.x.x should NOT match 2.0.0");
}

fn test_123_matches(version: &str) {
    let matcher = VersionMatcher::new(version);
    assert!(
        matcher.matches("1.2.3"),
        "{version} should match 1.2.3 with default matcher",
    );
    assert!(
        !matcher.matches("1.2.3-alpha"),
        "{version} should NOT match 1.2.3-alpha with default matcher",
    );
    assert!(
        !matcher.matches("1.2.3+build"),
        "{version} should NOT match 1.2.3+build with default matcher",
    );
    assert!(
        !matcher.matches("1.2.3-alpha+build"),
        "{version} should NOT match 1.2.3-alpha+build with default matcher",
    );

    let mut matcher = VersionMatcher::new(version);
    matcher.prerelease(true);
    assert!(
        matcher.matches("1.2.3"),
        "{version} should match 1.2.3 with matcher with prerelease enabled",
    );
    assert!(
        matcher.matches("1.2.3-alpha"),
        "{version} should match 1.2.3-alpha with matcher with prerelease enabled",
    );
    assert!(
        !matcher.matches("1.2.3+build"),
        "{version} should NOT match 1.2.3+build with matcher with prerelease enabled",
    );
    assert!(
        !matcher.matches("1.2.3-alpha+build"),
        "{version} should NOT match 1.2.3-alpha+build with matcher with prerelease enabled",
    );

    let mut matcher = VersionMatcher::new(version);
    matcher.build(true);
    assert!(
        matcher.matches("1.2.3"),
        "{version} should match 1.2.3 with matcher with build enabled",
    );
    assert!(
        !matcher.matches("1.2.3-alpha"),
        "{version} should NOT match 1.2.3-alpha with matcher with build enabled",
    );
    assert!(
        matcher.matches("1.2.3+build"),
        "{version} should match 1.2.3+build with matcher with build enabled",
    );
    assert!(
        !matcher.matches("1.2.3-alpha+build"),
        "{version} should NOT match 1.2.3-alpha+build with matcher with build enabled",
    );

    let mut matcher = VersionMatcher::new(version);
    matcher.prerelease(true);
    matcher.build(true);
    assert!(
        matcher.matches("1.2.3"),
        "{version} should match 1.2.3 with matcher with prerelease+build enabled",
    );
    assert!(
        matcher.matches("1.2.3-alpha"),
        "{version} should match 1.2.3-alpha with matcher with prerelease+build enabled",
    );
    assert!(
        matcher.matches("1.2.3+build"),
        "{version} should match 1.2.3+build with matcher with prerelease+build enabled",
    );
    assert!(
        matcher.matches("1.2.3-alpha+build"),
        "{version} should match 1.2.3-alpha+build with matcher with prerelease+build enabled",
    );
}

#[test]
fn version_parser_compare() {
    let values = vec![
        "v0.0.9",
        "v0.0.11",
        "awesome",
        "v0.0.1",
        "v0.0.9-rc1",
        "v0.0.9-beta",
        "v0.0.9-alpha",
        "v0.0.9-alpha.2",
    ];
    let expected = vec![
        "awesome",
        "v0.0.1",
        "v0.0.9-alpha",
        "v0.0.9-alpha.2",
        "v0.0.9-beta",
        "v0.0.9-rc1",
        "v0.0.9",
        "v0.0.11",
    ];

    let mut actual = values.clone();
    actual.sort_by(|a, b| VersionParser::compare(a, b));

    assert_eq!(actual, expected);
}

#[test]
fn version_matcher_new_handles_pyproject_toml_formats() {
    // Test quoted version ranges like ">='1.2'"
    let matcher = VersionMatcher::new("'>='1.2'");
    assert_eq!(matcher.expected_version, ">=1.2");

    // Test comma-separated ranges like ">=3.4.5,<4.0.0"
    let matcher = VersionMatcher::new(">=3.4.5,<4.0.0");
    assert_eq!(matcher.expected_version, ">=3.4.5 <4.0.0");

    // Test quoted comma-separated ranges
    let matcher = VersionMatcher::new("'>=1.0.0,<2.0.0'");
    assert_eq!(matcher.expected_version, ">=1.0.0 <2.0.0");

    // Test double-quoted ranges
    let matcher = VersionMatcher::new("\">=2.7,!=3.0.*,!=3.1.*,!=3.2.*\"");
    assert_eq!(matcher.expected_version, ">=2.7 !=3.0.* !=3.1.* !=3.2.*");

    // Test semicolon-separated ranges (alternative format)
    let matcher = VersionMatcher::new(">=1.0.0;<=2.0.0");
    assert_eq!(matcher.expected_version, ">=1.0.0 <=2.0.0");

    // Test mixed quotes and separators
    let matcher = VersionMatcher::new(">='1.2.3', !='1.3.0', <'2.0.0'");
    assert_eq!(matcher.expected_version, ">=1.2.3  !=1.3.0  <2.0.0");
}
