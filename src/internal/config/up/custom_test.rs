use super::*;

#[test]
fn test_valid_env_names() {
    assert!(is_valid_env_name("PATH"));
    assert!(is_valid_env_name("_PATH"));
    assert!(is_valid_env_name("MY_VAR_1"));
    assert!(!is_valid_env_name(""));
    assert!(!is_valid_env_name("1VAR"));
    assert!(!is_valid_env_name("MY-VAR"));
    assert!(!is_valid_env_name("MY VAR"));
}

#[test]
fn test_valid_delimiters() {
    assert!(is_valid_delimiter("EOF"));
    assert!(is_valid_delimiter("END"));
    assert!(is_valid_delimiter("DONE"));
    assert!(is_valid_delimiter("123"));
    assert!(is_valid_delimiter("MY_DELIMITER"));
    assert!(!is_valid_delimiter(""));
    assert!(!is_valid_delimiter("END HERE"));
    assert!(!is_valid_delimiter("END\n"));
}

mod parse_env_file_lines {
    use super::*;

    #[test]
    fn test_basic_env_operations() {
        let input = vec![
            "export PATH=/usr/bin",
            "LANG=en_US.UTF-8",
            "unset TEMP OLD_PATH",
            "# This is a comment",
            "",
            "JAVA_HOME=/opt/java",
        ];

        let result = parse_env_file_lines(input.into_iter()).unwrap();

        assert_eq!(result.len(), 5);

        assert_eq!(result[0].name, "PATH");
        assert_eq!(result[0].operation, EnvOperationEnum::Set);
        assert_eq!(result[0].value, Some("/usr/bin".to_string()));

        assert_eq!(result[1].name, "LANG");
        assert_eq!(result[1].operation, EnvOperationEnum::Set);
        assert_eq!(result[1].value, Some("en_US.UTF-8".to_string()));

        assert_eq!(result[2].name, "TEMP");
        assert_eq!(result[2].operation, EnvOperationEnum::Set);
        assert_eq!(result[2].value, None);

        assert_eq!(result[3].name, "OLD_PATH");
        assert_eq!(result[3].operation, EnvOperationEnum::Set);
        assert_eq!(result[3].value, None);

        assert_eq!(result[4].name, "JAVA_HOME");
        assert_eq!(result[4].operation, EnvOperationEnum::Set);
        assert_eq!(result[4].value, Some("/opt/java".to_string()));
    }

    #[test]
    fn test_advanced_operations() {
        let input = vec![
            "PATH>>=/new/bin",        // Append
            "LD_LIBRARY_PATH<<=/lib", // Prepend
            "PATH-=/old/bin",         // Remove
            "PREFIX<=value",          // Prefix
            "SUFFIX>=value",          // Suffix
        ];

        let result = parse_env_file_lines(input.into_iter()).unwrap();

        assert_eq!(result.len(), 5);

        assert_eq!(result[0].name, "PATH");
        assert_eq!(result[0].operation, EnvOperationEnum::Append);
        assert_eq!(result[0].value, Some("/new/bin".to_string()));

        assert_eq!(result[1].name, "LD_LIBRARY_PATH");
        assert_eq!(result[1].operation, EnvOperationEnum::Prepend);
        assert_eq!(result[1].value, Some("/lib".to_string()));

        assert_eq!(result[2].name, "PATH");
        assert_eq!(result[2].operation, EnvOperationEnum::Remove);
        assert_eq!(result[2].value, Some("/old/bin".to_string()));

        assert_eq!(result[3].name, "PREFIX");
        assert_eq!(result[3].operation, EnvOperationEnum::Prefix);
        assert_eq!(result[3].value, Some("value".to_string()));

        assert_eq!(result[4].name, "SUFFIX");
        assert_eq!(result[4].operation, EnvOperationEnum::Suffix);
        assert_eq!(result[4].value, Some("value".to_string()));
    }

    #[test]
    fn test_heredoc() {
        let input = vec![
            "MULTILINE<<EOF",
            "line 1",
            "line 2",
            "line 3",
            "EOF",
            "NEXT_VAR=value",
        ];

        let result = parse_env_file_lines(input.into_iter()).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "MULTILINE");
        assert_eq!(result[0].operation, EnvOperationEnum::Set);
        assert_eq!(result[0].value, Some("line 1\nline 2\nline 3".to_string()));

