use super::*;

mod dynamic_env {
    use super::*;
    use crate::internal::cache::up_environments::UpEnvironment;
    use crate::internal::cache::up_environments::UpVersion;

    fn create_test_up_version(
        tool: &str,
        backend: &str,
        version: &str,
        bin_path: &str,
        data_path: Option<String>,
    ) -> UpVersion {
        UpVersion {
            tool: tool.to_string(),
            plugin_name: tool.to_string(),
            normalized_name: tool.to_string(),
            backend: backend.to_string(),
            version: version.to_string(),
            bin_path: bin_path.to_string(),
            dir: String::new(),
            data_path,
        }
    }

    fn create_test_environment_with_versions(versions: Vec<UpVersion>) -> UpEnvironment {
        UpEnvironment {
            versions,
            paths: Vec::new(),
            env_vars: Vec::new(),
            config_modtimes: std::collections::BTreeMap::new(),
            config_hash: String::new(),
        }
    }

    fn create_test_dynamic_env() -> DynamicEnv {
        DynamicEnv {
            path: Some(".".to_string()),
            environment: OnceCell::new(),
            id: OnceCell::new(),
            data_str: None,
            data: None,
            features: Vec::new(),
            cache: UpEnvironmentsCache::get(),
        }
    }

    mod apply_versions {
        use super::*;

        #[test]
        fn test_ghrelease_backend() {
            let versions = vec![create_test_up_version(
                "gh",
                "ghrelease",
                "2.0.0",
                "bin",
                None,
            )];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();
            let path_additions = env_data.lists.get("PATH").unwrap();
            assert_eq!(path_additions.len(), 1);
            assert!(path_additions[0].value.ends_with("/gh/2.0.0"));
        }

        #[test]
        fn test_cargo_install_backend() {
            let versions = vec![create_test_up_version(
                "ripgrep",
                "cargo-install",
                "13.0.0",
                "bin",
                None,
            )];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();
            let path_additions = env_data.lists.get("PATH").unwrap();
            assert_eq!(path_additions.len(), 1);
            assert!(path_additions[0].value.ends_with("/ripgrep/13.0.0/bin"));
        }

        #[test]
        fn test_go_install_backend() {
            let versions = vec![create_test_up_version(
                "google.golang.org/protobuf/cmd/protoc-gen-go",
                "go-install",
                "1.2.3",
                "bin",
                None,
            )];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();
            let path_additions = env_data.lists.get("PATH").unwrap();
            assert_eq!(path_additions.len(), 1);
            assert!(path_additions[0]
                .value
                .ends_with("/google.golang.org/protobuf/cmd/protoc-gen-go/1.2.3/bin"));
        }

        #[test]
        fn test_ruby_tool_setup() {
            let versions = vec![create_test_up_version("ruby", "", "3.1.0", "bin", None)];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            assert!(env_data.values.contains_key("GEM_HOME"));
            assert!(env_data.values.contains_key("GEM_ROOT"));
            assert!(env_data.values.contains_key("RUBY_ENGINE"));
            assert!(env_data.values.contains_key("RUBY_ROOT"));
            assert!(env_data.values.contains_key("RUBY_VERSION"));

            assert_eq!(
                env_data
                    .values
                    .get("RUBY_ENGINE")
                    .unwrap()
                    .curr
                    .as_ref()
                    .unwrap(),
                "ruby"
            );
            assert_eq!(
                env_data
                    .values
                    .get("RUBY_VERSION")
                    .unwrap()
                    .curr
                    .as_ref()
                    .unwrap(),
                "3.1.0"
            );

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(path_additions.len() >= 2);

            let gem_path_additions = env_data.lists.get("GEM_PATH").unwrap();
            assert!(!gem_path_additions.is_empty());
        }

