use super::*;

mod check_allowed {
    use super::*;

    #[test]
    fn test_empty_patterns() {
        assert!(check_allowed("any/path", &[]));
        assert!(check_allowed("", &[]));
    }

    #[test]
    fn test_exact_matches() {
        // Test exact path matching
        assert!(check_allowed("src/main.rs", &["src/main.rs".to_string()]));

        // Test exact path with deny
        assert!(!check_allowed("src/main.rs", &["!src/main.rs".to_string()]));
    }

    #[test]
    fn test_wildcard_patterns() {
        // Test with * wildcard
        assert!(check_allowed("src/test.rs", &["src/*.rs".to_string()]));

        // Test with multiple patterns including wildcard
        assert!(check_allowed(
            "src/lib.rs",
            &["src/*.rs".to_string(), "!src/test.rs".to_string()]
        ));

        // Test with ** wildcard
        assert!(check_allowed(
            "src/subfolder/test.rs",
            &["src/**/*.rs".to_string()]
        ));
    }

    #[test]
    fn test_directory_prefix_matching() {
        // Test directory prefix without trailing slash
        assert!(check_allowed(
            "src/subfolder/file.rs",
            &["src/subfolder".to_string()]
        ));

        // Test directory prefix with trailing slash
        assert!(check_allowed(
            "src/subfolder/file.rs",
            &["src/subfolder/".to_string()]
        ));

        // Test nested directory matching
        assert!(check_allowed(
            "src/deep/nested/file.rs",
            &["src/deep".to_string()]
        ));
    }

    #[test]
    fn test_multiple_patterns() {
        let patterns = vec![
            "src/secret/public.rs".to_string(),
            "!src/secret/**".to_string(),
            "src/**".to_string(),
        ];

        // Should match general src pattern
        assert!(check_allowed("src/main.rs", &patterns));

        // Should be denied by secret pattern
        assert!(!check_allowed("src/secret/private.rs", &patterns));

        // Should be explicitly allowed despite being in secret dir
        assert!(check_allowed("src/secret/public.rs", &patterns));
    }

    #[test]
    fn test_default_behavior() {
        // Test default allow (last pattern is negative)
        assert!(check_allowed(
            "random.txt",
            &["src/**".to_string(), "!tests/**".to_string()]
        ));

        // Test default deny (last pattern is positive)
        assert!(!check_allowed(
            "random.txt",
            &["!src/**".to_string(), "src/allowed.rs".to_string()]
        ));
    }

    #[test]
    fn test_pattern_priority() {
        let patterns = vec![
            "docs/internal/public/**".to_string(),
            "!docs/internal/**".to_string(),
            "docs/**".to_string(),
        ];

        // Should match third pattern
        assert!(check_allowed("docs/api.md", &patterns));

        // Should be denied by second pattern
        assert!(!check_allowed("docs/internal/secret.md", &patterns));

        // Should be allowed by first pattern
        assert!(check_allowed("docs/internal/public/readme.md", &patterns));
    }

    #[test]
    fn test_directory_prefix() {
        let patterns = vec!["src".to_string()];

        assert!(check_allowed("src", &patterns));
        assert!(check_allowed("src/test", &patterns));
        assert!(check_allowed("src/another/test", &patterns));
    }

    #[test]
    fn test_edge_cases() {
        // Test root pattern
        assert!(check_allowed("any/path", &["**".to_string()]));

        // Test single negative pattern
        assert!(!check_allowed("any/path", &["!**".to_string()]));

        // Test pattern with just trailing wildcard
        assert!(check_allowed("src/anything", &["src/*".to_string()]));
    }
}
