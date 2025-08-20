use super::*;

mod multi_from_config_value {
    use super::*;

    #[test]
    fn empty() {
        let yaml = "";
        let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
        let config = UpConfigGithubReleases::from_config_value(
            Some(&config_value),
            &ConfigErrorHandler::noop(),
        );
        assert_eq!(config.releases.len(), 0);
    }

    #[test]
    fn str() {
        let yaml = "owner/repo";
        let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
        let config = UpConfigGithubReleases::from_config_value(
            Some(&config_value),
            &ConfigErrorHandler::noop(),
        );
        assert_eq!(config.releases.len(), 1);
        assert_eq!(config.releases[0].repository, "owner/repo");
        assert_eq!(config.releases[0].version, None);
        assert!(!config.releases[0].prerelease);
        assert!(!config.releases[0].build);
        assert!(config.releases[0].binary);
        assert_eq!(config.releases[0].api_url, None);
    }

    #[test]
    fn object_single() {
        let yaml = r#"{"repository": "owner/repo"}"#;
        let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
        let config = UpConfigGithubReleases::from_config_value(
            Some(&config_value),
            &ConfigErrorHandler::noop(),
        );
        assert_eq!(config.releases.len(), 1);
        assert_eq!(config.releases[0].repository, "owner/repo");
        assert_eq!(config.releases[0].version, None);
        assert!(!config.releases[0].prerelease);
        assert!(!config.releases[0].build);
        assert!(config.releases[0].binary);
        assert_eq!(config.releases[0].api_url, None);
    }

    #[test]
    fn object_multi() {
        let yaml = r#"{"owner/repo": "1.2.3", "owner2/repo2": {"version": "2.3.4", "prerelease": true, "build": true, "binary": false, "api_url": "https://gh.example.com"}, "owner3/repo3": {}}"#;
        let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
        let config = UpConfigGithubReleases::from_config_value(
            Some(&config_value),
            &ConfigErrorHandler::noop(),
        );
        assert_eq!(config.releases.len(), 3);

        assert_eq!(config.releases[0].repository, "owner/repo");
        assert_eq!(config.releases[0].version, Some("1.2.3".to_string()));
        assert!(!config.releases[0].prerelease);
        assert!(!config.releases[0].build);
        assert!(config.releases[0].binary);
        assert_eq!(config.releases[0].api_url, None);

        assert_eq!(config.releases[1].repository, "owner2/repo2");
        assert_eq!(config.releases[1].version, Some("2.3.4".to_string()));
        assert!(config.releases[1].prerelease);
        assert!(config.releases[1].build);
        assert!(!config.releases[1].binary);
        assert_eq!(
            config.releases[1].api_url,
            Some("https://gh.example.com".to_string())
        );

        assert_eq!(config.releases[2].repository, "owner3/repo3");
        assert_eq!(config.releases[2].version, None);
        assert!(!config.releases[2].prerelease);
        assert!(!config.releases[2].build);
        assert!(config.releases[2].binary);
        assert_eq!(config.releases[2].api_url, None);
    }

    #[test]
    fn list_multi() {
        let yaml = r#"["owner/repo", {"repository": "owner2/repo2", "version": "2.3.4", "prerelease": true, "build": true, "binary": false, "api_url": "https://gh.example.com"}, {"owner3/repo3": "3.4.5"}, {"owner4/repo4": {"version": "4.5.6"}}]"#;
        let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
        let config = UpConfigGithubReleases::from_config_value(
            Some(&config_value),
            &ConfigErrorHandler::noop(),
        );
        assert_eq!(config.releases.len(), 4);

        assert_eq!(config.releases[0].repository, "owner/repo");
        assert_eq!(config.releases[0].version, None);
        assert!(!config.releases[0].prerelease);
        assert!(!config.releases[0].build);
        assert!(config.releases[0].binary);
        assert_eq!(config.releases[0].api_url, None);

        assert_eq!(config.releases[1].repository, "owner2/repo2");
        assert_eq!(config.releases[1].version, Some("2.3.4".to_string()));
        assert!(config.releases[1].prerelease);
        assert!(config.releases[1].build);
        assert!(!config.releases[1].binary);
        assert_eq!(
            config.releases[1].api_url,
            Some("https://gh.example.com".to_string())
        );

        assert_eq!(config.releases[2].repository, "owner3/repo3");
        assert_eq!(config.releases[2].version, Some("3.4.5".to_string()));
        assert!(!config.releases[2].prerelease);
        assert!(!config.releases[2].build);
        assert!(config.releases[2].binary);
        assert_eq!(config.releases[2].api_url, None);

        assert_eq!(config.releases[3].repository, "owner4/repo4");
        assert_eq!(config.releases[3].version, Some("4.5.6".to_string()));
        assert!(!config.releases[3].prerelease);
        assert!(!config.releases[3].build);
        assert!(config.releases[3].binary);
        assert_eq!(config.releases[3].api_url, None);
    }
}