        #[test]
        fn test_ruby_tool_with_data_path() {
            let versions = vec![create_test_up_version(
                "ruby",
                "",
                "3.1.0",
                "bin",
                Some("/custom/gem/path".to_string()),
            )];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            assert_eq!(
                env_data
                    .values
                    .get("GEM_HOME")
                    .unwrap()
                    .curr
                    .as_ref()
                    .unwrap(),
                "/custom/gem/path"
            );

            let gem_path_additions = env_data.lists.get("GEM_PATH").unwrap();
            assert!(gem_path_additions
                .iter()
                .any(|p| p.value == "/custom/gem/path"));

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(path_additions
                .iter()
                .any(|p| p.value == "/custom/gem/path/bin"));
        }

        #[test]
        fn test_rust_tool_setup() {
            std::env::set_var("RUSTUP_HOME", "invalid_value");
            std::env::remove_var("CARGO_HOME");
            std::env::set_var("RUSTUP_TOOLCHAIN", "invalid_value");

            let versions = vec![create_test_up_version("rust", "", "1.70.0", "bin", None)];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();
            eprintln!("env_data: {env_data:?}");

            assert!(env_data.values.contains_key("RUSTUP_HOME"));
            assert!(env_data.values.contains_key("CARGO_HOME"));
            assert!(env_data.values.contains_key("RUSTUP_TOOLCHAIN"));

            assert_eq!(
                env_data
                    .values
                    .get("RUSTUP_TOOLCHAIN")
                    .unwrap()
                    .curr
                    .as_ref()
                    .unwrap(),
                "1.70.0"
            );

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(!path_additions.is_empty());
        }

        #[test]
        fn test_rust_tool_with_data_path() {
            let versions = vec![create_test_up_version(
                "rust",
                "",
                "1.70.0",
                "bin",
                Some("/custom/cargo/install".to_string()),
            )];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            assert_eq!(
                env_data
                    .values
                    .get("CARGO_INSTALL_ROOT")
                    .unwrap()
                    .curr
                    .as_ref()
                    .unwrap(),
                "/custom/cargo/install"
            );

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(path_additions
                .iter()
                .any(|p| p.value == "/custom/cargo/install/bin"));
        }

        #[test]
        fn test_go_tool_setup() {
            std::env::remove_var("GOROOT");
            std::env::remove_var("GOMODCACHE");

            let versions = vec![create_test_up_version("go", "", "1.20.0", "bin", None)];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            assert!(env_data.values.contains_key("GOROOT"));
            assert!(env_data.values.contains_key("GOVERSION"));
            assert!(env_data.values.contains_key("GOBIN"));
            assert!(env_data.values.contains_key("GOMODCACHE"));

            assert_eq!(
                env_data
                    .values
                    .get("GOVERSION")
                    .unwrap()
                    .curr
                    .as_ref()
                    .unwrap(),
                "1.20.0"
            );

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(!path_additions.is_empty());
        }

        #[test]
        fn test_go_tool_with_data_path() {
            let versions = vec![create_test_up_version(
                "go",
                "",
                "1.20.0",
                "bin",
                Some("/custom/gopath".to_string()),
            )];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            assert_eq!(
                env_data.values.get("GOBIN").unwrap().curr.as_ref().unwrap(),
                "/custom/gopath/bin"
            );

            let gopath_additions = env_data.lists.get("GOPATH").unwrap();
            assert!(gopath_additions.iter().any(|p| p.value == "/custom/gopath"));

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(path_additions
                .iter()
                .any(|p| p.value == "/custom/gopath/bin"));
        }

        #[test]
        fn test_python_tool_setup() {
            let versions = vec![create_test_up_version("python", "", "3.11.0", "bin", None)];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            assert!(env_data.values.contains_key("POETRY_CONFIG_DIR"));
            assert!(env_data.values.contains_key("POETRY_CACHE_DIR"));
            assert!(env_data.values.contains_key("POETRY_DATA_DIR"));

            if let Some(pythonhome_val) = env_data.values.get("PYTHONHOME") {
                assert!(pythonhome_val.curr.is_none());
            }

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(!path_additions.is_empty());
        }

