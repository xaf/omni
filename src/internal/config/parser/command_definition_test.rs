use super::*;

fn disable_colors() {
    std::env::set_var("NO_COLOR", "true");
}

mod command_syntax {
    use super::*;

    mod check_parameters_unique_names {
        use super::*;

        #[test]
        fn test_params_dest() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        dest: Some("paramdest".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        dest: Some("paramdest".to_string()),
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let errmsg = "identifier paramdest is defined more than once";
            assert_eq!(
                syntax.check_parameters_unique_names(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_params_names() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string(), "--param2".to_string()],
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let errmsg = "name --param2 is defined more than once";
            assert_eq!(
                syntax.check_parameters_unique_names(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_params_and_groups() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    ..SyntaxOptArg::default()
                }],
                groups: vec![SyntaxGroup {
                    name: "param1".to_string(),
                    parameters: vec!["--param1".to_string()],
                    ..SyntaxGroup::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg = "identifier param1 is defined more than once";
            assert_eq!(
                syntax.check_parameters_unique_names(),
                Err(errmsg.to_string())
            );
        }
    }

    mod check_parameters_references {
        use super::*;

        #[test]
        fn test_param_requires() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    requires: vec!["--param2".to_string()],
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg =
                "parameter or group param2 specified in requires for param1 does not exist";
            assert_eq!(
                syntax.check_parameters_references(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_param_conflicts_with() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    conflicts_with: vec!["--param2".to_string()],
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg =
                "parameter or group param2 specified in conflicts_with for param1 does not exist";
            assert_eq!(
                syntax.check_parameters_references(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_param_required_without() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    required_without: vec!["--param2".to_string()],
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg =
                "parameter or group param2 specified in required_without for param1 does not exist";
            assert_eq!(
                syntax.check_parameters_references(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_param_required_without_all() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    required_without_all: vec!["--param2".to_string()],
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg =
                "parameter or group param2 specified in required_without_all for param1 does not exist";
            assert_eq!(
                syntax.check_parameters_references(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_param_required_if_eq() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    required_if_eq: HashMap::from_iter(vec![(
                        "param2".to_string(),
                        "value".to_string(),
                    )]),
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg =
                "parameter or group param2 specified in required_if_eq for param1 does not exist";
            assert_eq!(
                syntax.check_parameters_references(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_param_required_if_eq_all() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    required_if_eq_all: HashMap::from_iter(vec![(
                        "param2".to_string(),
                        "value".to_string(),
                    )]),
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg =
                "parameter or group param2 specified in required_if_eq_all for param1 does not exist";
            assert_eq!(
                syntax.check_parameters_references(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_group_parameters() {
            let syntax = CommandSyntax {
                groups: vec![SyntaxGroup {
                    name: "group1".to_string(),
                    parameters: vec!["--param1".to_string()],
                    ..SyntaxGroup::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg =
                "parameter or group param1 specified in parameters for group1 does not exist";
            assert_eq!(
                syntax.check_parameters_references(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_group_requires_group_exists() {
            let syntax = CommandSyntax {
                groups: vec![
                    SyntaxGroup {
                        name: "group1".to_string(),
                        parameters: vec![],
                        requires: vec!["group2".to_string()],
                        ..SyntaxGroup::default()
                    },
                    SyntaxGroup {
                        name: "group2".to_string(),
                        parameters: vec![],
                        ..SyntaxGroup::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            assert_eq!(syntax.check_parameters_references(), Ok(()));
        }

        #[test]
        fn test_group_requires_param_exists() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    ..SyntaxOptArg::default()
                }],
                groups: vec![SyntaxGroup {
                    name: "group1".to_string(),
                    parameters: vec![],
                    requires: vec!["param1".to_string()],
                    ..SyntaxGroup::default()
                }],
                ..CommandSyntax::default()
            };

            assert_eq!(syntax.check_parameters_references(), Ok(()));
        }

        #[test]
        fn test_group_requires() {
            let syntax = CommandSyntax {
                groups: vec![SyntaxGroup {
                    name: "group1".to_string(),
                    parameters: vec![],
                    requires: vec!["group2".to_string()],
                    ..SyntaxGroup::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg =
                "parameter or group group2 specified in requires for group1 does not exist";
            assert_eq!(
                syntax.check_parameters_references(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_group_conflicts_with() {
            let syntax = CommandSyntax {
                groups: vec![SyntaxGroup {
                    name: "group1".to_string(),
                    parameters: vec![],
                    conflicts_with: vec!["group2".to_string()],
                    ..SyntaxGroup::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg =
                "parameter or group group2 specified in conflicts_with for group1 does not exist";
            assert_eq!(
                syntax.check_parameters_references(),
                Err(errmsg.to_string())
            );
        }
    }

    mod check_parameters_leftovers {
        use super::*;

        #[test]
        fn test_use_more_than_once() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        leftovers: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["param2".to_string()],
                        leftovers: true,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let errmsg = "only one argument can use leftovers; found param1, param2";
            assert_eq!(syntax.check_parameters_leftovers(), Err(errmsg.to_string()));
        }

        #[test]
        fn test_use_before_last_positional() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["param2".to_string()],
                        leftovers: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["param3".to_string()],
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let errmsg = "only the last positional argument can use leftovers";
            assert_eq!(syntax.check_parameters_leftovers(), Err(errmsg.to_string()));
        }

        #[test]
        fn test_use_with_non_positional() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        leftovers: true,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let errmsg = "only positional arguments can use leftovers; found --param2";
            assert_eq!(syntax.check_parameters_leftovers(), Err(errmsg.to_string()));
        }
    }

    mod check_parameters_last {
        use super::*;

        #[test]
        fn test_non_positional() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    last_arg_double_hyphen: true,
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg = "only positional arguments can use last; found --param1";
            assert_eq!(syntax.check_parameters_last(), Err(errmsg.to_string()));
        }
    }

    mod check_parameters_counter {
        use super::*;

        #[test]
        fn test_positional() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["param1".to_string()],
                    arg_type: SyntaxOptArgType::Counter,
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg = "param1: counter argument cannot be positional";
            assert_eq!(syntax.check_parameters_counter(), Err(errmsg.to_string()));
        }

        #[test]
        fn test_num_values() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    arg_type: SyntaxOptArgType::Counter,
                    num_values: Some(SyntaxOptArgNumValues::Exactly(1)),
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg = "--param1: counter argument cannot have a num_values (counters do not take any values)";
            assert_eq!(syntax.check_parameters_counter(), Err(errmsg.to_string()));
        }
    }

    mod check_parameters_allow_hyphen_values {
        use super::*;

        #[test]
        fn test_num_values() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    arg_type: SyntaxOptArgType::String,
                    num_values: Some(SyntaxOptArgNumValues::Exactly(0)),
                    allow_hyphen_values: true,
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg = "--param1: cannot use allow_hyphen_values with 'num_values=0'";
            assert_eq!(
                syntax.check_parameters_allow_hyphen_values(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_counter() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    arg_type: SyntaxOptArgType::Counter,
                    allow_hyphen_values: true,
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg = "--param1: cannot use allow_hyphen_values on a counter";
            assert_eq!(
                syntax.check_parameters_allow_hyphen_values(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_flag() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    arg_type: SyntaxOptArgType::Flag,
                    allow_hyphen_values: true,
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg = "--param1: cannot use allow_hyphen_values on a flag";
            assert_eq!(
                syntax.check_parameters_allow_hyphen_values(),
                Err(errmsg.to_string())
            );
        }
    }

    mod check_parameters_positional {
        use super::*;

        #[test]
        fn test_positional_required_before_non_required() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        required: false,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["param2".to_string()],
                        required: true,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let errmsg =
                "param2: required positional argument cannot appear after non-required one param1";
            assert_eq!(
                syntax.check_parameters_positional(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_positional_num_values() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        num_values: Some(SyntaxOptArgNumValues::Exactly(2)),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["param2".to_string()],
                        required: false,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let errmsg = "param2: positional need to be required or use 'last=true' if appearing after param1 with num_values > 1";
            assert_eq!(
                syntax.check_parameters_positional(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_positional_num_values_ok_if_required() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        num_values: Some(SyntaxOptArgNumValues::Exactly(2)),
                        required: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["param2".to_string()],
                        required: true,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            assert_eq!(syntax.check_parameters_positional(), Ok(()));
        }

        #[test]
        fn test_positional_num_values_ok_if_last() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        num_values: Some(SyntaxOptArgNumValues::Exactly(2)),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["param2".to_string()],
                        last_arg_double_hyphen: true,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            assert_eq!(syntax.check_parameters_positional(), Ok(()));
        }

        #[test]
        fn test_positional_required_num_values_exactly_zero() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["param1".to_string()],
                    required: true,
                    num_values: Some(SyntaxOptArgNumValues::Exactly(0)),
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg = "param1: positional argument cannot have 'num_values=0'";
            assert_eq!(
                syntax.check_parameters_positional(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_positional_required_num_values_at_most_zero() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["param1".to_string()],
                    required: true,
                    num_values: Some(SyntaxOptArgNumValues::AtMost(0)),
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg = "param1: positional argument cannot have 'num_values=0'";
            assert_eq!(
                syntax.check_parameters_positional(),
                Err(errmsg.to_string())
            );
        }

        #[test]
        fn test_positional_required_num_values_between_max_zero() {
            disable_colors();

            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["param1".to_string()],
                    required: true,
                    num_values: Some(SyntaxOptArgNumValues::Between(0, 0)),
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let errmsg = "param1: positional argument cannot have 'num_values=0'";
            assert_eq!(
                syntax.check_parameters_positional(),
                Err(errmsg.to_string())
            );
        }
    }

    mod parse_args {
        use super::*;

        fn check_expectations(syntax: &CommandSyntax, expectations: &Vec<(&[&str], Option<&str>)>) {
            for (argv, expectation) in expectations {
                let parsed_args = syntax.parse_args(
                    argv.iter().map(|s| s.to_string()).collect(),
                    vec!["test".to_string()],
                );
                match &expectation {
                    Some(errmsg) => match &parsed_args {
                        Ok(_args) => {
                            panic!("case with args {argv:?} should have failed but succeeded")
                        }
                        Err(e) => assert_eq!((argv, e.simple()), (argv, errmsg.to_string())),
                    },
                    None => {
                        if let Err(ref e) = parsed_args {
                            panic!("case with args {argv:?} should have succeeded but failed with error: {e}");
                        }
                    }
                }
            }
        }

        fn check_type_expectations(
            arg_name: &str,
            arg_type: &str,
            syntax: &CommandSyntax,
            expectations: &Vec<(Vec<&str>, Result<&str, &str>)>,
        ) {
            for (argv, expectation) in expectations {
                let args = match syntax.parse_args(
                    argv.iter().map(|s| s.to_string()).collect(),
                    vec!["test".to_string()],
                ) {
                    Ok(args) => {
                        if expectation.is_err() {
                            panic!("{argv:?} should have failed")
                        }
                        args
                    }
                    Err(e) => {
                        if let Err(expect_err) = &expectation {
                            assert_eq!((&argv, e.simple()), (&argv, expect_err.to_string()));
                            continue;
                        }
                        panic!("{argv:?} should have succeeded, instead: {e}");
                    }
                };

                let value = expectation.expect("should not get here if not Ok");

                let type_var = format!("OMNI_ARG_{}_TYPE", arg_name.to_uppercase());
                let value_var = format!("OMNI_ARG_{}_VALUE", arg_name.to_uppercase());

                let mut expectations = vec![("OMNI_ARG_LIST", arg_name), (&type_var, arg_type)];
                if !value.is_empty() {
                    expectations.push((&value_var, value));
                }

                let expectations_len = expectations.len();
                for (key, value) in expectations {
                    assert_eq!(
                        (&argv, key, args.get(key)),
                        (&argv, key, Some(&value.to_string()))
                    );
                }
                assert_eq!((&argv, args.len()), (&argv, expectations_len));
            }
        }

        #[test]
        fn test_simple() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        required: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        desc: Some("takes an int".to_string()),
                        required: false,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let args = match syntax.parse_args(
                ["--param1", "value1", "--param2", "42"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                vec!["test".to_string()],
            ) {
                Ok(args) => args,
                Err(e) => panic!("{}", e),
            };

            let expectations = vec![
                ("OMNI_ARG_LIST", "param1 param2"),
                ("OMNI_ARG_PARAM1_TYPE", "str"),
                ("OMNI_ARG_PARAM1_VALUE", "value1"),
                ("OMNI_ARG_PARAM2_TYPE", "int"),
                ("OMNI_ARG_PARAM2_VALUE", "42"),
            ];

            assert_eq!(args.len(), expectations.len());
            for (key, value) in expectations {
                assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
            }
        }

        #[test]
        fn test_value_string() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    arg_type: SyntaxOptArgType::String,
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(Vec<&str>, Result<&str, &str>)> = vec![
                (vec!["--param1", "value1"], Ok("value1")),
                (vec!["--param1", ""], Ok("")),
                (vec!["--param1", "1"], Ok("1")),
                (vec!["--param1", "value1,value2"], Ok("value1,value2")),
            ];

            check_type_expectations("param1", "str", &syntax, &expectations);
        }

        #[test]
        fn test_value_int() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    arg_type: SyntaxOptArgType::Integer,
                    allow_hyphen_values: true,
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(Vec<&str>, Result<&str, &str>)> = vec![
                (vec!["--param1", "1"], Ok("1")),
                (vec!["--param1", "10"], Ok("10")),
                (vec!["--param1", "0"], Ok("0")),
                (vec!["--param1", "-100"], Ok("-100")),
                (vec!["--param1", ""], Err("invalid value '' for '--param1 <param1>': cannot parse integer from empty string")),
                (vec!["--param1", "1.2"], Err("invalid value '1.2' for '--param1 <param1>': invalid digit found in string")),
                (vec!["--param1", "1,2"], Err("invalid value '1,2' for '--param1 <param1>': invalid digit found in string")),
            ];

            check_type_expectations("param1", "int", &syntax, &expectations);
        }

        #[test]
        fn test_value_float() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    arg_type: SyntaxOptArgType::Float,
                    allow_hyphen_values: true,
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(Vec<&str>, Result<&str, &str>)> = vec![
                (vec!["--param1", "1.978326"], Ok("1.978326")),
                (vec!["--param1", "10"], Ok("10")),
                (vec!["--param1", "0"], Ok("0")),
                (vec!["--param1", "-100.4"], Ok("-100.4")),
                (vec!["--param1", ""], Err("invalid value '' for '--param1 <param1>': cannot parse float from empty string")),
                (vec!["--param1", "1.2"], Ok("1.2")),
                (vec!["--param1", "1,2"], Err("invalid value '1,2' for '--param1 <param1>': invalid float literal")),
            ];

            check_type_expectations("param1", "float", &syntax, &expectations);
        }

        #[test]
        fn test_value_bool() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    arg_type: SyntaxOptArgType::Boolean,
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(Vec<&str>, Result<&str, &str>)> = vec![
                (vec!["--param1", "true"], Ok("true")),
                (vec!["--param1", "false"], Ok("false")),
                (vec!["--param1", ""], Err("a value is required for '--param1 <param1>' but none was supplied [possible values: true, false]")),
                (vec!["--param1", "TRUE"], Err("invalid value 'TRUE' for '--param1 <param1>' [possible values: true, false]")),
                (vec!["--param1", "no"], Err("invalid value 'no' for '--param1 <param1>' [possible values: true, false]")),
                (vec!["--param1", "1"], Err("invalid value '1' for '--param1 <param1>' [possible values: true, false]")),
                (vec!["--param1", "0"], Err("invalid value '0' for '--param1 <param1>' [possible values: true, false]")),
                (vec!["--param1", "on"], Err("invalid value 'on' for '--param1 <param1>' [possible values: true, false]")),
                (vec!["--param1", "off"], Err("invalid value 'off' for '--param1 <param1>' [possible values: true, false]")),
            ];

            check_type_expectations("param1", "bool", &syntax, &expectations);
        }

        #[test]
        fn test_value_enum() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    arg_type: SyntaxOptArgType::Enum(vec![
                        "a".to_string(),
                        "b".to_string(),
                        "c".to_string(),
                    ]),
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(Vec<&str>, Result<&str, &str>)> = vec![
                (vec!["--param1", "a"], Ok("a")),
                (vec!["--param1", "b"], Ok("b")),
                (vec!["--param1", "c"], Ok("c")),
                (vec!["--param1", "d"], Err("invalid value 'd' for '--param1 <param1>' [possible values: a, b, c]")),
                (vec!["--param1", ""], Err("a value is required for '--param1 <param1>' but none was supplied [possible values: a, b, c]")),
            ];

            check_type_expectations("param1", "str", &syntax, &expectations);
        }

        #[test]
        fn test_value_flag() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    arg_type: SyntaxOptArgType::Flag,
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(Vec<&str>, Result<&str, &str>)> = vec![
                (vec!["--param1"], Ok("true")),
                (vec![], Ok("false")),
                (vec!["--param1", "c"], Err("unexpected argument 'c' found")),
                (vec!["--param1", ""], Err("unexpected argument '' found")),
            ];

            check_type_expectations("param1", "bool", &syntax, &expectations);
        }

        #[test]
        fn test_value_counter() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--count".to_string(), "-c".to_string()],
                    arg_type: SyntaxOptArgType::Counter,
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(Vec<&str>, Result<&str, &str>)> = vec![
                (vec![], Ok("0")),
                (vec!["--count"], Ok("1")),
                (vec!["--count", "--count"], Ok("2")),
                (vec!["-c", "-c", "-c"], Ok("3")),
                (vec!["-cc", "-c"], Ok("3")),
                (vec!["-ccc"], Ok("3")),
                (
                    vec!["--count", "blah"],
                    Err("unexpected argument 'blah' found"),
                ),
                (vec!["--count", ""], Err("unexpected argument '' found")),
            ];

            check_type_expectations("count", "int", &syntax, &expectations);
        }

        #[test]
        fn test_unexpected_argument() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        required: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        desc: Some("takes an int".to_string()),
                        required: false,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            match syntax.parse_args(
                ["unexpected", "--param1", "value1", "--param2", "42"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                vec!["test".to_string()],
            ) {
                Ok(_) => panic!("should have failed"),
                Err(e) => assert!(e.to_string().contains("unexpected argument 'unexpected'")),
            }
        }

        #[test]
        fn test_param_default() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--str".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        default: Some("default1".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--int".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        desc: Some("takes an int".to_string()),
                        default: Some("42".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--float".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        desc: Some("takes a float".to_string()),
                        default: Some("3.14".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--bool".to_string()],
                        arg_type: SyntaxOptArgType::Boolean,
                        desc: Some("takes a boolean".to_string()),
                        default: Some("true".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--enum".to_string()],
                        arg_type: SyntaxOptArgType::Enum(vec!["a".to_string(), "b".to_string()]),
                        desc: Some("takes an enum".to_string()),
                        default: Some("a".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--flag".to_string()],
                        arg_type: SyntaxOptArgType::Flag,
                        desc: Some("takes a flag (default to false)".to_string()),
                        default: Some("false".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--no-flag".to_string()],
                        arg_type: SyntaxOptArgType::Flag,
                        desc: Some("takes a flag (default to true)".to_string()),
                        default: Some("true".to_string()),
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let args = match syntax.parse_args(vec![], vec!["test".to_string()]) {
                Ok(args) => args,
                Err(e) => panic!("{}", e),
            };

            let expectations = vec![
                ("OMNI_ARG_LIST", "str int float bool enum flag no_flag"),
                ("OMNI_ARG_STR_TYPE", "str"),
                ("OMNI_ARG_STR_VALUE", "default1"),
                ("OMNI_ARG_INT_TYPE", "int"),
                ("OMNI_ARG_INT_VALUE", "42"),
                ("OMNI_ARG_FLOAT_TYPE", "float"),
                ("OMNI_ARG_FLOAT_VALUE", "3.14"),
                ("OMNI_ARG_BOOL_TYPE", "bool"),
                ("OMNI_ARG_BOOL_VALUE", "true"),
                ("OMNI_ARG_ENUM_TYPE", "str"),
                ("OMNI_ARG_ENUM_VALUE", "a"),
                ("OMNI_ARG_FLAG_TYPE", "bool"),
                ("OMNI_ARG_FLAG_VALUE", "false"),
                ("OMNI_ARG_NO_FLAG_TYPE", "bool"),
                ("OMNI_ARG_NO_FLAG_VALUE", "true"),
            ];

            assert_eq!(args.len(), expectations.len());
            for (key, value) in expectations {
                assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
            }
        }

        #[test]
        fn test_param_default_array_with_value_delimiter() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--arr-str".to_string()],
                        arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                        default: Some("default1,default2".to_string()),
                        value_delimiter: Some(','),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--arr-int".to_string()],
                        arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Integer)),
                        default: Some("42|43|44".to_string()),
                        value_delimiter: Some('|'),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--arr-float".to_string()],
                        arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Float)),
                        default: Some("3.14/2.71".to_string()),
                        value_delimiter: Some('/'),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--arr-bool".to_string()],
                        arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Boolean)),
                        default: Some("true%false".to_string()),
                        value_delimiter: Some('%'),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--arr-enum".to_string()],
                        arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::Enum(vec![
                            "a".to_string(),
                            "b".to_string(),
                        ]))),
                        default: Some("a,b,a,a".to_string()),
                        value_delimiter: Some(','),
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let args = match syntax.parse_args(vec![], vec!["test".to_string()]) {
                Ok(args) => args,
                Err(e) => panic!("{}", e),
            };

            let expectations = vec![
                (
                    "OMNI_ARG_LIST",
                    "arr_str arr_int arr_float arr_bool arr_enum",
                ),
                ("OMNI_ARG_ARR_STR_TYPE", "str/2"),
                ("OMNI_ARG_ARR_STR_VALUE_0", "default1"),
                ("OMNI_ARG_ARR_STR_VALUE_1", "default2"),
                ("OMNI_ARG_ARR_INT_TYPE", "int/3"),
                ("OMNI_ARG_ARR_INT_VALUE_0", "42"),
                ("OMNI_ARG_ARR_INT_VALUE_1", "43"),
                ("OMNI_ARG_ARR_INT_VALUE_2", "44"),
                ("OMNI_ARG_ARR_FLOAT_TYPE", "float/2"),
                ("OMNI_ARG_ARR_FLOAT_VALUE_0", "3.14"),
                ("OMNI_ARG_ARR_FLOAT_VALUE_1", "2.71"),
                ("OMNI_ARG_ARR_BOOL_TYPE", "bool/2"),
                ("OMNI_ARG_ARR_BOOL_VALUE_0", "true"),
                ("OMNI_ARG_ARR_BOOL_VALUE_1", "false"),
                ("OMNI_ARG_ARR_ENUM_TYPE", "str/4"),
                ("OMNI_ARG_ARR_ENUM_VALUE_0", "a"),
                ("OMNI_ARG_ARR_ENUM_VALUE_1", "b"),
                ("OMNI_ARG_ARR_ENUM_VALUE_2", "a"),
                ("OMNI_ARG_ARR_ENUM_VALUE_3", "a"),
            ];

            let expect_len = expectations.len();
            for (key, value) in expectations {
                assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
            }
            assert_eq!(args.len(), expect_len);
        }

        #[test]
        fn test_param_default_missing_value() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--str".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        num_values: Some(SyntaxOptArgNumValues::AtMost(1)),
                        default_missing_value: Some("default1".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--int".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        desc: Some("takes an int".to_string()),
                        num_values: Some(SyntaxOptArgNumValues::AtMost(1)),
                        default_missing_value: Some("42".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--float".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        desc: Some("takes a float".to_string()),
                        num_values: Some(SyntaxOptArgNumValues::AtMost(1)),
                        default_missing_value: Some("3.14".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--bool".to_string()],
                        arg_type: SyntaxOptArgType::Boolean,
                        desc: Some("takes a boolean".to_string()),
                        num_values: Some(SyntaxOptArgNumValues::AtMost(1)),
                        default_missing_value: Some("true".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--enum".to_string()],
                        arg_type: SyntaxOptArgType::Enum(vec!["a".to_string(), "b".to_string()]),
                        desc: Some("takes an enum".to_string()),
                        num_values: Some(SyntaxOptArgNumValues::AtMost(1)),
                        default_missing_value: Some("a".to_string()),
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let argv = ["--str", "--int", "--float", "--bool", "--enum"];

            let args = match syntax.parse_args(
                argv.iter().map(|s| s.to_string()).collect(),
                vec!["test".to_string()],
            ) {
                Ok(args) => args,
                Err(e) => panic!("{}", e),
            };

            let expectations = vec![
                ("OMNI_ARG_LIST", "str int float bool enum"),
                ("OMNI_ARG_STR_TYPE", "str"),
                ("OMNI_ARG_STR_VALUE", "default1"),
                ("OMNI_ARG_INT_TYPE", "int"),
                ("OMNI_ARG_INT_VALUE", "42"),
                ("OMNI_ARG_FLOAT_TYPE", "float"),
                ("OMNI_ARG_FLOAT_VALUE", "3.14"),
                ("OMNI_ARG_BOOL_TYPE", "bool"),
                ("OMNI_ARG_BOOL_VALUE", "true"),
                ("OMNI_ARG_ENUM_TYPE", "str"),
                ("OMNI_ARG_ENUM_VALUE", "a"),
            ];

            let expectations_len = expectations.len();
            for (key, value) in expectations {
                assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
            }
            assert_eq!(args.len(), expectations_len);
        }

        #[test]
        fn test_param_value_delimiter_on_non_array() {
            let syntax = CommandSyntax {
                parameters: vec![SyntaxOptArg {
                    names: vec!["--param1".to_string()],
                    arg_type: SyntaxOptArgType::String,
                    value_delimiter: Some(','),
                    ..SyntaxOptArg::default()
                }],
                ..CommandSyntax::default()
            };

            let args = match syntax.parse_args(
                ["--param1", "value1,value2"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                vec!["test".to_string()],
            ) {
                Ok(args) => args,
                Err(e) => panic!("{}", e),
            };

            let expectations = vec![
                ("OMNI_ARG_LIST", "param1"),
                ("OMNI_ARG_PARAM1_TYPE", "str/2"),
                ("OMNI_ARG_PARAM1_VALUE_0", "value1"),
                ("OMNI_ARG_PARAM1_VALUE_1", "value2"),
            ];

            assert_eq!(args.len(), expectations.len());
            for (key, value) in expectations {
                assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
            }
        }

        #[test]
        fn test_param_num_values() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        num_values: Some(SyntaxOptArgNumValues::Exactly(2)),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        desc: Some("takes an int".to_string()),
                        num_values: Some(SyntaxOptArgNumValues::Exactly(3)),
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let args = match syntax.parse_args(
                ["--param1", "value1", "value2", "--param2", "42", "43", "44"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                vec!["test".to_string()],
            ) {
                Ok(args) => args,
                Err(e) => panic!("{}", e),
            };

            let expectations = vec![
                ("OMNI_ARG_LIST", "param1 param2"),
                ("OMNI_ARG_PARAM1_TYPE", "str/2"),
                ("OMNI_ARG_PARAM1_VALUE_0", "value1"),
                ("OMNI_ARG_PARAM1_VALUE_1", "value2"),
                ("OMNI_ARG_PARAM2_TYPE", "int/3"),
                ("OMNI_ARG_PARAM2_VALUE_0", "42"),
                ("OMNI_ARG_PARAM2_VALUE_1", "43"),
                ("OMNI_ARG_PARAM2_VALUE_2", "44"),
            ];

            assert_eq!(args.len(), expectations.len());
            for (key, value) in expectations {
                assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
            }
        }

        #[test]
        fn test_param_num_values_at_most() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--exactly".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        num_values: Some(SyntaxOptArgNumValues::Exactly(2)),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--at-most-3".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        num_values: Some(SyntaxOptArgNumValues::AtMost(3)),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--at-least-2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        num_values: Some(SyntaxOptArgNumValues::AtLeast(2)),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--between-2-4".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        num_values: Some(SyntaxOptArgNumValues::Between(2, 4)),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--any".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        num_values: Some(SyntaxOptArgNumValues::Any),
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let test_cases: Vec<(Vec<&str>, Option<&str>)> = vec![
                (vec!["--exactly"], Some("a value is required for '--exactly <exactly> <exactly>' but none was supplied")),
                (vec!["--exactly", "1"], Some("2 values required for '--exactly <exactly> <exactly>' but 1 was provided")),
                (vec!["--exactly", "1", "2"], None),
                (vec!["--exactly", "1", "2", "3"], Some("unexpected argument '3' found")),
                (vec!["--at-most-3"], None),
                (vec!["--at-most-3", "1"], None),
                (vec!["--at-most-3", "1", "2"], None),
                (vec!["--at-most-3", "1", "2", "3"], None),
                (vec!["--at-most-3", "1", "2", "3", "4"], Some("unexpected argument '4' found")),
                (vec!["--at-least-2"], Some("a value is required for '--at-least-2 <at_least_2> <at_least_2>...' but none was supplied")),
                (vec!["--at-least-2", "1"], Some("2 values required by '--at-least-2 <at_least_2> <at_least_2>...'; only 1 was provided")),
                (vec!["--at-least-2", "1", "2"], None),
                (vec!["--at-least-2", "1", "2", "3"], None),
                (vec!["--at-least-2", "1", "2", "3", "4"], None),
                (vec!["--between-2-4"], Some("a value is required for '--between-2-4 <between_2_4> <between_2_4>...' but none was supplied")),
                (vec!["--between-2-4", "1"], Some("2 values required by '--between-2-4 <between_2_4> <between_2_4>...'; only 1 was provided")),
                (vec!["--between-2-4", "1", "2"], None),
                (vec!["--between-2-4", "1", "2", "3"], None),
                (vec!["--between-2-4", "1", "2", "3", "4"], None),
                (vec!["--between-2-4", "1", "2", "3", "4", "5"], Some("unexpected argument '5' found")),
                (vec!["--any"], None),
                (vec!["--any", "1"], None),
                (vec!["--any", "1", "2", "3", "4", "5", "6", "7", "8", "9"], None),
            ];

            for (i, (argv, error)) in test_cases.iter().enumerate() {
                match syntax.parse_args(
                    argv.iter().map(|s| s.to_string()).collect(),
                    vec!["test".to_string()],
                ) {
                    Ok(args) => {
                        if error.is_some() {
                            panic!(
                                "case {i} with argv {argv:?} should have failed, instead: {args:?}"
                            );
                        }

                        let mut expectations = vec![(
                            "OMNI_ARG_LIST".to_string(),
                            "exactly at_most_3 at_least_2 between_2_4 any".to_string(),
                        )];

                        let params = &[
                            ("--exactly", "exactly"),
                            ("--at-most-3", "at_most_3"),
                            ("--at-least-2", "at_least_2"),
                            ("--between-2-4", "between_2_4"),
                            ("--any", "any"),
                        ];

                        for (param, env_name) in params {
                            // Get the position of the parameter in argv
                            let pos = argv.iter().position(|s| s == param);
                            let values = match pos {
                                Some(pos) => {
                                    // Take all values until the next value with --
                                    argv.iter()
                                        .skip(pos + 1)
                                        .take_while(|s| !s.starts_with("--"))
                                        .collect::<Vec<_>>()
                                }
                                None => vec![],
                            };

                            // Add the type and values to the expectations
                            let type_var = format!("OMNI_ARG_{}_TYPE", env_name.to_uppercase());
                            expectations.push((type_var, format!("int/{}", values.len())));

                            for (i, value) in values.iter().enumerate() {
                                let value_var =
                                    format!("OMNI_ARG_{}_VALUE_{}", env_name.to_uppercase(), i);
                                expectations.push((value_var, value.to_string()));
                            }
                        }

                        // Validate that the expectations are met
                        let expect_len = expectations.len();
                        for (key, value) in expectations {
                            assert_eq!(
                                (&argv, &key, args.get(&key)),
                                (&argv, &key, Some(&value.to_string()))
                            );
                        }
                        assert_eq!((&argv, args.len()), (&argv, expect_len));
                    }
                    Err(e) => {
                        if let Some(errmsg) = error {
                            assert_eq!(e.simple(), errmsg.to_string());
                            continue;
                        }
                        panic!("case {i} with argv {argv:?} should have succeeded, instead: {e}");
                    }
                }
            }
        }

        #[test]
        fn test_param_group_occurrences() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--group".to_string()],
                        num_values: Some(SyntaxOptArgNumValues::AtLeast(1)),
                        arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                        group_occurrences: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--no-group".to_string()],
                        num_values: Some(SyntaxOptArgNumValues::AtLeast(1)),
                        arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let argv = vec![
                "--group",
                "group1.1",
                "group1.2",
                "--no-group",
                "no-group1.1",
                "no-group1.2",
                "--group",
                "group2.1",
                "--no-group",
                "no-group2.1",
                "--group",
                "group3.1",
                "group3.2",
                "group3.3",
                "--no-group",
                "no-group3.1",
                "no-group3.2",
                "no-group3.3",
            ];

            let args = match syntax.parse_args(
                argv.iter().map(|s| s.to_string()).collect(),
                vec!["test".to_string()],
            ) {
                Ok(args) => args,
                Err(e) => panic!("{}", e),
            };

            let expectations = vec![
                ("OMNI_ARG_LIST", "group no_group"),
                ("OMNI_ARG_GROUP_TYPE", "str/3/3"),
                ("OMNI_ARG_GROUP_TYPE_0", "str/2"),
                ("OMNI_ARG_GROUP_VALUE_0_0", "group1.1"),
                ("OMNI_ARG_GROUP_VALUE_0_1", "group1.2"),
                ("OMNI_ARG_GROUP_TYPE_1", "str/1"),
                ("OMNI_ARG_GROUP_VALUE_1_0", "group2.1"),
                ("OMNI_ARG_GROUP_TYPE_2", "str/3"),
                ("OMNI_ARG_GROUP_VALUE_2_0", "group3.1"),
                ("OMNI_ARG_GROUP_VALUE_2_1", "group3.2"),
                ("OMNI_ARG_GROUP_VALUE_2_2", "group3.3"),
                ("OMNI_ARG_NO_GROUP_TYPE", "str/6"),
                ("OMNI_ARG_NO_GROUP_VALUE_0", "no-group1.1"),
                ("OMNI_ARG_NO_GROUP_VALUE_1", "no-group1.2"),
                ("OMNI_ARG_NO_GROUP_VALUE_2", "no-group2.1"),
                ("OMNI_ARG_NO_GROUP_VALUE_3", "no-group3.1"),
                ("OMNI_ARG_NO_GROUP_VALUE_4", "no-group3.2"),
                ("OMNI_ARG_NO_GROUP_VALUE_5", "no-group3.3"),
            ];

            eprintln!("{args:?}");

            let expectations_len = expectations.len();
            for (key, value) in expectations {
                assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
            }
            assert_eq!(args.len(), expectations_len);
        }

        #[test]
        fn test_param_last() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["param2".to_string()],
                        arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                        last_arg_double_hyphen: true,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let args = match syntax.parse_args(
                ["value1", "--", "value2", "value3"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                vec!["test".to_string()],
            ) {
                Ok(args) => args,
                Err(e) => panic!("{}", e),
            };

            let expectations = vec![
                ("OMNI_ARG_LIST", "param1 param2"),
                ("OMNI_ARG_PARAM1_TYPE", "str"),
                ("OMNI_ARG_PARAM1_VALUE", "value1"),
                ("OMNI_ARG_PARAM2_TYPE", "str/2"),
                ("OMNI_ARG_PARAM2_VALUE_0", "value2"),
                ("OMNI_ARG_PARAM2_VALUE_1", "value3"),
            ];

            assert_eq!(args.len(), expectations.len());
            for (key, value) in expectations {
                assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
            }
        }

        #[test]
        fn test_param_last_single_value() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["param2".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        last_arg_double_hyphen: true,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let args = syntax.parse_args(
                ["value1", "--", "value2", "value3"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                vec!["test".to_string()],
            );

            match args {
                Ok(_) => panic!("should have failed"),
                Err(e) => assert_eq!(
                    e.simple(),
                    "the argument '[param2]' cannot be used multiple times"
                ),
            }
        }

        #[test]
        fn test_param_leftovers() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["param2".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        leftovers: true,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let args = match syntax.parse_args(
                ["value1", "value2", "value3"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                vec!["test".to_string()],
            ) {
                Ok(args) => args,
                Err(e) => panic!("{}", e),
            };

            let expectations = vec![
                ("OMNI_ARG_LIST", "param1 param2"),
                ("OMNI_ARG_PARAM1_TYPE", "str"),
                ("OMNI_ARG_PARAM1_VALUE", "value1"),
                ("OMNI_ARG_PARAM2_TYPE", "str/2"),
                ("OMNI_ARG_PARAM2_VALUE_0", "value2"),
                ("OMNI_ARG_PARAM2_VALUE_1", "value3"),
            ];

            assert_eq!(args.len(), expectations.len());
            for (key, value) in expectations {
                assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
            }
        }

        #[test]
        fn test_param_leftovers_no_allow_hyphens() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["param2".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        leftovers: true,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let args = syntax.parse_args(
                ["value1", "--value2", "value3"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                vec!["test".to_string()],
            );

            match args {
                Ok(_) => panic!("should have failed"),
                Err(e) => assert_eq!(e.simple(), "unexpected argument '--value2' found"),
            }
        }

        #[test]
        fn test_param_leftovers_allow_hyphens() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["param2".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        leftovers: true,
                        allow_hyphen_values: true,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let args = match syntax.parse_args(
                ["value1", "--value2", "value3"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                vec!["test".to_string()],
            ) {
                Ok(args) => args,
                Err(e) => panic!("{}", e),
            };

            let expectations = vec![
                ("OMNI_ARG_LIST", "param1 param2"),
                ("OMNI_ARG_PARAM1_TYPE", "str"),
                ("OMNI_ARG_PARAM1_VALUE", "value1"),
                ("OMNI_ARG_PARAM2_TYPE", "str/2"),
                ("OMNI_ARG_PARAM2_VALUE_0", "--value2"),
                ("OMNI_ARG_PARAM2_VALUE_1", "value3"),
            ];

            assert_eq!(args.len(), expectations.len());
            for (key, value) in expectations {
                assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
            }
        }

        #[test]
        fn test_param_allow_negative_numbers() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        allow_negative_numbers: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        allow_negative_numbers: true,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let args = match syntax.parse_args(
                ["--param1", "-42", "--param2", "-3.14"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                vec!["test".to_string()],
            ) {
                Ok(args) => args,
                Err(e) => panic!("{}", e),
            };

            let expectations = vec![
                ("OMNI_ARG_LIST", "param1 param2"),
                ("OMNI_ARG_PARAM1_TYPE", "int"),
                ("OMNI_ARG_PARAM1_VALUE", "-42"),
                ("OMNI_ARG_PARAM2_TYPE", "float"),
                ("OMNI_ARG_PARAM2_VALUE", "-3.14"),
            ];

            let expectations_len = expectations.len();
            for (key, value) in expectations {
                assert_eq!((key, args.get(key)), (key, Some(&value.to_string())));
            }
            assert_eq!(args.len(), expectations_len);
        }

        #[test]
        fn test_param_allow_negative_numbers_scenarios() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        allow_negative_numbers: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        allow_negative_numbers: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param3".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        allow_hyphen_values: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param4".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        allow_hyphen_values: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param5".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        allow_negative_numbers: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param6".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        allow_hyphen_values: true,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(&[&str], Option<&str>)> = vec![
                (&["--param1", "42"], None),
                (&["--param2", "3.14"], None),
                (&["--param3", "42"], None),
                (&["--param4", "3.14"], None),
                (&["--param5", "42"], None),
                (&["--param6", "3.14"], None),
                (&["--param1", "-42"], None),
                (&["--param2", "-3.14"], None),
                (&["--param3", "-42"], None),
                (&["--param4", "-3.14"], None),
                (&["--param5", "-42"], None),
                (&["--param6", "-3.14"], None),
                (
                    &["--param1", "-blah"],
                    Some("unexpected argument '-b' found"),
                ),
                (
                    &["--param2", "-blah"],
                    Some("unexpected argument '-b' found"),
                ),
                (
                    &["--param3", "-blah"],
                    Some("invalid value '-blah' for '--param3 <param3>': invalid digit found in string"),
                ),
                (
                    &["--param4", "-blah"],
                    Some("invalid value '-blah' for '--param4 <param4>': invalid float literal"),
                ),
                (
                    &["--param5", "-blah"],
                    Some("unexpected argument '-b' found"),
                ),
                (&["--param6", "-blah"], None),
            ];

            check_expectations(&syntax, &expectations);
        }

        #[test]
        fn test_param_required() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        required: true,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        desc: Some("takes an int".to_string()),
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(&[&str], Option<&str>)> = vec![
                (
                    &[],
                    Some("the following required arguments were not provided: --param1 <param1>"),
                ),
                (
                    &["--param2", "42"],
                    Some("the following required arguments were not provided: --param1 <param1>"),
                ),
                (&["--param1", "value1"], None),
            ];

            check_expectations(&syntax, &expectations);
        }

        #[test]
        fn test_group_multiple() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        desc: Some("takes an int".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param3".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        desc: Some("takes a float".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param4".to_string()],
                        arg_type: SyntaxOptArgType::Boolean,
                        desc: Some("takes a boolean".to_string()),
                        ..SyntaxOptArg::default()
                    },
                ],
                groups: vec![
                    SyntaxGroup {
                        name: "group1".to_string(),
                        parameters: vec!["--param1".to_string(), "--param2".to_string()],
                        ..SyntaxGroup::default()
                    },
                    SyntaxGroup {
                        name: "group2".to_string(),
                        parameters: vec!["--param3".to_string(), "--param4".to_string()],
                        multiple: true,
                        ..SyntaxGroup::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(&[&str], Option<&str>)> = vec![
                (&[], None),
                (&["--param1", "value1"], None),
                (&["--param2", "42"], None),
                (&["--param3", "3.14"], None),
                (&["--param4", "true"], None),
                (&["--param1", "value1", "--param3", "3.14"], None),
                (
                    &["--param1", "value1", "--param2", "42"],
                    Some(
                        "the argument '--param1 <param1>' cannot be used with '--param2 <param2>'",
                    ),
                ),
                (&["--param3", "3.14", "--param4", "true"], None),
            ];

            check_expectations(&syntax, &expectations);
        }

        #[test]
        fn test_group_required() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        desc: Some("takes an int".to_string()),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param3".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        desc: Some("takes a float".to_string()),
                        ..SyntaxOptArg::default()
                    },
                ],
                groups: vec![SyntaxGroup {
                    name: "group1".to_string(),
                    parameters: vec!["--param1".to_string(), "--param2".to_string()],
                    required: true,
                    ..SyntaxGroup::default()
                }],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(&[&str], Option<&str>)> = vec![
                (&[], Some("the following required arguments were not provided: <--param1 <param1>|--param2 <param2>>")),
                (&["--param3", "3.14"], Some("the following required arguments were not provided: <--param1 <param1>|--param2 <param2>>")),
                (&["--param1", "value1", "--param3", "3.14"], None),
                (&["--param2", "42", "--param3", "3.14"], None),
                (&["--param1", "value1"], None),
                (&["--param2", "42"], None),
            ];

            check_expectations(&syntax, &expectations);
        }

        #[test]
        fn test_group_requires() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param3".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        ..SyntaxOptArg::default()
                    },
                ],
                groups: vec![
                    SyntaxGroup {
                        name: "group1".to_string(),
                        parameters: vec!["--param1".to_string()],
                        requires: vec!["param2".to_string()],
                        ..SyntaxGroup::default()
                    },
                    SyntaxGroup {
                        name: "group3".to_string(),
                        parameters: vec!["--param3".to_string()],
                        requires: vec!["group1".to_string()],
                        ..SyntaxGroup::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(&[&str], Option<&str>)> = vec![
                (&["--param2", "42"], None),
                (
                    &["--param1", "value1"],
                    Some("the following required arguments were not provided: --param2 <param2>"),
                ),
                (&["--param1", "value1", "--param2", "42"], None),
                (
                    &["--param3", "3.14"],
                    Some("the following required arguments were not provided: <--param1 <param1>>"),
                ),
                (
                    &["--param3", "3.14", "--param2", "42"],
                    Some("the following required arguments were not provided: <--param1 <param1>>"),
                ),
                (
                    &["--param1", "value1", "--param2", "42", "--param3", "3.14"],
                    None,
                ),
                (&[], None),
            ];

            check_expectations(&syntax, &expectations);
        }

        #[test]
        fn test_group_conflicts_with() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param3".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        ..SyntaxOptArg::default()
                    },
                ],
                groups: vec![
                    SyntaxGroup {
                        name: "group1".to_string(),
                        parameters: vec!["--param1".to_string()],
                        conflicts_with: vec!["group2".to_string()],
                        ..SyntaxGroup::default()
                    },
                    SyntaxGroup {
                        name: "group2".to_string(),
                        parameters: vec!["--param2".to_string()],
                        ..SyntaxGroup::default()
                    },
                    SyntaxGroup {
                        name: "group3".to_string(),
                        parameters: vec!["--param3".to_string()],
                        conflicts_with: vec!["param1".to_string()],
                        ..SyntaxGroup::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(&[&str], Option<&str>)> = vec![
                (&[], None),
                (&["--param1", "value1"], None),
                (&["--param2", "42"], None),
                (&["--param3", "3.14"], None),
                (&["--param2", "42", "--param3", "3.14"], None),
                (
                    &["--param1", "value1", "--param2", "42"],
                    Some(
                        "the argument '--param1 <param1>' cannot be used with '--param2 <param2>'",
                    ),
                ),
                (
                    &["--param1", "value1", "--param3", "3.14"],
                    Some(
                        "the argument '--param1 <param1>' cannot be used with '--param3 <param3>'",
                    ),
                ),
            ];

            check_expectations(&syntax, &expectations);
        }

        #[test]
        fn test_param_requires() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        requires: vec!["param2".to_string()],
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param3".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        requires: vec!["group2".to_string()],
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param4".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        requires: vec!["param1".to_string()],
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param5".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        requires: vec!["group1".to_string()],
                        ..SyntaxOptArg::default()
                    },
                ],
                groups: vec![
                    SyntaxGroup {
                        name: "group1".to_string(),
                        parameters: vec!["--param1".to_string()],
                        ..SyntaxGroup::default()
                    },
                    SyntaxGroup {
                        name: "group2".to_string(),
                        parameters: vec!["--param2".to_string()],
                        ..SyntaxGroup::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(&[&str], Option<&str>)> = vec![
                (&["--param2", "42"], None),
                (
                    &["--param1", "value1"],
                    Some("the following required arguments were not provided: --param2 <param2>"),
                ),
                (&["--param1", "value1", "--param2", "42"], None),
                (&["--param3", "3.14"], Some("the following required arguments were not provided: <--param2 <param2>>")),
                (&["--param3", "3.14", "--param2", "42"], None),
                (&["--param4", "10"], Some("the following required arguments were not provided: --param2 <param2> --param1 <param1>")),
                (&["--param4", "10", "--param1", "value1"], Some("the following required arguments were not provided: --param2 <param2>")),
                (&["--param4", "10", "--param1", "value1", "--param2", "42"], None),
                (&["--param5", "20"], Some("the following required arguments were not provided: <--param1 <param1>>")),
                (&["--param5", "20", "--param1", "value1"], Some("the following required arguments were not provided: --param2 <param2>")),
                (&["--param5", "20", "--param2", "42"], Some("the following required arguments were not provided: <--param1 <param1>>")),
                (&["--param5", "20", "--param1", "value1", "--param2", "42"], None),
                (&[], None),
            ];

            check_expectations(&syntax, &expectations);
        }

        #[test]
        fn test_param_conflicts_with() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        conflicts_with: vec!["param2".to_string()],
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param3".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        conflicts_with: vec!["group2".to_string()],
                        ..SyntaxOptArg::default()
                    },
                ],
                groups: vec![
                    SyntaxGroup {
                        name: "group1".to_string(),
                        parameters: vec!["--param1".to_string()],
                        ..SyntaxGroup::default()
                    },
                    SyntaxGroup {
                        name: "group2".to_string(),
                        parameters: vec!["--param2".to_string()],
                        ..SyntaxGroup::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(&[&str], Option<&str>)> = vec![
                (&[], None),
                (&["--param1", "value1"], None),
                (&["--param2", "42"], None),
                (&["--param3", "3.14"], None),
                (&["--param1", "value1", "--param3", "3.14"], None),
                (
                    &["--param1", "value1", "--param2", "42"],
                    Some(
                        "the argument '--param1 <param1>' cannot be used with '--param2 <param2>'",
                    ),
                ),
                (
                    &["--param2", "42", "--param3", "3.14"],
                    Some(
                        "the argument '--param3 <param3>' cannot be used with '--param2 <param2>'",
                    ),
                ),
                (
                    &["--param1", "value1", "--param2", "42", "--param3", "3.14"],
                    Some(
                        "the argument '--param1 <param1>' cannot be used with '--param2 <param2>'",
                    ),
                ),
            ];

            check_expectations(&syntax, &expectations);
        }

        #[test]
        fn test_param_required_without() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        required_without: vec!["param2".to_string(), "param3".to_string()],
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        required: false,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param3".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        required: false,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param4".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        required_without: vec!["group1".to_string()],
                        ..SyntaxOptArg::default()
                    },
                ],
                groups: vec![SyntaxGroup {
                    name: "group1".to_string(),
                    parameters: vec!["--param1".to_string()],
                    ..SyntaxGroup::default()
                }],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(&[&str], Option<&str>)> = vec![
                (&["--param1", "value1"], None),
                (&["--param2", "42"], Some("the following required arguments were not provided: --param4 <param4>")),
                (&["--param3", "3.14"], Some("the following required arguments were not provided: --param4 <param4>")),
                (&["--param2", "42", "--param3", "43"], Some("the following required arguments were not provided: --param4 <param4>")),
                (&["--param1", "value1", "--param2", "42"], None),
                (&["--param1", "value1", "--param2", "42", "--param3", "3.14", "--param4", "10"], None),
                (&[], Some("the following required arguments were not provided: --param1 <param1> --param4 <param4>")),
            ];

            check_expectations(&syntax, &expectations);
        }

        #[test]
        fn test_param_required_without_all() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        required_without_all: vec!["param2".to_string(), "param3".to_string()],
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        required: false,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param3".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        required: false,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param4".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        required_without_all: vec!["group5".to_string(), "group2".to_string()],
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param5".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        ..SyntaxOptArg::default()
                    },
                ],
                groups: vec![
                    SyntaxGroup {
                        name: "group5".to_string(),
                        parameters: vec!["--param5".to_string()],
                        ..SyntaxGroup::default()
                    },
                    SyntaxGroup {
                        name: "group2".to_string(),
                        parameters: vec!["--param2".to_string()],
                        ..SyntaxGroup::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(&[&str], Option<&str>)> = vec![
                (&["--param1", "value1"], Some("the following required arguments were not provided: --param4 <param4>")),
                (&["--param2", "42"], Some("the following required arguments were not provided: --param1 <param1> --param4 <param4>")),
                (&["--param3", "3.14"], Some("the following required arguments were not provided: --param1 <param1> --param4 <param4>")),
                (&["--param2", "42", "--param3", "43"], Some("the following required arguments were not provided: --param4 <param4>")),
                (&["--param1", "value1", "--param2", "42"], Some("the following required arguments were not provided: --param4 <param4>")),
                (&["--param1", "value1", "--param4", "10"], None),
                (&["--param1", "value1", "--param2", "42", "--param3", "3.14", "--param4", "10", "--param5", "20"], None),
                (&["--param2", "42", "--param3", "3.14", "--param5", "20"], None),
                (&[], Some("the following required arguments were not provided: --param1 <param1> --param4 <param4>")),
            ];

            check_expectations(&syntax, &expectations);
        }

        #[test]
        fn test_param_required_if_eq() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        required_if_eq: HashMap::from_iter(vec![(
                            "param2".to_string(),
                            "42".to_string(),
                        )]),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param3".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        required_if_eq: HashMap::from_iter(vec![(
                            "param4".to_string(),
                            "true".to_string(),
                        )]),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param4".to_string()],
                        arg_type: SyntaxOptArgType::Boolean,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(&[&str], Option<&str>)> = vec![
                (&["--param1", "value1"], None),
                (
                    &["--param2", "42"],
                    Some("the following required arguments were not provided: --param1 <param1>"),
                ),
                (&["--param1", "value1", "--param2", "42"], None),
                (&["--param3", "3.14"], None),
                (
                    &["--param4", "true"],
                    Some("the following required arguments were not provided: --param3 <param3>"),
                ),
                (&["--param3", "3.14", "--param4", "true"], None),
                (
                    &[
                        "--param1", "value1", "--param2", "42", "--param3", "3.14", "--param4",
                        "true",
                    ],
                    None,
                ),
                (&[], None),
            ];

            check_expectations(&syntax, &expectations);
        }

        #[test]
        fn test_param_required_if_eq_all() {
            let syntax = CommandSyntax {
                parameters: vec![
                    SyntaxOptArg {
                        names: vec!["--param1".to_string()],
                        arg_type: SyntaxOptArgType::String,
                        required_if_eq_all: HashMap::from_iter(vec![
                            ("param2".to_string(), "42".to_string()),
                            ("param3".to_string(), "3.14".to_string()),
                        ]),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param2".to_string()],
                        arg_type: SyntaxOptArgType::Integer,
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param3".to_string()],
                        arg_type: SyntaxOptArgType::Float,
                        required_if_eq_all: HashMap::from_iter(vec![(
                            "param4".to_string(),
                            "true".to_string(),
                        )]),
                        ..SyntaxOptArg::default()
                    },
                    SyntaxOptArg {
                        names: vec!["--param4".to_string()],
                        arg_type: SyntaxOptArgType::Boolean,
                        ..SyntaxOptArg::default()
                    },
                ],
                ..CommandSyntax::default()
            };

            let expectations: Vec<(&[&str], Option<&str>)> = vec![
                (&["--param1", "value1"], None),
                (&["--param2", "42"], None),
                (&["--param3", "3.14"], None),
                (
                    &["--param2", "42", "--param3", "3.14"],
                    Some("the following required arguments were not provided: --param1 <param1>"),
                ),
                (&["--param1", "value1", "--param2", "42"], None),
                (&["--param1", "value1", "--param3", "3.14"], None),
                (
                    &["--param1", "value1", "--param4", "true"],
                    Some("the following required arguments were not provided: --param3 <param3>"),
                ),
                (&["--param3", "3.14"], None),
                (
                    &["--param4", "true"],
                    Some("the following required arguments were not provided: --param3 <param3>"),
                ),
                (&["--param3", "3.14", "--param4", "true"], None),
                (
                    &[
                        "--param1", "value1", "--param2", "42", "--param3", "3.14", "--param4",
                        "true",
                    ],
                    None,
                ),
                (&[], None),
            ];

            check_expectations(&syntax, &expectations);
        }
    }
}

mod parse_arg_name {
    use super::*;

    #[test]
    fn test_simple_positional() {
        let (names, arg_type, placeholders, leftovers) = parse_arg_name("arg");
        assert_eq!(names, vec!["arg"]);
        assert_eq!(arg_type, SyntaxOptArgType::String);
        assert!(placeholders.is_empty());
        assert!(!leftovers);
    }

    #[test]
    fn test_short_option() {
        let (names, arg_type, placeholders, leftovers) = parse_arg_name("-a");
        assert_eq!(names, vec!["-a"]);
        assert_eq!(arg_type, SyntaxOptArgType::String);
        assert!(placeholders.is_empty());
        assert!(!leftovers);
    }

    #[test]
    fn test_long_option() {
        let (names, arg_type, placeholders, leftovers) = parse_arg_name("--option");
        assert_eq!(names, vec!["--option"]);
        assert_eq!(arg_type, SyntaxOptArgType::String);
        assert!(placeholders.is_empty());
        assert!(!leftovers);
    }

    #[test]
    fn test_multiple_names() {
        let (names, arg_type, placeholders, leftovers) = parse_arg_name("-a, --alpha");
        assert_eq!(names, vec!["-a", "--alpha"]);
        assert_eq!(arg_type, SyntaxOptArgType::String);
        assert!(placeholders.is_empty());
        assert!(!leftovers);
    }

    #[test]
    fn test_counter_option() {
        let (names, arg_type, placeholders, leftovers) = parse_arg_name("--count...");
        assert_eq!(names, vec!["--count"]);
        assert_eq!(arg_type, SyntaxOptArgType::Counter);
        assert!(placeholders.is_empty());
        assert!(!leftovers);
    }

    #[test]
    fn test_positional_with_placeholder() {
        let (names, arg_type, placeholders, leftovers) = parse_arg_name("arg PLACEHOLDER");
        assert_eq!(names, vec!["arg"]);
        assert_eq!(arg_type, SyntaxOptArgType::String);
        assert_eq!(placeholders, vec!["PLACEHOLDER"]);
        assert!(!leftovers);
    }

    #[test]
    fn test_option_with_placeholder() {
        let (names, arg_type, placeholders, leftovers) = parse_arg_name("--option VALUE");
        assert_eq!(names, vec!["--option"]);
        assert_eq!(arg_type, SyntaxOptArgType::String);
        assert_eq!(placeholders, vec!["VALUE"]);
        assert!(!leftovers);
    }

    #[test]
    fn test_multiple_placeholders() {
        let (names, arg_type, placeholders, leftovers) = parse_arg_name("--option FIRST SECOND");
        assert_eq!(names, vec!["--option"]);
        assert_eq!(arg_type, SyntaxOptArgType::String);
        assert_eq!(placeholders, vec!["FIRST", "SECOND"]);
        assert!(!leftovers);
    }

    #[test]
    fn test_leftovers_positional() {
        let (names, arg_type, placeholders, leftovers) = parse_arg_name("args...");
        assert_eq!(names, vec!["args"]);
        assert_eq!(arg_type, SyntaxOptArgType::String);
        assert!(placeholders.is_empty());
        assert!(leftovers);
    }

    #[test]
    fn test_multiple_names_with_placeholder_at_the_end() {
        let (names, arg_type, placeholders, leftovers) = parse_arg_name("-f, --file FILENAME");
        assert_eq!(names, vec!["-f", "--file"]);
        assert_eq!(arg_type, SyntaxOptArgType::String);
        assert_eq!(placeholders, vec!["FILENAME"]);
        assert!(!leftovers);
    }

    #[test]
    fn test_multiple_names_with_placeholders_for_each() {
        let (names, arg_type, placeholders, leftovers) =
            parse_arg_name("-f FILENAME1, --file FILENAME2");
        assert_eq!(names, vec!["-f", "--file"]);
        assert_eq!(arg_type, SyntaxOptArgType::String);
        assert_eq!(placeholders, vec!["FILENAME1", "FILENAME2"]);
        assert!(!leftovers);
    }

    #[test]
    fn test_equals_separator() {
        let (names, arg_type, placeholders, leftovers) = parse_arg_name("--option=VALUE");
        assert_eq!(names, vec!["--option"]);
        assert_eq!(arg_type, SyntaxOptArgType::String);
        assert_eq!(placeholders, vec!["VALUE"]);
        assert!(!leftovers);
    }

    #[test]
    fn test_empty_input() {
        let (names, arg_type, placeholders, leftovers) = parse_arg_name("");
        assert_eq!(names, vec![""]);
        assert_eq!(arg_type, SyntaxOptArgType::String);
        assert!(placeholders.is_empty());
        assert!(!leftovers);
    }

    #[test]
    fn test_whitespace_handling() {
        let (names, arg_type, placeholders, leftovers) = parse_arg_name("  --option  VALUE  ");
        assert_eq!(names, vec!["--option"]);
        assert_eq!(arg_type, SyntaxOptArgType::String);
        assert_eq!(placeholders, vec!["VALUE"]);
        assert!(!leftovers);
    }

    #[test]
    fn test_multiple_names_whitespace() {
        let (names, arg_type, placeholders, leftovers) =
            parse_arg_name("-f,   --file,  -F  FILENAME");
        assert_eq!(names, vec!["-f", "--file", "-F"]);
        assert_eq!(arg_type, SyntaxOptArgType::String);
        assert_eq!(placeholders, vec!["FILENAME"]);
        assert!(!leftovers);
    }
}

mod syntax_opt_arg_type {
    use super::*;
    use crate::internal::config::ConfigValue;

    #[test]
    fn test_from_config_value_list_as_enum() {
        let error_handler = ConfigErrorHandler::default();

        // Test with array of strings as type
        let type_value = ConfigValue::from_str("[debug, info, warn, error]").unwrap();
        let result =
            SyntaxOptArgType::from_config_value(Some(&type_value), None, None, &error_handler);

        assert_eq!(
            result,
            Some(SyntaxOptArgType::Enum(vec![
                "debug".to_string(),
                "info".to_string(),
                "warn".to_string(),
                "error".to_string(),
            ]))
        );
    }

    #[test]
    fn test_from_config_value_traditional_enum() {
        let error_handler = ConfigErrorHandler::default();

        // Test traditional enum syntax with separate values
        let type_value = ConfigValue::from_str("enum").unwrap();
        let values_value = ConfigValue::from_str("[one, two, three]").unwrap();
        let result = SyntaxOptArgType::from_config_value(
            Some(&type_value),
            Some(&values_value),
            None,
            &error_handler,
        );

        assert_eq!(
            result,
            Some(SyntaxOptArgType::Enum(vec![
                "one".to_string(),
                "two".to_string(),
                "three".to_string(),
            ]))
        );
    }

    #[test]
    fn test_from_config_value_inline_enum() {
        let error_handler = ConfigErrorHandler::default();

        // Test inline enum syntax
        let type_value = ConfigValue::from_str("enum(fast, safe, rollback)").unwrap();
        let result =
            SyntaxOptArgType::from_config_value(Some(&type_value), None, None, &error_handler);

        assert_eq!(
            result,
            Some(SyntaxOptArgType::Enum(vec![
                "fast".to_string(),
                "safe".to_string(),
                "rollback".to_string(),
            ]))
        );
    }

    #[test]
    fn test_from_config_value_basic_types() {
        let error_handler = ConfigErrorHandler::default();

        // Test basic string type
        let type_value = ConfigValue::from_str("str").unwrap();
        let result =
            SyntaxOptArgType::from_config_value(Some(&type_value), None, None, &error_handler);
        assert_eq!(result, Some(SyntaxOptArgType::String));

        // Test integer type
        let type_value = ConfigValue::from_str("int").unwrap();
        let result =
            SyntaxOptArgType::from_config_value(Some(&type_value), None, None, &error_handler);
        assert_eq!(result, Some(SyntaxOptArgType::Integer));

        // Test boolean type
        let type_value = ConfigValue::from_str("bool").unwrap();
        let result =
            SyntaxOptArgType::from_config_value(Some(&type_value), None, None, &error_handler);
        assert_eq!(result, Some(SyntaxOptArgType::Boolean));
    }

    #[test]
    fn test_from_config_value_empty_list() {
        let error_handler = ConfigErrorHandler::default();

        // Test with empty array
        let type_value = ConfigValue::from_str("[]").unwrap();
        let result =
            SyntaxOptArgType::from_config_value(Some(&type_value), None, None, &error_handler);

        assert_eq!(result, Some(SyntaxOptArgType::Enum(vec![])));
    }

    #[test]
    fn test_from_config_value_single_item_list() {
        let error_handler = ConfigErrorHandler::default();

        // Test with single item array
        let type_value = ConfigValue::from_str("[debug]").unwrap();
        let result =
            SyntaxOptArgType::from_config_value(Some(&type_value), None, None, &error_handler);

        assert_eq!(
            result,
            Some(SyntaxOptArgType::Enum(vec!["debug".to_string()]))
        );
    }

    #[test]
    fn test_from_config_value_precedence() {
        let error_handler = ConfigErrorHandler::default();

        // Test that list syntax takes precedence over values field
        let type_value = ConfigValue::from_str("[debug, info]").unwrap();
        let values_value = ConfigValue::from_str("[ignored, values]").unwrap();
        let result = SyntaxOptArgType::from_config_value(
            Some(&type_value),
            Some(&values_value),
            None,
            &error_handler,
        );

        // Should use the list from type, not values
        assert_eq!(
            result,
            Some(SyntaxOptArgType::Enum(vec![
                "debug".to_string(),
                "info".to_string(),
            ]))
        );
    }
}
