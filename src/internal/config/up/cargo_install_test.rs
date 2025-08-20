use super::*;

use std::os::unix::fs::PermissionsExt;

use crate::internal::testutils::run_with_env;

mod parse_cargo_crate_name {
    use super::*;

    #[test]
    fn simple_crate() {
        let (name, version) = parse_cargo_crate_name("serde").unwrap();
        assert_eq!(name, "serde");
        assert_eq!(version, None);
    }

    #[test]
    fn crate_with_version() {
        let (name, version) = parse_cargo_crate_name("serde@1.0.0").unwrap();
        assert_eq!(name, "serde");
        assert_eq!(version, Some("1.0.0".to_string()));
    }

    #[test]
    fn invalid_multiple_at() {
        let result = parse_cargo_crate_name("serde@1.0.0@latest");
        assert!(matches!(
            result,
            Err(CargoInstallError::InvalidCrateName(_))
        ));
    }
}

mod install {
    use super::*;

    #[test]
    fn latest_version() {
        test_install_crate(
            TestOptions::default().version("1.2.3"),
            UpConfigCargoInstall {
                crate_name: "test-crate".to_string(),
                ..UpConfigCargoInstall::default()
            },
        );
    }

    #[test]
    fn specific_version() {
        test_install_crate(
            TestOptions::default().version("1.0.0").no_list(),
            UpConfigCargoInstall {
                crate_name: "test-crate".to_string(),
                version: Some("1.0.0".to_string()),
                exact: true,
                ..UpConfigCargoInstall::default()
            },
        );
    }

    #[test]
    fn with_prerelease() {
        test_install_crate(
            TestOptions::default().version("2.0.0-alpha"),
            UpConfigCargoInstall {
                crate_name: "test-crate".to_string(),
                prerelease: true,
                ..UpConfigCargoInstall::default()
            },
        );
    }

    #[test]
    fn with_build() {
        test_install_crate(
            TestOptions::default().version("2.0.0+build"),
            UpConfigCargoInstall {
                crate_name: "test-crate".to_string(),
                build: true,
                ..UpConfigCargoInstall::default()
            },
        );
    }

    struct TestOptions {
        expected_version: Option<String>,
        list_versions: bool,
        versions: CratesApiVersions,
    }

    impl Default for TestOptions {
        fn default() -> Self {
            TestOptions {
                expected_version: None,
                list_versions: true,
                versions: CratesApiVersions {
                    versions: vec![
                        CratesApiVersion {
                            num: "1.0.0".to_string(),
                            yanked: false,
                        },
                        CratesApiVersion {
                            num: "1.2.3".to_string(),
                            yanked: false,
                        },
                        CratesApiVersion {
                            num: "2.0.0-alpha".to_string(),
                            yanked: false,
                        },
                        CratesApiVersion {
                            num: "2.0.0+build".to_string(),
                            yanked: false,
                        },
                        CratesApiVersion {
                            num: "3.0.0".to_string(),
                            yanked: true,
                        },
                    ],
                },
            }
        }
    }

    impl TestOptions {
        fn version(mut self, version: &str) -> Self {
            self.expected_version = Some(version.to_string());
            self
        }

        fn no_list(mut self) -> Self {
            self.list_versions = false;
            self
        }
    }

    fn test_install_crate(test: TestOptions, config: UpConfigCargoInstall) {
        run_with_env(&[], || {
            let mut mock_server = mockito::Server::new();
            let api_url = mock_server.url();

            let config = UpConfigCargoInstall {
                api_url: Some(api_url.to_string()),
                ..config
            };

            // Mock the crates.io API response
            let versions_response =
                serde_json::to_string(&test.versions).expect("failed to serialize versions");

            let mock_versions = mock_server
                .mock(
                    "GET",
                    format!("/crates/{}/versions", config.crate_name).as_str(),
                )
                .with_status(200)
                .with_body(versions_response)
                .create();

            // Create a temporary cargo binary for testing
            let temp_dir = tempfile::tempdir().unwrap();
            let cargo_bin_path = temp_dir.path().join("cargo");
            let cargo_bin = CargoBin {
                bin: cargo_bin_path.clone(),
                version: "1.0.0".to_string(),
            };
            let script = r#"#!/usr/bin/env bash
                echo "Running mock cargo with args: $@" >&2
                if [[ "$1" != "install" ]]; then
                    echo "Nothing to do" >&2
                    exit 0
                fi
                next_arg_is_root=false
                for arg in "$@"; do
                    echo "Processing arg: $arg" >&2
                    if [[ "$next_arg_is_root" == "true" ]]; then
                        root_dir="$arg"
                        next_arg_is_root=false
                        break
                    fi
                    case "$arg" in
                        --root=*)
                            root_dir="${arg#--root=}"
                            break
                            ;;
                        --root)
                            next_arg_is_root=true
                            ;;
                    esac
                done
                if [[ -z "$root_dir" ]]; then
                    echo "No root directory provided" >&2
                    exit 1
                fi
                echo "Creating bin directory in $root_dir" >&2
                mkdir -p "$root_dir/bin"
                new_bin="$root_dir/bin/fakecrate"
                touch "$new_bin"
                chmod +x "$new_bin"
                exit 0
            "#;

            std::fs::write(&cargo_bin_path, script).expect("failed to write cargo script");
            std::fs::set_permissions(&cargo_bin_path, std::fs::Permissions::from_mode(0o755))
                .expect("failed to set permissions");

            let options = UpOptions::default().cache_disabled();
            let mut environment = UpEnvironment::new();
            let progress_handler = UpProgressHandler::new_void();

            let result = config.up(&options, &mut environment, &progress_handler, &cargo_bin);

            assert!(result.is_ok(), "result should be ok, got {result:?}");
            if test.list_versions {
                mock_versions.assert();
            } else {
                assert!(
                    !mock_versions.matched(),
                    "should not have called the API to list versions"
                );
            }

            // Verify the installed version
            if let Some(expected_version) = test.expected_version {
                assert_eq!(
                    config.actual_version.get(),
                    Some(&expected_version),
                    "Wrong version installed"
                );
            }
        });
    }
}

mod cleanup {
    use super::*;

    #[test]
    fn cleanup_removes_unused() {
        run_with_env(&[], || {
            let progress_handler = UpProgressHandler::new_void();

            // Create some fake installed crates
            let base_path = cargo_install_bin_path();
            std::fs::create_dir_all(&base_path).unwrap();

            // Create test structure
            std::fs::create_dir_all(base_path.join("serde/1.0.0/bin")).unwrap();
            std::fs::create_dir_all(base_path.join("tokio/1.0.0/bin")).unwrap();
            std::fs::create_dir_all(base_path.join("old-crate/0.1.0/bin")).unwrap();

            // Add some crates to cache as "in use"
            let cache = CargoInstallOperationCache::get();
            cache.add_installed("serde", "1.0.0").unwrap();
            cache.add_installed("tokio", "1.0.0").unwrap();

            let result = UpConfigCargoInstalls::cleanup(&progress_handler).unwrap();

            // Verify cleanup message
            assert!(result.is_some());
            assert!(result.unwrap().contains("old-crate"));

            // Verify directory structure
            assert!(base_path.join("serde/1.0.0").exists());
            assert!(base_path.join("tokio/1.0.0").exists());
            assert!(!base_path.join("old-crate").exists());
        });
    }
}