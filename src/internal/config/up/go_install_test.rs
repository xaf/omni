use super::*;

// Feuilletage imports for deserialization tests
use crate::internal::config::{FeuilletageConfigContext, FeuilletageConfigLevel, FeuilletageConfigSource};

// Helper to deserialize from YAML using feuilletage
fn deserialize_go_installs_yaml(yaml: &str) -> Result<UpConfigGoInstalls, feuilletage::Error> {
    let mut config = feuilletage::Config::default();
    config.load_yaml(
        yaml,
        FeuilletageConfigContext::new(FeuilletageConfigSource::Programmatic, FeuilletageConfigLevel::Local),
    );
    config.deserialize::<UpConfigGoInstalls>()
}

// ============================================================================
// UpConfigGoInstalls parsing tests
// ============================================================================

#[test]
fn test_go_installs_string_input() {
    // String input should parse as single tool with embedded version
    let config = deserialize_go_installs_yaml("github.com/owner/tool@v1.0.0").unwrap();
    assert_eq!(config.tools.len(), 1);
    assert_eq!(config.tools[0].path, "github.com/owner/tool");
    assert_eq!(config.tools[0].version, Some("v1.0.0".to_string()));
    assert!(config.tools[0].exact);
}

#[test]
fn test_go_installs_string_input_no_version() {
    // String input without version
    let config = deserialize_go_installs_yaml("github.com/owner/tool").unwrap();
    assert_eq!(config.tools.len(), 1);
    assert_eq!(config.tools[0].path, "github.com/owner/tool");
    assert_eq!(config.tools[0].version, None);
    assert!(!config.tools[0].exact);
}

#[test]
fn test_go_installs_array_input() {
    // Array input
    let yaml = r#"
- github.com/owner/tool1@v1.0.0
- github.com/owner/tool2
"#;
    let config = deserialize_go_installs_yaml(yaml).unwrap();
    assert_eq!(config.tools.len(), 2);
    assert_eq!(config.tools[0].path, "github.com/owner/tool1");
    assert_eq!(config.tools[0].version, Some("v1.0.0".to_string()));
    assert_eq!(config.tools[1].path, "github.com/owner/tool2");
    assert_eq!(config.tools[1].version, None);
}

#[test]
fn test_go_installs_object_with_path_key() {
    // Object with explicit "path" key is treated as single tool
    let yaml = r#"
path: github.com/owner/tool
version: "1.0.0"
upgrade: true
"#;
    let config = deserialize_go_installs_yaml(yaml).unwrap();
    assert_eq!(config.tools.len(), 1);
    assert_eq!(config.tools[0].path, "github.com/owner/tool");
    assert_eq!(config.tools[0].version, Some("1.0.0".to_string()));
    assert!(config.tools[0].upgrade);
}

#[test]
fn test_go_installs_map_notation() {
    // Map notation: keys become paths, scalar values become versions
    let yaml = r#"
github.com/owner/tool1: v1.0.0
github.com/owner/tool2: v2.0.0
"#;
    let config = deserialize_go_installs_yaml(yaml).unwrap();
    assert_eq!(config.tools.len(), 2);

    // Check tools are sorted by path (order_by = "path")
    assert_eq!(config.tools[0].path, "github.com/owner/tool1");
    assert_eq!(config.tools[0].version, Some("v1.0.0".to_string()));
    assert!(!config.tools[0].exact);
    assert_eq!(config.tools[1].path, "github.com/owner/tool2");
    assert_eq!(config.tools[1].version, Some("v2.0.0".to_string()));
    assert!(!config.tools[1].exact);
}

#[test]
fn test_go_installs_map_notation_with_object_values() {
    // Map notation with object values for extra config
    let yaml = r#"
github.com/owner/tool1:
  version: "1.0.0"
  upgrade: true
github.com/owner/tool2:
  version: "2.0.0"
  prerelease: true
"#;
    let config = deserialize_go_installs_yaml(yaml).unwrap();
    assert_eq!(config.tools.len(), 2);

    // Check sorted order
    assert_eq!(config.tools[0].path, "github.com/owner/tool1");
    assert_eq!(config.tools[0].version, Some("1.0.0".to_string()));
    assert!(config.tools[0].upgrade);

    assert_eq!(config.tools[1].path, "github.com/owner/tool2");
    assert_eq!(config.tools[1].version, Some("2.0.0".to_string()));
    assert!(config.tools[1].prerelease);
}

