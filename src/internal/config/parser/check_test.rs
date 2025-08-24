use super::*;

mod path_pattern_from_str {
    use super::*;

    #[test]
    fn test_global_patterns() {
        // Test absolute paths with global flag
        assert_eq!(
            path_pattern_from_str("/some/path", None, true),
            "/some/path"
        );

        // Test relative paths with global flag
        let current_dir = std::env::current_dir().expect("failed to get current dir");
        assert_eq!(
            path_pattern_from_str("some/path", None, true),
            current_dir.join("some/path").to_string_lossy().to_string(),
        );
    }

    #[test]
    fn test_negative_patterns() {
        // Test negative absolute path
        assert_eq!(
            path_pattern_from_str("!/some/path", None, true),
            "!/some/path"
        );

        // Test negative relative path with location
        assert_eq!(
            path_pattern_from_str("!relative/path", Some("/base/dir"), false),
            "!/base/dir/relative/path"
        );
    }

    #[test]
    fn test_glob_patterns() {
        // Test basic glob pattern
        assert_eq!(
            path_pattern_from_str("**/file.txt", None, false),
            "**/file.txt"
        );

        // Test double-star pattern
        assert_eq!(path_pattern_from_str("**", None, false), "**");

        // Test negative glob pattern
        assert_eq!(
            path_pattern_from_str("!**/file.txt", None, false),
            "!**/file.txt"
        );
    }

    #[test]
    fn test_relative_paths() {
        // Test relative path with location
        assert_eq!(
            path_pattern_from_str("relative/path", Some("/base/dir"), false),
            "/base/dir/relative/path"
        );

        // Test relative path without location (should use current dir)
        let current_dir = std::env::current_dir().expect("failed to get current dir");
        assert_eq!(
            path_pattern_from_str("relative/path", None, false),
            current_dir
                .join("relative/path")
                .to_string_lossy()
                .to_string(),
        );
    }

    #[test]
    fn test_trim_leading_slash() {
        // Test that leading slashes are trimmed for non-global patterns
        assert_eq!(
            path_pattern_from_str("/path/to/file", Some("/base/dir"), false),
            "/base/dir/path/to/file"
        );
    }

    #[test]
    fn test_edge_cases() {
        // Test empty pattern
        assert_eq!(
            path_pattern_from_str("", Some("/base/dir"), false),
            "/base/dir"
        );

        // Test single slash
        assert_eq!(path_pattern_from_str("/", None, true), "/");

        // Test negative empty pattern
        assert_eq!(
            path_pattern_from_str("!", Some("/base/dir"), false),
            "!/base/dir"
        );
    }
}