mod single_from_config_value {
    use super::*;

    #[test]
    fn str() {
        let yaml = "owner/repo";
        let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
        let config = UpConfigGithubRelease::from_config_value(
            Some(&config_value),
            &ConfigErrorHandler::noop(),
        );
        assert_eq!(config.repository, "owner/repo");
        assert_eq!(config.version, None);
        assert!(!config.prerelease);
        assert!(!config.build);
        assert!(config.binary);
        assert_eq!(config.api_url, None);
    }

    #[test]
    fn object() {
        let yaml = r#"{"repository": "owner/repo"}"#;
        let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
        let config = UpConfigGithubRelease::from_config_value(
            Some(&config_value),
            &ConfigErrorHandler::noop(),
        );
        assert_eq!(config.repository, "owner/repo");
        assert_eq!(config.version, None);
        assert!(!config.prerelease);
        assert!(!config.build);
        assert!(config.binary);
        assert_eq!(config.api_url, None);
    }

    #[test]
    fn object_repo_alias() {
        let yaml = r#"{"repo": "owner/repo"}"#;
        let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
        let config = UpConfigGithubRelease::from_config_value(
            Some(&config_value),
            &ConfigErrorHandler::noop(),
        );
        assert_eq!(config.repository, "owner/repo");
        assert_eq!(config.version, None);
        assert!(!config.prerelease);
        assert!(!config.build);
        assert!(config.binary);
        assert_eq!(config.api_url, None);
    }

    #[test]
    fn with_all_values() {
        let yaml = r#"{"repository": "owner/repo", "version": "1.2.3", "prerelease": true, "build": true, "binary": false, "api_url": "https://gh.example.com"}"#;
        let config_value = ConfigValue::from_str(yaml).expect("failed to create config value");
        let config = UpConfigGithubRelease::from_config_value(
            Some(&config_value),
            &ConfigErrorHandler::noop(),
        );
        assert_eq!(config.repository, "owner/repo");
        assert_eq!(config.version, Some("1.2.3".to_string()));
        assert!(config.prerelease);
        assert!(config.build);
        assert!(!config.binary);
        assert_eq!(config.api_url, Some("https://gh.example.com".to_string()));
    }
}

mod up {
    use super::*;

    use crate::internal::build::compatible_release_arch;
    use crate::internal::build::compatible_release_os;
    use crate::internal::testutils::run_with_env;

    #[test]
    fn latest_release_binary() {
        test_download_release(
            TestOptions::default().version("v1.2.3"),
            UpConfigGithubRelease {
                repository: "owner/repo".to_string(),
                ..UpConfigGithubRelease::default()
            },
        );
    }

    #[test]
    fn latest_release_binary_with_prerelease() {
        test_download_release(
            TestOptions::default().version("v2.0.0-alpha"),
            UpConfigGithubRelease {
                repository: "owner/repo".to_string(),
                prerelease: true,
                ..UpConfigGithubRelease::default()
            },
        );
    }

    #[test]
    fn specific_release_binary_1_major() {
        test_download_release(
            TestOptions::default().version("v1.2.3"),
            UpConfigGithubRelease {
                repository: "owner/repo".to_string(),
                version: Some("1".to_string()),
                ..UpConfigGithubRelease::default()
            },
        );
    }

    #[test]
    fn specific_release_binary_1_1_minor() {
        test_download_release(
            TestOptions::default().version("v1.1.9"),
            UpConfigGithubRelease {
                repository: "owner/repo".to_string(),
                version: Some("1.1".to_string()),
                ..UpConfigGithubRelease::default()
            },
        );
    }