        #[test]
        fn test_python_tool_with_data_path() {
            let versions = vec![create_test_up_version(
                "python",
                "",
                "3.11.0",
                "bin",
                Some("/custom/venv".to_string()),
            )];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            assert_eq!(
                env_data
                    .values
                    .get("VIRTUAL_ENV")
                    .unwrap()
                    .curr
                    .as_ref()
                    .unwrap(),
                "/custom/venv"
            );
            assert_eq!(
                env_data
                    .values
                    .get("UV_PROJECT_ENVIRONMENT")
                    .unwrap()
                    .curr
                    .as_ref()
                    .unwrap(),
                "/custom/venv"
            );

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(path_additions.iter().any(|p| p.value == "/custom/venv/bin"));
        }

        #[test]
        fn test_node_tool_setup() {
            let versions = vec![create_test_up_version("node", "", "18.0.0", "bin", None)];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            assert!(env_data.values.contains_key("NODE_VERSION"));
            assert_eq!(
                env_data
                    .values
                    .get("NODE_VERSION")
                    .unwrap()
                    .curr
                    .as_ref()
                    .unwrap(),
                "18.0.0"
            );

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(!path_additions.is_empty());
        }

        #[test]
        fn test_node_tool_with_data_path() {
            let versions = vec![create_test_up_version(
                "node",
                "",
                "18.0.0",
                "bin",
                Some("/custom/npm".to_string()),
            )];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            assert_eq!(
                env_data
                    .values
                    .get("npm_config_prefix")
                    .unwrap()
                    .curr
                    .as_ref()
                    .unwrap(),
                "/custom/npm"
            );

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(path_additions.iter().any(|p| p.value == "/custom/npm/bin"));
        }

        #[test]
        fn test_helm_tool_setup() {
            let versions = vec![create_test_up_version("helm", "", "3.12.0", "bin", None)];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(!path_additions.is_empty());
        }

        #[test]
        fn test_helm_tool_with_data_path() {
            let versions = vec![create_test_up_version(
                "helm",
                "",
                "3.12.0",
                "bin",
                Some("/custom/helm".to_string()),
            )];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            assert_eq!(
                env_data
                    .values
                    .get("HELM_CONFIG_HOME")
                    .unwrap()
                    .curr
                    .as_ref()
                    .unwrap(),
                "/custom/helm/config"
            );
            assert_eq!(
                env_data
                    .values
                    .get("HELM_CACHE_HOME")
                    .unwrap()
                    .curr
                    .as_ref()
                    .unwrap(),
                "/custom/helm/cache"
            );
            assert_eq!(
                env_data
                    .values
                    .get("HELM_DATA_HOME")
                    .unwrap()
                    .curr
                    .as_ref()
                    .unwrap(),
                "/custom/helm/data"
            );
        }

        #[test]
        fn test_generic_tool_setup() {
            let versions = vec![create_test_up_version(
                "generic_tool",
                "",
                "1.0.0",
                "bin",
                None,
            )];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(!path_additions.is_empty());
            assert!(dynamic_env
                .features
                .contains(&"generic_tool:1.0.0".to_string()));
        }

        #[test]
        fn test_default_backend() {
            let versions = vec![create_test_up_version(
                "some_tool",
                "default",
                "1.0.0",
                "bin",
                None,
            )];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(!path_additions.is_empty());
            assert!(dynamic_env
                .features
                .contains(&"some_tool:1.0.0".to_string()));
        }

        #[test]
        fn test_empty_backend() {
            let versions = vec![create_test_up_version(
                "another_tool",
                "",
                "2.0.0",
                "bin",
                None,
            )];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(!path_additions.is_empty());
            assert!(dynamic_env
                .features
                .contains(&"another_tool:2.0.0".to_string()));
        }