#[test]
fn test_go_installs_map_notation_sorting() {
    // Verify lexicographic sorting by path
    let yaml = r#"
z-path.com/tool: v1.0.0
a-path.com/tool: v2.0.0
m-path.com/tool: v3.0.0
"#;
    let config = deserialize_go_installs_yaml(yaml).unwrap();
    assert_eq!(config.tools.len(), 3);

    // Should be sorted: a, m, z
    assert_eq!(config.tools[0].path, "a-path.com/tool");
    assert_eq!(config.tools[1].path, "m-path.com/tool");
    assert_eq!(config.tools[2].path, "z-path.com/tool");
}

#[test]
fn test_go_install_dirs_single_value() {
    // dirs (via "dir") accepts single string
    let yaml = r#"
path: github.com/owner/tool
dir: /path/to/dir
"#;
    let config = deserialize_go_installs_yaml(yaml).unwrap();
    assert_eq!(config.tools.len(), 1);
    assert!(config.tools[0].dirs.contains("/path/to/dir"));
}

#[test]
fn test_go_install_dirs_array_value() {
    // dirs (via "dir") accepts array
    let yaml = r#"
path: github.com/owner/tool
dir:
  - /path/to/dir1
  - /path/to/dir2
"#;
    let config = deserialize_go_installs_yaml(yaml).unwrap();
    assert_eq!(config.tools.len(), 1);
    assert_eq!(config.tools[0].dirs.len(), 2);
    assert!(config.tools[0].dirs.contains("/path/to/dir1"));
    assert!(config.tools[0].dirs.contains("/path/to/dir2"));
}

#[test]
fn test_go_install_version_resolves_by_default() {
    let yaml = r#"
path: github.com/owner/tool
version: "1.0.0"
"#;
    let config = deserialize_go_installs_yaml(yaml).unwrap();
    assert!(!config.tools[0].exact);
}

#[test]
fn test_go_install_exact_default_without_version() {
    // exact defaults to false when version is not specified
    let yaml = r#"
path: github.com/owner/tool
"#;
    let config = deserialize_go_installs_yaml(yaml).unwrap();
    assert!(
        !config.tools[0].exact,
        "exact should default to false when version is not specified"
    );
}

#[test]
fn test_go_install_exact_can_be_enabled_explicitly() {
    let yaml = r#"
path: github.com/owner/tool
version: "1.0.0"
exact: true
"#;
    let config = deserialize_go_installs_yaml(yaml).unwrap();
    assert!(config.tools[0].exact);
}

#[test]
fn test_go_install_version_conflict_error() {
    // Version specified in both path and version field should set config_error
    let yaml = r#"
path: github.com/owner/tool@v1.0.0
version: "2.0.0"
"#;
    let config = deserialize_go_installs_yaml(yaml).unwrap();
    assert!(
        config.tools[0].config_error.is_some(),
        "config_error should be set when version is specified in both path and version field"
    );
    assert!(config.tools[0]
        .config_error
        .as_ref()
        .unwrap()
        .contains("both"));
}

#[test]
fn test_go_install_empty_default() {
    // Empty config should result in empty tools vec
    let config = deserialize_go_installs_yaml("").unwrap();
    assert!(config.tools.is_empty());
}

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
fn test_parse_go_install_path_via_deserialization() {
    // Test path@version parsing via deserialization

    // Valid: path with version
    let config = deserialize_go_installs_yaml("github.com/user/repo@v1.0.0").unwrap();
    assert_eq!(config.tools[0].path, "github.com/user/repo");
    assert_eq!(config.tools[0].version, Some("v1.0.0".to_string()));

    // Valid: path without version
    let config = deserialize_go_installs_yaml("github.com/user/repo").unwrap();
    assert_eq!(config.tools[0].path, "github.com/user/repo");
    assert_eq!(config.tools[0].version, None);

    // Valid: pseudo-version
    let config =
        deserialize_go_installs_yaml("github.com/user/repo@v0.0.0-20191109021931-daa7c04131f5")
            .unwrap();
    assert_eq!(config.tools[0].path, "github.com/user/repo");
    assert_eq!(
        config.tools[0].version,
        Some("v0.0.0-20191109021931-daa7c04131f5".to_string())
    );

    // Invalid: multiple @ - should still parse but record error
    let config = deserialize_go_installs_yaml("github.com/user/repo@tag@extra").unwrap();
    // The path should contain the first part up to second @
    // and version should be everything after first @
    // This tests that the parsing handles this gracefully
    assert!(
        config.tools[0].config_error.is_some()
            || config.tools[0]
                .version
                .as_ref()
                .map(|v| v.contains('@'))
                .unwrap_or(false),
        "Should either have config error or version containing @"
    );

    // Empty path should record an error
    let config = deserialize_go_installs_yaml("\"\"").unwrap();
    assert!(
        config.tools.is_empty() || config.tools[0].config_error.is_some(),
        "Empty path should be recorded as error"
    );
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