    #[test]
    fn specific_release_binary_1_2_2_full() {
        test_download_release(
            TestOptions::default().version("v1.2.2"),
            UpConfigGithubRelease {
                repository: "owner/repo".to_string(),
                version: Some("1.2.2".to_string()),
                ..UpConfigGithubRelease::default()
            },
        );
    }

    #[test]
    fn different_prefix() {
        test_download_release(
            TestOptions::default().version("prefix-1.2.0"),
            UpConfigGithubRelease {
                repository: "owner/repo".to_string(),
                version: Some("1.2.0".to_string()),
                ..UpConfigGithubRelease::default()
            },
        );
    }

    #[test]
    fn non_standard_version() {
        test_download_release(
            TestOptions::default().version("nonstandard"),
            UpConfigGithubRelease {
                repository: "owner/repo".to_string(),
                version: Some("nonstandard".to_string()),
                ..UpConfigGithubRelease::default()
            },
        );
    }

    #[test]
    fn more_than_one_asset() {
        test_download_release(
            TestOptions::default().version("twoassets").assets(2),
            UpConfigGithubRelease {
                repository: "owner/repo".to_string(),
                version: Some("twoassets".to_string()),
                ..UpConfigGithubRelease::default()
            },
        );
    }

    #[test]
    fn fails_if_binary_is_false_and_only_binaries() {
        test_download_release(
            TestOptions::default(),
            UpConfigGithubRelease {
                repository: "owner/repo".to_string(),
                version: None,
                binary: false,
                ..UpConfigGithubRelease::default()
            },
        );
    }

    #[test]
    fn fails_if_no_assets() {
        test_download_release(
            TestOptions::default(),
            UpConfigGithubRelease {
                repository: "owner/repo".to_string(),
                version: Some("noassets".to_string()),
                ..UpConfigGithubRelease::default()
            },
        );
    }

    #[test]
    fn fails_if_no_matching_assets() {
        test_download_release(
            TestOptions::default(),
            UpConfigGithubRelease {
                repository: "owner/repo".to_string(),
                version: Some("nomatchingassets".to_string()),
                ..UpConfigGithubRelease::default()
            },
        );
    }

    #[derive(Default)]
    struct TestOptions {
        expected_version: Option<String>,
        assets: usize,
    }

    impl TestOptions {
        fn version(mut self, version: &str) -> Self {
            self.expected_version = Some(version.to_string());
            if self.assets == 0 {
                self.assets = 1;
            }
            self
        }

        fn assets(mut self, assets: usize) -> Self {
            self.assets = assets;
            self
        }
    }

