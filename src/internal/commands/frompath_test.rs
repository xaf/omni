use super::*;

mod from_source_file_header {
    use super::*;

    #[test]
    fn default() {
        let mut reader = BufReader::new("".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());

        let details = details.unwrap();
        assert_eq!(details.category, None);
        assert_eq!(details.help, None);
        assert!(matches!(
            details.autocompletion,
            CommandAutocompletion::Null
        ));
        assert_eq!(details.syntax, None);
        assert!(!details.sync_update);
    }

    #[test]
    fn simple() {
        let mut reader = BufReader::new("# category: test cat\n# help: test help\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert_eq!(details.category, Some(vec!["test cat".to_string()]));
        assert_eq!(details.help, Some("test help".to_string()));
    }

    #[test]
    fn help() {
        let mut reader = BufReader::new("# help: test help\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert_eq!(details.help, Some("test help".to_string()));
    }

    #[test]
    fn help_multiline_using_repeat() {
        let mut reader =
            BufReader::new("# help: test help\n# help: continued help\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert_eq!(details.help, Some("test help\ncontinued help".to_string()));
    }

    #[test]
    fn help_multiline_using_plus() {
        let mut reader = BufReader::new("# help: test help\n# +: continued help\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert_eq!(details.help, Some("test help\ncontinued help".to_string()));
    }

    #[test]
    fn category() {
        let mut reader = BufReader::new("# category: test cat\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert_eq!(details.category, Some(vec!["test cat".to_string()]));
    }

    #[test]
    fn category_splits_commas() {
        let mut reader = BufReader::new("# category: test cat, continued cat\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert_eq!(
            details.category,
            Some(vec!["test cat".to_string(), "continued cat".to_string()])
        );
    }

    #[test]
    fn category_multiline_appends_to_existing() {
        let mut reader =
            BufReader::new("# category: test cat\n# category: continued cat\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert_eq!(
            details.category,
            Some(vec!["test cat".to_string(), "continued cat".to_string()])
        );
    }

    #[test]
    fn category_multiline_splits_commas() {
        let mut reader = BufReader::new(
            "# category: test cat, other cat\n# category: continued cat, more cat\n".as_bytes(),
        );
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert_eq!(
            details.category,
            Some(vec![
                "test cat".to_string(),
                "other cat".to_string(),
                "continued cat".to_string(),
                "more cat".to_string()
            ])
        );
    }

    #[test]
    fn autocompletion() {
        let mut reader = BufReader::new("# autocompletion: true\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert!(matches!(
            details.autocompletion,
            CommandAutocompletion::Full
        ));
    }

    #[test]
    fn autocompletion_partial() {
        let mut reader = BufReader::new("# autocompletion: partial\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert!(matches!(
            details.autocompletion,
            CommandAutocompletion::Partial
        ));
    }

    #[test]
    fn autocompletion_false() {
        let mut reader = BufReader::new("# autocompletion: false\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert!(matches!(
            details.autocompletion,
            CommandAutocompletion::Null
        ));
    }

    #[test]
    fn argparser() {
        let mut reader = BufReader::new("# argparser: true\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert!(details.argparser);
    }

    #[test]
    fn argparser_false() {
        let mut reader = BufReader::new("# argparser: false\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert!(!details.argparser);
    }

    #[test]
    fn sync_update() {
        let mut reader = BufReader::new("# sync_update: true\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert!(details.sync_update);
    }

    #[test]
    fn sync_update_false() {
        let mut reader = BufReader::new("# sync_update: false\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert!(!details.sync_update);
    }

    #[test]
    fn arg_simple_short() {
        let mut reader = BufReader::new("# arg: -a: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_simple_long() {
        let mut reader = BufReader::new("# arg: --arg: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["--arg".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_simple_positional() {
        let mut reader = BufReader::new("# arg: arg: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["arg".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_without_description() {
        let mut reader = BufReader::new("# arg: -a\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                required: true,
                ..Default::default()
            }
        );
    }

    fn param_with_type(required: bool, type_str: &str, type_enum: SyntaxOptArgType) {
        let value = format!(
            "# {}: -a: type={}: test desc\n",
            if required { "arg" } else { "opt" },
            type_str
        );
        let mut reader = BufReader::new(value.as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required,
                arg_type: type_enum,
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_type_string() {
        param_with_type(true, "string", SyntaxOptArgType::String);
    }

    #[test]
    fn arg_with_type_int() {
        param_with_type(true, "int", SyntaxOptArgType::Integer);
    }

    #[test]
    fn arg_with_type_integer() {
        param_with_type(true, "integer", SyntaxOptArgType::Integer);
    }

    #[test]
    fn arg_with_type_float() {
        param_with_type(true, "float", SyntaxOptArgType::Float);
    }

    #[test]
    fn arg_with_type_bool() {
        param_with_type(true, "bool", SyntaxOptArgType::Boolean);
    }

    #[test]
    fn arg_with_type_boolean() {
        param_with_type(true, "boolean", SyntaxOptArgType::Boolean);
    }

    #[test]
    fn arg_with_type_flag() {
        param_with_type(true, "flag", SyntaxOptArgType::Flag);
    }

    #[test]
    fn arg_with_type_array_string() {
        param_with_type(
            true,
            "array/string",
            SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
        );
    }

    #[test]
    fn arg_with_type_array_int() {
        param_with_type(
            true,
            "array/int",
            SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Integer)),
        );
    }

    #[test]
    fn arg_with_type_array_integer() {
        param_with_type(
            true,
            "array/integer",
            SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Integer)),
        );
    }

    #[test]
    fn arg_with_type_array_float() {
        param_with_type(
            true,
            "array/float",
            SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Float)),
        );
    }

    #[test]
    fn arg_with_type_array_bool() {
        param_with_type(
            true,
            "array/bool",
            SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Boolean)),
        );
    }

    #[test]
    fn arg_with_type_array_boolean() {
        param_with_type(
            true,
            "array/boolean",
            SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Boolean)),
        );
    }