        assert_eq!(result[1].name, "NEXT_VAR");
        assert_eq!(result[1].operation, EnvOperationEnum::Set);
        assert_eq!(result[1].value, Some("value".to_string()));
    }

    #[test]
    fn test_heredoc_indentation() {
        let input = vec![
            "NORMAL<<-EOF",
            "    line 1",
            "      line 2",
            "    line 3",
            "EOF",
            "MINIMAL<<~EOF",
            "    line 1",
            "      line 2",
            "    line 3",
            "EOF",
        ];

        let result = parse_env_file_lines(input.into_iter()).unwrap();

        assert_eq!(result.len(), 2);

        // Test normal heredoc with -
        assert_eq!(result[0].name, "NORMAL");
        assert_eq!(result[0].operation, EnvOperationEnum::Set);
        assert_eq!(result[0].value, Some("line 1\nline 2\nline 3".to_string()));

        // Test minimal indentation heredoc with ~
        assert_eq!(result[1].name, "MINIMAL");
        assert_eq!(result[1].operation, EnvOperationEnum::Set);
        assert_eq!(
            result[1].value,
            Some("line 1\n  line 2\nline 3".to_string())
        );
    }

    #[test]
    fn test_invalid_operations() {
        // Invalid environment variable name
        let input = vec!["1INVALID=value"];
        assert!(parse_env_file_lines(input.into_iter()).is_err());

        // Invalid operation
        let input = vec!["VAR+=value"];
        assert!(parse_env_file_lines(input.into_iter()).is_err());

        // Missing heredoc terminator
        let input = vec!["VAR<<EOF", "content"];
        assert!(parse_env_file_lines(input.into_iter()).is_err());

        // Invalid heredoc delimiter
        let input = vec!["VAR<<EOF HERE", "content", "EOF HERE"];
        assert!(parse_env_file_lines(input.into_iter()).is_err());

        // Invalid unset syntax
        let input = vec!["unset 1INVALID"];
        assert!(parse_env_file_lines(input.into_iter()).is_err());
    }

    #[test]
    fn test_quoted_heredoc_delimiter() {
        let input = vec![
            "VAR<<'EOF'",
            "content",
            "EOF",
            "VAR2<<\"END\"",
            "more content",
            "END",
        ];

        let result = parse_env_file_lines(input.into_iter()).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].value, Some("content".to_string()));
        assert_eq!(result[1].value, Some("more content".to_string()));
    }

    #[test]
    fn test_mixed_operations() {
        let input = vec![
            "# Setting up environment",
            "JAVA_HOME=/opt/java",
            "PATH>>=$JAVA_HOME/bin",
            "unset OLD_JAVA_HOME TEMP_PATH",
            "CONFIG<<EOF",
            "key1=value1",
            "key2=value2",
            "EOF",
            "DEBUG>=true",
        ];

        let result = parse_env_file_lines(input.into_iter()).unwrap();

        assert_eq!(result.len(), 6);

        // Verify JAVA_HOME
        assert_eq!(result[0].name, "JAVA_HOME");
        assert_eq!(result[0].operation, EnvOperationEnum::Set);
        assert_eq!(result[0].value, Some("/opt/java".to_string()));

        // Verify PATH
        assert_eq!(result[1].name, "PATH");
        assert_eq!(result[1].operation, EnvOperationEnum::Append);
        assert_eq!(result[1].value, Some("$JAVA_HOME/bin".to_string()));

        // Verify unset operations
        assert_eq!(result[2].name, "OLD_JAVA_HOME");
        assert_eq!(result[2].operation, EnvOperationEnum::Set);
        assert_eq!(result[2].value, None);

        assert_eq!(result[3].name, "TEMP_PATH");
        assert_eq!(result[3].operation, EnvOperationEnum::Set);
        assert_eq!(result[3].value, None);

        // Verify heredoc
        assert_eq!(result[4].name, "CONFIG");
        assert_eq!(result[4].operation, EnvOperationEnum::Set);
        assert_eq!(
            result[4].value,
            Some("key1=value1\nkey2=value2".to_string())
        );

        // Verify prefix operation
        assert_eq!(result[5].name, "DEBUG");
        assert_eq!(result[5].operation, EnvOperationEnum::Suffix);
        assert_eq!(result[5].value, Some("true".to_string()));
    }
}