    fn test_download_release(test: TestOptions, config: UpConfigGithubRelease) {
        run_with_env(&[], || {
            let mut mock_server = mockito::Server::new();
            let api_url = mock_server.url();

            let config = UpConfigGithubRelease {
                api_url: Some(api_url.to_string()),
                ..config
            };

            let current_arch = compatible_release_arch()
                .into_iter()
                .next()
                .expect("no compatible arch")
                .into_iter()
                .next()
                .expect("no compatible arch");
            let current_os = compatible_release_os()
                .into_iter()
                .next()
                .expect("no compatible os");

            let list_releases_body = format!(
                r#"[
                {{
                    "name": "Release 2.0.0-alpha",
                    "tag_name": "v2.0.0-alpha",
                    "draft": false,
                    "prerelease": true,
                    "assets": [
                        {{
                            "name": "asset1_{arch}_{os}",
                            "url": "{url}/download/v2.0.0-alpha/asset1"
                        }}
                    ]
                }},
                {{
                    "name": "Release 1.2.3",
                    "tag_name": "v1.2.3",
                    "draft": false,
                    "prerelease": false,
                    "assets": [
                        {{
                            "name": "asset1_{arch}_{os}",
                            "url": "{url}/download/v1.2.3/asset1"
                        }}
                    ]
                }},
                {{
                    "name": "Release 1.2.2",
                    "tag_name": "v1.2.2",
                    "draft": false,
                    "prerelease": false,
                    "assets": [
                        {{
                            "name": "asset1_{arch}_{os}",
                            "url": "{url}/download/v1.2.2/asset1"
                        }}
                    ]
                }},
                {{
                    "name": "Release 1.2.0",
                    "tag_name": "prefix-1.2.0",
                    "draft": false,
                    "prerelease": false,
                    "assets": [
                        {{
                            "name": "asset1_{arch}_{os}",
                            "url": "{url}/download/prefix-1.2.0/asset1"
                        }}
                    ]
                }},
                {{
                    "name": "Release nonstandard",
                    "tag_name": "nonstandard",
                    "draft": false,
                    "prerelease": false,
                    "assets": [
                        {{
                            "name": "asset1_{arch}_{os}",
                            "url": "{url}/download/nonstandard/asset1"
                        }}
                    ]
                }},
                {{
                    "name": "Release noassets",
                    "tag_name": "noassets",
                    "draft": false,
                    "prerelease": false,
                    "assets": []
                }},
                {{
                    "name": "Release nomatchingassets",
                    "tag_name": "nomatchingassets",
                    "draft": false,
                    "prerelease": false,
                    "assets": [
                        {{
                            "name": "asset1",
                            "url": "{url}/download/nomatchingassets/asset1"
                        }}
                    ]
                }},
                {{
                    "name": "Release twoassets",
                    "tag_name": "twoassets",
                    "draft": false,
                    "prerelease": false,
                    "assets": [
                        {{
                            "name": "asset1_{arch}_{os}",
                            "url": "{url}/download/twoassets/asset1"
                        }},
                        {{
                            "name": "asset2_{arch}_{os}",
                            "url": "{url}/download/twoassets/asset2"
                        }}
                    ]
                }},
                {{
                    "name": "Release 1.1.9",
                    "tag_name": "v1.1.9",
                    "draft": false,
                    "prerelease": false,
                    "assets": [
                        {{
                            "name": "asset1_{arch}_{os}",
                            "url": "{url}/download/v1.1.9/asset1"
                        }}
                    ]
                }}
            ]"#,
                url = mock_server.url(),
                arch = current_arch,
                os = current_os
            );

            let mock_list_releases = mock_server
                .mock("GET", "/repos/owner/repo/releases?per_page=100&page=1")
                .with_status(200)
                .with_body(list_releases_body)
                .create();

            let mock_downloads = (1..=test.assets)
                .map(|asset_id| {
                    eprintln!("Setting up asset id {asset_id}");
                    mock_server
                        .mock(
                            "GET",
                            format!(
                                "/download/{}/asset{}",
                                test.expected_version.clone().unwrap(),
                                asset_id
                            )
                            .as_str(),
                        )
                        .with_status(200)
                        .with_body(format!("asset{asset_id} contents"))
                        .create()
                })
                .collect::<Vec<_>>();

            let options = UpOptions::default().cache_disabled();
            let mut environment = UpEnvironment::new();
            let progress_handler = UpProgressHandler::new(None);

            let result = config.up(&options, &mut environment, &progress_handler);

            assert!(if test.expected_version.is_some() {
                result.is_ok()
            } else {
                result.is_err()
            });

            // Check the mocks have been called
            mock_list_releases.assert();
            mock_downloads.iter().for_each(|mock| mock.assert());

            for asset_id in 1..=test.assets {
                // Check the binary file exists
                let expected_bin = github_releases_bin_path()
                    .join("owner/repo")
                    .join(test.expected_version.clone().unwrap())
                    .join(format!("asset{asset_id}"));
                if !expected_bin.exists() {
                    // Use walkdir to print all the tree under github_releases_bin_path()
                    let tree = walkdir::WalkDir::new(github_releases_bin_path())
                        .into_iter()
                        .flatten()
                        .map(|entry| entry.path().display().to_string())
                        .collect::<Vec<String>>()
                        .join("\n");
                    panic!(
                        "binary file not found at {}\nExisting paths:\n{}",
                        expected_bin.display(),
                        tree
                    );
                }

                // Check the file is executable
                let metadata = expected_bin.metadata().expect("failed to get metadata");
                assert_eq!(
                    metadata.permissions().mode() & 0o111,
                    0o111,
                    "file is not executable"
                );
            }
        });
    }
}