    #[test]
    fn arg_with_delimiter() {
        let mut reader = BufReader::new("# arg: -a: delimiter=,: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                value_delimiter: Some(','),
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_last() {
        let mut reader = BufReader::new("# arg: -a: last=true: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                last_arg_double_hyphen: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_leftovers_dots() {
        let mut reader = BufReader::new("# arg: a...: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                leftovers: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_leftovers_no_dots() {
        let mut reader = BufReader::new("# arg: -a: leftovers=true: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                leftovers: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_allow_hyphen_values() {
        let mut reader = BufReader::new("# arg: -a: allow_hyphen=true: test desc\n# arg: -b: allow_hyphen_values=true: test desc2".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 2);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                allow_hyphen_values: true,
                ..Default::default()
            }
        );

        let arg = &syntax.parameters[1];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-b".to_string()],
                desc: Some("test desc2".to_string()),
                required: true,
                allow_hyphen_values: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_allow_negative_numbers() {
        let mut reader = BufReader::new("# arg: -a: allow_negative_numbers=true: test desc\n# arg: -b: negative_numbers=true: test desc2".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 2);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                allow_negative_numbers: true,
                ..Default::default()
            }
        );

        let arg = &syntax.parameters[1];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-b".to_string()],
                desc: Some("test desc2".to_string()),
                required: true,
                allow_negative_numbers: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_requires_single() {
        let mut reader = BufReader::new("# arg: -a: requires=b: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                requires: vec!["b".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_requires_multiple() {
        let mut reader = BufReader::new("# arg: -a: requires=b c: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                requires: vec!["b".to_string(), "c".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_requires_multiple_repeat() {
        let mut reader =
            BufReader::new("# arg: -a: requires=b: requires=c: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                requires: vec!["b".to_string(), "c".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_conflicts_with() {
        let mut reader = BufReader::new("# arg: -a: conflicts_with=b: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                conflicts_with: vec!["b".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_conflits_with_multiple() {
        let mut reader =
            BufReader::new("# arg: -a: conflicts_with=b c: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                conflicts_with: vec!["b".to_string(), "c".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_conflits_with_multiple_repeat() {
        let mut reader = BufReader::new(
            "# arg: -a: conflicts_with=b: conflicts_with=c: test desc\n".as_bytes(),
        );
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                conflicts_with: vec!["b".to_string(), "c".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_required_without() {
        let mut reader =
            BufReader::new("# arg: -a: required_without=b: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                required_without: vec!["b".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_required_without_multiple() {
        let mut reader =
            BufReader::new("# arg: -a: required_without=b c: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                required_without: vec!["b".to_string(), "c".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_required_without_multiple_repeat() {
        let mut reader = BufReader::new(
            "# arg: -a: required_without=b: required_without=c: test desc\n".as_bytes(),
        );
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                required_without: vec!["b".to_string(), "c".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_required_without_all() {
        let mut reader =
            BufReader::new("# arg: -a: required_without_all=b c: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                required_without_all: vec!["b".to_string(), "c".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_required_if_eq() {
        let mut reader =
            BufReader::new("# arg: -a: required_if_eq=b c=5: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                required_if_eq: HashMap::from_iter(vec![
                    ("b".to_string(), "".to_string()),
                    ("c".to_string(), "5".to_string())
                ]),
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_required_if_eq_all() {
        let mut reader =
            BufReader::new("# arg: -a: required_if_eq_all=b c=5 d=10: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                required_if_eq_all: HashMap::from_iter(vec![
                    ("b".to_string(), "".to_string()),
                    ("c".to_string(), "5".to_string()),
                    ("d".to_string(), "10".to_string())
                ]),
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_with_default() {
        let mut reader = BufReader::new("# arg: -a: default=5: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: true,
                default: Some("5".to_string()),
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_multiline_between_options_using_repeat() {
        let mut reader = BufReader::new(
            "# arg: -a: type=int\n# arg: -a: delimiter=,\n# arg: -a: test desc\n".as_bytes(),
        );
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                arg_type: SyntaxOptArgType::Integer,
                value_delimiter: Some(','),
                required: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_multiline_between_options_using_plus() {
        let mut reader = BufReader::new(
            "# arg: -a: type=int\n# +: delimiter=,\n# +: test desc\n".as_bytes(),
        );
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                arg_type: SyntaxOptArgType::Integer,
                value_delimiter: Some(','),
                required: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_multiline_description_using_repeat() {
        let mut reader =
            BufReader::new("# arg: -a: test desc\n# arg: -a: continued desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc\ncontinued desc".to_string()),
                required: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn arg_multiline_description_using_plus() {
        let mut reader =
            BufReader::new("# arg: -a: test desc\n# +: continued desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc\ncontinued desc".to_string()),
                required: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn opt_simple_short() {
        let mut reader = BufReader::new("# opt: -a: test desc\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 1);

        let arg = &syntax.parameters[0];
        assert_eq!(
            arg,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                required: false,
                ..Default::default()
            }
        );
    }

    #[test]
    fn arggroup_simple() {
        let mut reader = BufReader::new("# arggroup: a_group: a\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.groups.len(), 1);

        let group = &syntax.groups[0];
        assert_eq!(
            group,
            &SyntaxGroup {
                name: "a_group".to_string(),
                parameters: vec!["a".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arggroup_multiple() {
        let mut reader =
            BufReader::new("# arggroup: a_group: multiple=true: a b c\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.groups.len(), 1);

        let group = &syntax.groups[0];
        assert_eq!(
            group,
            &SyntaxGroup {
                name: "a_group".to_string(),
                multiple: true,
                parameters: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arggroup_required() {
        let mut reader =
            BufReader::new("# arggroup: a_group: required=true: a b c\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.groups.len(), 1);

        let group = &syntax.groups[0];
        assert_eq!(
            group,
            &SyntaxGroup {
                name: "a_group".to_string(),
                required: true,
                parameters: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arggroup_conflicts_with() {
        let mut reader =
            BufReader::new("# arggroup: a_group: conflicts_with=b_group: a b c\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.groups.len(), 1);

        let group = &syntax.groups[0];
        assert_eq!(
            group,
            &SyntaxGroup {
                name: "a_group".to_string(),
                conflicts_with: vec!["b_group".to_string()],
                parameters: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arggroup_requires() {
        let mut reader =
            BufReader::new("# arggroup: a_group: requires=b_group: a b c\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.groups.len(), 1);

        let group = &syntax.groups[0];
        assert_eq!(
            group,
            &SyntaxGroup {
                name: "a_group".to_string(),
                requires: vec!["b_group".to_string()],
                parameters: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arggroup_repeat() {
        let mut reader = BufReader::new(
            "# arggroup: a_group: a b c\n# arggroup: a_group: d e f\n".as_bytes(),
        );
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();

        assert_eq!(syntax.groups.len(), 1);

        let group = &syntax.groups[0];
        assert_eq!(
            group,
            &SyntaxGroup {
                name: "a_group".to_string(),
                parameters: vec![
                    "a".to_string(),
                    "b".to_string(),
                    "c".to_string(),
                    "d".to_string(),
                    "e".to_string(),
                    "f".to_string()
                ],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arggroup_repeat_plus() {
        let mut reader = BufReader::new("# arggroup: a_group: a b c\n# +: d e f\n".as_bytes());
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();

        assert_eq!(syntax.groups.len(), 1);

        let group = &syntax.groups[0];
        assert_eq!(
            group,
            &SyntaxGroup {
                name: "a_group".to_string(),
                parameters: vec![
                    "a".to_string(),
                    "b".to_string(),
                    "c".to_string(),
                    "d".to_string(),
                    "e".to_string(),
                    "f".to_string()
                ],
                ..Default::default()
            }
        );
    }

    #[test]
    fn arggroup_repeat_with_required() {
        let mut reader = BufReader::new(
            "# arggroup: a_group: required=true\n# arggroup: a_group: a b c\n".as_bytes(),
        );
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some(), "Details are not present");
        let details = details.unwrap();

        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();

        assert_eq!(syntax.groups.len(), 1);

        let group = &syntax.groups[0];
        assert_eq!(
            group,
            &SyntaxGroup {
                name: "a_group".to_string(),
                required: true,
                parameters: vec!["a".to_string(), "b".to_string(), "c".to_string(),],
                ..Default::default()
            }
        );
    }

    #[test]
    fn complex_multiline_everywhere() {
        let mut reader = BufReader::new(
            "# category: test cat\n# +: more cat\n# autocompletion: true\n# argparser: true\n# sync_update: false\n# help: test help\n# +: more help\n# arg: -a: type=int\n# +: delimiter=,\n# +: test desc\n# opt: -b: type=string\n# +: delimiter=|\n# +: test desc\n# arggroup: a_group: multiple=true: a".as_bytes(),
        );
        let details = PathCommandFileDetails::from_source_file_header(
            &mut reader,
            &ConfigErrorHandler::noop(),
        );

        assert!(details.is_some());
        let details = details.unwrap();

        assert_eq!(
            details.category,
            Some(vec!["test cat".to_string(), "more cat".to_string()])
        );
        assert_eq!(details.help, Some("test help\nmore help".to_string()));
        assert!(matches!(
            details.autocompletion,
            CommandAutocompletion::Full
        ));
        assert!(details.argparser);
        assert!(!details.sync_update);
        assert!(details.syntax.is_some(), "Syntax is not present");

        let syntax = details.syntax.unwrap();
        assert_eq!(syntax.parameters.len(), 2);

        let arg_a = &syntax.parameters[0];
        assert_eq!(
            arg_a,
            &SyntaxOptArg {
                names: vec!["-a".to_string()],
                desc: Some("test desc".to_string()),
                arg_type: SyntaxOptArgType::Integer,
                value_delimiter: Some(','),
                required: true,
                ..Default::default()
            }
        );

        let arg_b = &syntax.parameters[1];
        assert_eq!(
            arg_b,
            &SyntaxOptArg {
                names: vec!["-b".to_string()],
                desc: Some("test desc".to_string()),
                arg_type: SyntaxOptArgType::String,
                value_delimiter: Some('|'),
                required: false,
                ..Default::default()
            }
        );

        assert_eq!(syntax.groups.len(), 1);

        let group_a = &syntax.groups[0];
        assert_eq!(
            group_a,
            &SyntaxGroup {
                name: "a_group".to_string(),
                multiple: true,
                parameters: vec!["a".to_string()],
                ..Default::default()
            }
        );
    }

    mod error_handling {
        use super::*;

        #[test]
        fn test_invalid_value_type_boolean() {
            let mut reader = BufReader::new("# autocompletion: not_a_bool\n".as_bytes());

            let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
            let _ =
                PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
            let errors = error_handler.errors();

            assert!(
                errors.iter().any(|err| {
                    matches!(err.kind(), ConfigErrorKind::MetadataHeaderInvalidValueType)
                        && err.lineno() == 1
                        && err.context_str("key") == "autocompletion"
                        && err.context_str("value") == "not_a_bool"
                        && err.context_str("expected") == "boolean"
                }),
                "Did not find expected error, found: {errors:?}"
            );
        }

        #[test]
        fn test_metadata_header_missing_help() {
            let mut reader = BufReader::new("# category: test\n".as_bytes());

            let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
            let _ =
                PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
            let errors = error_handler.errors();

            assert!(
                errors.iter().any(|err| matches!(
                    err.kind(),
                    ConfigErrorKind::MetadataHeaderMissingHelp
                )),
                "Did not find expected error, found: {errors:?}"
            );
        }

        #[test]
        fn test_metadata_header_missing_syntax() {
            let mut reader = BufReader::new("# help: test help\n".as_bytes());

            let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
            let _ =
                PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
            let errors = error_handler.errors();

            assert!(
                errors.iter().any(|err| matches!(
                    err.kind(),
                    ConfigErrorKind::MetadataHeaderMissingSyntax
                )),
                "Did not find expected error, found: {errors:?}"
            );
        }

        #[test]
        fn test_metadata_header_duplicate_key() {
            let mut reader =
                BufReader::new("# autocompletion: true\n# autocompletion: false\n".as_bytes());

            let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
            let _ =
                PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
            let errors = error_handler.errors();

            assert!(
                errors.iter().any(|err| {
                    matches!(err.kind(), ConfigErrorKind::MetadataHeaderDuplicateKey)
                        && err.lineno() == 2
                        && err.context_str("key") == "autocompletion"
                        && err.context_usize("prev_lineno") == 1
                }),
                "Did not find expected error, found: {errors:?}"
            );
        }

        #[test]
        fn test_metadata_header_unknown_key() {
            let mut reader =
                BufReader::new("# category: test\n# unknown_key: value\n".as_bytes());

            let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
            let _ =
                PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
            let errors = error_handler.errors();

            assert!(
                errors.iter().any(|err| {
                    matches!(err.kind(), ConfigErrorKind::MetadataHeaderUnknownKey)
                        && err.lineno() == 2
                        && err.context_str("key") == "unknown_key"
                }),
                "Did not find expected error, found: {errors:?}"
            );
        }

        #[test]
        fn test_metadata_header_group_empty_part() {
            let mut reader = BufReader::new("# arggroup: test_group: :\n".as_bytes());

            let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
            let _ =
                PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
            let errors = error_handler.errors();

            assert!(
                errors.iter().any(|err| {
                    matches!(err.kind(), ConfigErrorKind::MetadataHeaderGroupEmptyPart)
                        && err.context_str("group") == "test_group"
                }),
                "Did not find expected error, found: {errors:?}"
            );
        }

        #[test]
        fn test_metadata_header_group_unknown_config_key() {
            let mut reader =
                BufReader::new("# arggroup: test_group: unknown_key=value\n".as_bytes());

            let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
            let _ =
                PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
            let errors = error_handler.errors();

            assert!(
                errors.iter().any(|err| {
                    matches!(
                        err.kind(),
                        ConfigErrorKind::MetadataHeaderGroupUnknownConfigKey
                    ) && err.context_str("group") == "test_group"
                        && err.context_str("config_key") == "unknown_key"
                }),
                "Did not find expected error, found: {errors:?}"
            );
        }

        #[test]
        fn test_metadata_header_group_missing_parameters() {
            let mut reader = BufReader::new("# arggroup: test_group:\n".as_bytes());

            let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
            let _ =
                PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
            let errors = error_handler.errors();

            assert!(
                errors.iter().any(|err| {
                    matches!(
                        err.kind(),
                        ConfigErrorKind::MetadataHeaderGroupMissingParameters
                    ) && err.context_str("group") == "test_group"
                }),
                "Did not find expected error, found: {errors:?}"
            );
        }

        #[test]
        fn test_metadata_header_parameter_empty_part() {
            let mut reader = BufReader::new("# arg: test_param: :\n".as_bytes());

            let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
            let _ =
                PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
            let errors = error_handler.errors();

            assert!(
                errors.iter().any(|err| {
                    matches!(
                        err.kind(),
                        ConfigErrorKind::MetadataHeaderParameterEmptyPart
                    ) && err.context_str("parameter") == "test_param"
                }),
                "Did not find expected error, found: {errors:?}"
            );
        }

        #[test]
        fn test_metadata_header_parameter_invalid_key_value() {
            let mut reader =
                BufReader::new("# arg: test_param: delimiter=invalid\n".as_bytes());

            let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
            let _ =
                PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
            let errors = error_handler.errors();

            assert!(
                errors.iter().any(|err| {
                    matches!(
                        err.kind(),
                        ConfigErrorKind::MetadataHeaderParameterInvalidKeyValue
                    ) && err.context_str("parameter") == "test_param"
                        && err.context_str("key") == "delimiter"
                        && err.context_str("value") == "invalid"
                }),
                "Did not find expected error, found: {errors:?}"
            );
        }

        #[test]
        fn test_metadata_header_parameter_unknown_config_key() {
            let mut reader =
                BufReader::new("# arg: test_param: unknown_key=value\n".as_bytes());

            let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
            let _ =
                PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
            let errors = error_handler.errors();
            assert!(
                errors.iter().any(|err| {
                    matches!(
                        err.kind(),
                        ConfigErrorKind::MetadataHeaderParameterUnknownConfigKey
                    ) && err.context_str("parameter") == "test_param"
                        && err.context_str("config_key") == "unknown_key"
                }),
                "Did not find expected error, found: {errors:?}"
            );
        }

        #[test]
        fn test_metadata_header_parameter_missing_description() {
            let mut reader = BufReader::new("# arg: test_param:\n".as_bytes());

            let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
            let _ =
                PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
            let errors = error_handler.errors();
            assert!(
                errors.iter().any(|err| {
                    matches!(
                        err.kind(),
                        ConfigErrorKind::MetadataHeaderParameterMissingDescription
                    ) && err.context_str("parameter") == "test_param"
                }),
                "Did not find expected error, found: {errors:?}"
            );
        }

        #[test]
        fn test_metadata_header_continue_without_key() {
            let mut reader = BufReader::new("# +: continued value\n".as_bytes());

            let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
            let _ =
                PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
            let errors = error_handler.errors();
            assert!(
                errors.iter().any(|err| {
                    matches!(
                        err.kind(),
                        ConfigErrorKind::MetadataHeaderContinueWithoutKey
                    ) && err.lineno() == 1
                }),
                "Did not find expected error, found: {errors:?}"
            );
        }

        #[test]
        fn test_metadata_header_missing_subkey() {
            let mut reader = BufReader::new("# arg:\n".as_bytes());

            let error_handler = ConfigErrorHandler::new().with_file("myfile.txt");
            let _ =
                PathCommandFileDetails::from_source_file_header(&mut reader, &error_handler);
            let errors = error_handler.errors();
            assert!(
                errors.iter().any(|err| {
                    matches!(err.kind(), ConfigErrorKind::MetadataHeaderMissingSubkey)
                        && err.lineno() == 1
                        && err.context_str("key") == "arg"
                }),
                "Did not find expected error, found: {errors:?}"
            );
        }
    }
}