use super::*;

#[test]
fn test_validate_go_install_version() {
    let valid = vec!["v1.0.0", "latest", "v0.0.1", "master", "1234abcd"];
    for v in valid {
        assert!(
            validate_go_install_version(v).is_ok(),
            "Failed for valid version: {v}"
        );
    }

    let invalid = vec![
        "version with spaces",
        "v1.0.0@tag",
        "<1.0.0>",
        "v1.0.0;",
        "v1.0.0,next",
    ];
    for v in invalid {
        assert!(
            validate_go_install_version(v).is_err(),
            "Failed to reject invalid version: {v}"
        );
    }
}

#[test]
fn test_validate_go_install_path() {
    let test_cases = vec![
        ("github.com/user/repo", Ok("github.com/user/repo")),
        ("https://github.com/user/repo", Ok("github.com/user/repo")),
        ("//github.com/user/repo", Ok("github.com/user/repo")),
        ("github.com//user///repo", Ok("github.com/user/repo")),
        ("", Err("empty import path")),
        ("///", Err("empty path after cleaning")),
    ];

    for (input, expected) in test_cases {
        match validate_go_install_path(input) {
            Ok(path) => {
                assert_eq!(path, expected.unwrap(), "Failed for input: {input}");
            }
            Err(e) => {
                assert_eq!(
                    e.to_string(),
                    format!("invalid path: {}", expected.unwrap_err())
                );
            }
        }
    }
}

#[test]
fn test_parse_go_install_path() {
    let test_cases = vec![
        (
            "github.com/user/repo@v1.0.0",
            Ok((
                "github.com/user/repo".to_string(),
                Some("v1.0.0".to_string()),
            )),
        ),
        (
            "github.com/user/repo",
            Ok(("github.com/user/repo".to_string(), None)),
        ),
        (
            "github.com/user/repo@v0.0.0-20191109021931-daa7c04131f5",
            Ok((
                "github.com/user/repo".to_string(),
                Some("v0.0.0-20191109021931-daa7c04131f5".to_string()),
            )),
        ),
        (
            "github.com/user/repo@tag@extra",
            Err("multiple @ symbols found"),
        ),
        ("", Err("empty import path")),
    ];

    for (input, expected) in test_cases {
        match parse_go_install_path(input) {
            Ok(result) => {
                assert_eq!(result, expected.unwrap(), "Failed for input: {input}");
            }
            Err(e) => {
                assert_eq!(
                    e.to_string(),
                    format!("invalid path: {}", expected.unwrap_err())
                );
            }
        }
    }
}

#[test]
fn test_go_pseudo_versions() {
    let test_cases = vec![
        // Valid base format variations
        ("v0.0.0-20191109021931-daa7c04131f5", true),
        ("v1.0.0-20191109021931-daa7c04131f5", true),
        ("v2.0.0-20191109021931-daa7c04131f5", true),
        // Valid pre-release format variations
        ("v1.2.3-pre.0.20191109021931-daa7c04131f5", true),
        ("v1.2.3-alpha.0.20191109021931-daa7c04131f5", true),
        ("v1.2.3-beta.0.20191109021931-daa7c04131f5", true),
        ("v1.2.3-RC.0.20191109021931-daa7c04131f5", true),
        // Valid release format variations
        ("v1.2.4-0.20191109021931-daa7c04131f5", true),
        ("v2.3.4-0.20191109021931-daa7c04131f5", true),
        ("v99999.99999.99999-0.20191109021931-daa7c04131f5", true),
        ("v1.2.3-pre.0.20191109021931-AABBCCDDEE11", true),
        // Invalid version formats
        ("not-a-version", false),
        ("v1.0.0", false),
        ("v1.0.0-alpha", false),
        ("1.0.0-20191109021931-daa7c04131f5", false),
        ("v0-20191109021931-daa7c04131f5", false),
        ("v0.0-20191109021931-daa7c04131f5", false),
        ("v0.0.0.0-20191109021931-daa7c04131f5", false),
        ("va.0.0-20191109021931-daa7c04131f5", false),
        ("v0.b.0-20191109021931-daa7c04131f5", false),
        ("v0.0.c-20191109021931-daa7c04131f5", false),
        // Invalid timestamps
        ("v0.0.0-2019110902193-daa7c04131f5", false),
        ("v0.0.0-201911090219311-daa7c04131f5", false),
        ("v0.0.0-abcd11090219-daa7c04131f5", false),
        ("v0.0.0-abcdef123456-daa7c04131f5", false),
        ("v0.0.0-99999999999999-ffffffffffff", false),
        ("v0.0.0-00000000000000-000000000000", false),
        // Invalid hashes
        ("v0.0.0-20191109021931-daa7c0413", false),
        ("v0.0.0-20191109021931-short", false),
        ("v0.0.0-20191109021931-notahexnumber", false),
        ("v0.0.0-20191109021931-daa7c04131f5aa", false),
        ("v0.0.0-20191109021931-xyz7c04131f5", false),
        // Invalid separators and missing parts
        ("v0.0.0-20191109021931-", false),
        ("v0.0.0--daa7c04131f5", false),
        ("v0.0.0_20191109021931-daa7c04131f5", false),
        ("v0.0.0-20191109021931_daa7c04131f5", false),
    ];

    for (version, expected) in test_cases {
        assert_eq!(
            is_go_pseudo_version(version),
            expected,
            "Failed for version: {version} (expected: {expected})"
        );
    }
}