        #[test]
        fn test_unknown_backend() {
            let versions = vec![create_test_up_version(
                "unknown_tool",
                "unknown_backend",
                "1.0.0",
                "bin",
                None,
            )];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            assert!(
                !env_data.lists.contains_key("PATH")
                    || env_data.lists.get("PATH").unwrap().is_empty()
            );
            assert!(!dynamic_env
                .features
                .contains(&"unknown_tool:1.0.0".to_string()));
        }

        #[test]
        fn test_empty_bin_path() {
            let versions = vec![create_test_up_version("tool_no_bin", "", "1.0.0", "", None)];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(!path_additions.is_empty());
            assert!(!path_additions[0].value.contains("/bin"));
        }

        #[test]
        fn test_custom_bin_path() {
            let versions = vec![create_test_up_version(
                "tool_custom_bin",
                "",
                "1.0.0",
                "custom/bin",
                None,
            )];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(!path_additions.is_empty());
            assert!(path_additions[0].value.contains("/custom/bin"));
        }

        #[test]
        fn test_multiple_versions() {
            let versions = vec![
                create_test_up_version("node", "", "18.0.0", "bin", None),
                create_test_up_version("python", "", "3.11.0", "bin", None),
                create_test_up_version("gh", "ghrelease", "2.0.0", "bin", None),
            ];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            assert!(env_data.values.contains_key("NODE_VERSION"));
            assert!(env_data.values.contains_key("POETRY_CONFIG_DIR"));
            assert!(dynamic_env.features.contains(&"node:18.0.0".to_string()));
            assert!(dynamic_env.features.contains(&"python:3.11.0".to_string()));

            let path_additions = env_data.lists.get("PATH").unwrap();
            assert!(path_additions.len() >= 3);
        }

        #[test]
        fn test_go_with_existing_goroot() {
            std::env::set_var("GOROOT", "/existing/go/root");
            std::env::set_var("PATH", "/existing/go/root/bin:/usr/bin");

            let versions = vec![create_test_up_version("go", "", "1.20.0", "bin", None)];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            let path_operations = env_data.lists.get("PATH").unwrap();
            assert!(path_operations
                .iter()
                .any(|p| p.operation == DynamicEnvListOperation::Del
                    && p.value == "/existing/go/root/bin"));

            std::env::remove_var("GOROOT");
            std::env::remove_var("PATH");
        }

        #[test]
        fn test_ruby_with_existing_gem_vars() {
            std::env::set_var("RUBY_ROOT", "/existing/ruby");
            std::env::set_var("GEM_ROOT", "/existing/gem");
            std::env::set_var("GEM_HOME", "/existing/gem/home");
            std::env::set_var(
                "PATH",
                "/existing/ruby/bin:/existing/gem/bin:/existing/gem/home/bin:/usr/bin",
            );

            let versions = vec![create_test_up_version("ruby", "", "3.1.0", "bin", None)];
            let up_env = create_test_environment_with_versions(versions);
            let mut dynamic_env = create_test_dynamic_env();
            let mut envsetter = DynamicEnvSetter::new();

            dynamic_env.apply_versions(&up_env, &mut envsetter, "");

            let env_data = envsetter.get_env_data();

            let path_list = env_data.lists.get("PATH").unwrap();
            let removals: Vec<_> = path_list
                .iter()
                .filter(|p| p.operation == DynamicEnvListOperation::Del)
                .collect();
            assert!(removals.iter().any(|p| p.value == "/existing/ruby/bin"));
            assert!(removals.iter().any(|p| p.value == "/existing/gem/bin"));
            assert!(removals.iter().any(|p| p.value == "/existing/gem/home/bin"));

            std::env::remove_var("RUBY_ROOT");
            std::env::remove_var("GEM_ROOT");
            std::env::remove_var("GEM_HOME");
            std::env::remove_var("PATH");
        }
    }
}
