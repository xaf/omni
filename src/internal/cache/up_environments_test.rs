use super::*;

use crate::internal::testutils::run_with_env;
use crate::internal::ConfigLoader;
use crate::internal::ConfigValue;

mod up_environments_cache {
    use super::*;

    #[test]
    fn test_get_and_assign_environment() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();
            let workdir_id = "test-workdir";
            let mut env = UpEnvironment::new().init();

            // Initially no environment exists
            assert!(cache.get_env(workdir_id).is_none());

            // Assign environment
            let (is_new, newly_assigned, _env_id) = cache
                .assign_environment(workdir_id, Some("test-sha".to_string()), &mut env)
                .expect("Failed to assign environment");
            assert!(is_new);
            assert!(newly_assigned);

            // Get environment and verify
            let retrieved = cache
                .get_env(workdir_id)
                .expect("Failed to get environment");
            assert_eq!(retrieved.config_hash, env.config_hash);
        });
    }

    #[test]
    fn test_assign_already_existing_environment() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();
            let workdir_id = "test-workdir";
            let mut env = UpEnvironment::new().init();

            // Assign environment
            let (is_new, newly_assigned, _env_id) = cache
                .assign_environment(workdir_id, Some("test-sha".to_string()), &mut env)
                .expect("Failed to assign environment");
            assert!(is_new);
            assert!(newly_assigned);

            // Assign environment again
            let (is_new, newly_assigned, _env_id) = cache
                .assign_environment(workdir_id, Some("test-sha".to_string()), &mut env)
                .expect("Failed to assign environment");
            assert!(!is_new);
            assert!(!newly_assigned);
        });
    }

    #[test]
    fn test_clear_environment() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();
            let workdir_id = "test-workdir";
            let mut env = UpEnvironment::new().init();

            // Initially no environment exists
            assert!(cache.get_env(workdir_id).is_none());

            // Assign environment
            let (_is_new, _newly_assigned, _env_id) = cache
                .assign_environment(workdir_id, Some("dumb".to_string()), &mut env)
                .expect("Failed to assign environment");

            // Verify it now has an environment
            assert!(cache.get_env(workdir_id).is_some());

            // Clear environment
            let cleared = cache
                .clear(workdir_id)
                .expect("Failed to clear environment");
            assert!(cleared);

            // Verify environment is cleared
            assert!(cache.get_env(workdir_id).is_none());
        });
    }

    #[test]
    fn test_environment_ids() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();
            let workdir_id = "test-workdir";
            let mut env = UpEnvironment::new().init();

            // Initially no environments
            assert!(cache.environment_ids().is_empty());

            // Assign environment
            let (_is_new, _newly_assigned, _env_id) = cache
                .assign_environment(workdir_id, None, &mut env)
                .expect("Failed to assign environment");

            // Verify environment id exists
            let ids = cache.environment_ids();
            assert_eq!(ids.len(), 1);
        });
    }

    #[test]
    fn test_assign_environment_with_different_sha() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();
            let workdir_id = "test-workdir";
            let mut env = UpEnvironment::new().init();

            // First assignment
            let (is_new, newly_assigned, env_id1) = cache
                .assign_environment(workdir_id, Some("sha1".to_string()), &mut env)
                .expect("Failed to assign environment");
            assert!(is_new);
            assert!(newly_assigned);

            // Same environment, different SHA
            let (is_new, newly_assigned, env_id2) = cache
                .assign_environment(workdir_id, Some("sha2".to_string()), &mut env)
                .expect("Failed to assign environment");
            assert!(!is_new);
            assert!(!newly_assigned);
            assert_eq!(env_id1, env_id2);
        });
    }

    #[test]
    fn test_assign_environment_without_sha() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();
            let workdir_id = "test-workdir";
            let mut env = UpEnvironment::new().init();

            // Assign without SHA
            let (is_new, newly_assigned, _env_id) = cache
                .assign_environment(workdir_id, None, &mut env)
                .expect("Failed to assign environment");
            assert!(is_new);
            assert!(newly_assigned);

            // Verify environment exists
            assert!(cache.get_env(workdir_id).is_some());
        });
    }

    #[test]
    fn test_multiple_workdir_environments() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();
            let mut env = UpEnvironment::new().init();

            // Assign to multiple workdirs
            let workdirs = ["workdir1", "workdir2", "workdir3"];
            for workdir in workdirs {
                let (is_new, newly_assigned, _env_id) = cache
                    .assign_environment(workdir, None, &mut env)
                    .expect("Failed to assign environment");
                assert!(is_new);
                assert!(newly_assigned);
            }

            // Verify each workdir has environment
            for workdir in workdirs {
                assert!(cache.get_env(workdir).is_some());
            }

            // Verify environment_ids contains all workdirs
            let ids = cache.environment_ids();
            assert_eq!(ids.len(), workdirs.len());
        });
    }

    #[test]
    fn test_clear_nonexistent_environment() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();
            let cleared = cache.clear("nonexistent-workdir").expect("Failed to clear");
            assert!(!cleared);
        });
    }

    #[test]
    fn test_environment_history_cleanup() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();
            let workdir_id = "test-workdir";
            let mut env = UpEnvironment::new().init();

            // Create multiple history entries
            for sha in &["sha1", "sha2", "sha3"] {
                cache
                    .assign_environment(workdir_id, Some(sha.to_string()), &mut env)
                    .expect("Failed to assign environment");
            }

            // Clear and verify cleanup
            let cleared = cache.clear(workdir_id).expect("Failed to clear");
            assert!(cleared);
            assert!(cache.get_env(workdir_id).is_none());
        });
    }

    #[test]
    fn test_assign_modified_environment() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();
            let workdir_id = "test-workdir";
            let mut env = UpEnvironment::new().init();

            // Initial assignment
            let (is_new, newly_assigned, env_id1) = cache
                .assign_environment(workdir_id, None, &mut env)
                .expect("Failed to assign environment");
            assert!(is_new);
            assert!(newly_assigned);

            // Modify environment
            env.add_env_var("TEST_VAR", "test_value");
            let (is_new, newly_assigned, env_id2) = cache
                .assign_environment(workdir_id, None, &mut env)
                .expect("Failed to assign environment");
            assert!(is_new);
            assert!(newly_assigned);
            assert_ne!(env_id1, env_id2);

            // Verify modified environment
            let retrieved = cache
                .get_env(workdir_id)
                .expect("Failed to get environment");
            assert_eq!(retrieved.env_vars.len(), 1);
            assert_eq!(retrieved.env_vars[0].name, "TEST_VAR");
        });
    }

    #[test]
    fn test_environment_retention_max_total_keep_open() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();

            // Write the max_total to the config file
            let expected_max_total = 5;
            if let Err(err) = ConfigLoader::edit_main_user_config_file(|config_value| {
                // Write to cache.environment.max_total, using a yaml string
                *config_value = ConfigValue::from_str(
                    format!("cache:\n  environment:\n    max_total: {expected_max_total}").as_str(),
                )
                .expect("Failed to create config value");

                true
            }) {
                panic!("Failed to edit main user config file: {err}");
            }

            // Check if the config was written correctly
            let max_total = match global_config().cache.environment.max_total {
                None => panic!("Failed to set max_total (None)"),
                Some(n) if n != expected_max_total => {
                    panic!("Failed to set max_total (expected {expected_max_total}, got {n})")
                }
                Some(n) => n,
            };

            // Create environments up to max_total limit
            let mut env = UpEnvironment::new().init();
            for i in 0..(max_total + 3) {
                let workdir = format!("workdir{i}");
                cache
                    .assign_environment(&workdir, None, &mut env)
                    .expect("Failed to assign environment");
            }

            // Verify that we keep the open environments, so none has been removed here
            let ids = cache.environment_ids();
            assert_eq!(ids.len(), max_total + 3);
        });
    }

    #[test]
    fn test_environment_retention_max_total() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();

            // Write the max_total to the config file
            let expected_max_total = 5;
            if let Err(err) = ConfigLoader::edit_main_user_config_file(|config_value| {
                // Write to cache.environment.max_total, using a yaml string
                *config_value = ConfigValue::from_str(
                    format!("cache:\n  environment:\n    max_total: {expected_max_total}").as_str(),
                )
                .expect("Failed to create config value");

                true
            }) {
                panic!("Failed to edit main user config file: {err}");
            }

            // Check if the config was written correctly
            let max_total = match global_config().cache.environment.max_total {
                None => panic!("Failed to set max_total (None)"),
                Some(n) if n != expected_max_total => {
                    panic!("Failed to set max_total (expected {expected_max_total}, got {n})")
                }
                Some(n) => n,
            };

            // Create environments up to max_total limit
            for i in 0..(max_total + 3) {
                let mut env = UpEnvironment::new().init();
                env.add_env_var("TEST_VAR".to_string(), format!("value{i}"));

                cache
                    .assign_environment("workdir", None, &mut env)
                    .expect("Failed to assign environment");
            }

            // Verify that we keep only kept max_total environments
            let ids = cache.environment_ids();
            assert_eq!(ids.len(), max_total);
        });
    }

    #[test]
    fn test_environment_retention_max_per_workdir() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();

            // Write the max_total to the config file
            let expected_max_per_workdir = 2;
            if let Err(err) = ConfigLoader::edit_main_user_config_file(|config_value| {
                // Write to cache.environment.max_total, using a yaml string
                *config_value = ConfigValue::from_str(
                    format!(
                        "cache:\n  environment:\n    max_per_workdir: {expected_max_per_workdir}"
                    )
                    .as_str(),
                )
                .expect("Failed to create config value");

                true
            }) {
                panic!("Failed to edit main user config file: {err}");
            }

            // Check if the config was written correctly
            let max_per_workdir = match global_config().cache.environment.max_per_workdir {
                None => panic!("Failed to set max_per_workdir (None)"),
                Some(n) if n != expected_max_per_workdir => panic!(
                    "Failed to set max_per_workdir (expected {expected_max_per_workdir}, got {n})"
                ),
                Some(n) => n,
            };

            // Create environments up to max_total limit
            let num_workdirs = 5;
            for i in 0..num_workdirs {
                let workdir = format!("workdir{i}");
                for j in 0..(max_per_workdir + 3) {
                    let mut env = UpEnvironment::new().init();
                    env.add_env_var("TEST_VAR".to_string(), format!("value.{i}.{j}"));

                    cache
                        .assign_environment(&workdir, None, &mut env)
                        .expect("Failed to assign environment");
                }
            }

            let expected_total = num_workdirs * max_per_workdir;
            let ids = cache.environment_ids();
            assert_eq!(ids.len(), expected_total);
        });
    }

    #[test]
    fn test_retention_stale_cleanup_old_entries() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();

            // Set retention_stale to 60 seconds for testing
            let retention_stale = 60;
            if let Err(err) = ConfigLoader::edit_main_user_config_file(|config_value| {
                *config_value = ConfigValue::from_str(
                    format!("cache:\n  environment:\n    retention_stale: {retention_stale}s")
                        .as_str(),
                )
                .expect("Failed to create config value");
                true
            }) {
                panic!("Failed to edit main user config file: {err}");
            }

            // Create an environment for workdir1
            let workdir1 = "github.com:test/stale-repo";
            let mut env = UpEnvironment::new().init();
            cache
                .assign_environment(workdir1, None, &mut env)
                .expect("Failed to assign environment");

            // Manually set the dates to be older than retention_stale
            let old_date = "2020-01-01T00:00:00.000Z";
            CacheManager::get()
                .execute(
                    "UPDATE env_history SET used_from_date = ?1, last_seen_at = ?1 WHERE workdir_id = ?2",
                    &[&old_date, &workdir1],
                )
                .expect("Failed to update dates");

            // Create another workdir to trigger cleanup
            let workdir2 = "github.com:test/fresh-repo";
            cache
                .assign_environment(workdir2, None, &mut env)
                .expect("Failed to assign environment");

            // Verify that workdir1 entry was closed (cleaned up)
            let result: Result<i64, _> = CacheManager::get().query_one(
                "SELECT COUNT(*) FROM env_history WHERE workdir_id = ?1 AND used_until_date IS NULL",
                &[&workdir1],
            );
            assert_eq!(result.unwrap(), 0, "Stale entry should have been closed");

            // Verify that workdir2 is still open
            let result: Result<i64, _> = CacheManager::get().query_one(
                "SELECT COUNT(*) FROM env_history WHERE workdir_id = ?1 AND used_until_date IS NULL",
                &[&workdir2],
            );
            assert_eq!(result.unwrap(), 1, "Fresh entry should still be open");
        });
    }

    #[test]
    fn test_retention_stale_keeps_recent_entries() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();

            // Set retention_stale to 60 seconds
            let retention_stale = 60;
            if let Err(err) = ConfigLoader::edit_main_user_config_file(|config_value| {
                *config_value = ConfigValue::from_str(
                    format!("cache:\n  environment:\n    retention_stale: {retention_stale}s")
                        .as_str(),
                )
                .expect("Failed to create config value");
                true
            }) {
                panic!("Failed to edit main user config file: {err}");
            }

            // Create an environment
            let workdir = "github.com:test/recent-repo";
            let mut env = UpEnvironment::new().init();
            cache
                .assign_environment(workdir, None, &mut env)
                .expect("Failed to assign environment");

            // Trigger cleanup by creating another workdir
            cache
                .assign_environment("github.com:test/other-repo", None, &mut env)
                .expect("Failed to assign environment");

            // Verify entry is still open
            let result: Result<i64, _> = CacheManager::get().query_one(
                "SELECT COUNT(*) FROM env_history WHERE workdir_id = ?1 AND used_until_date IS NULL",
                &[&workdir],
            );
            assert_eq!(result.unwrap(), 1, "Recent entry should still be open");
        });
    }

    #[test]
    fn test_retention_stale_uses_max_of_dates() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();

            // Set retention_stale to 60 seconds
            let retention_stale = 60;
            if let Err(err) = ConfigLoader::edit_main_user_config_file(|config_value| {
                *config_value = ConfigValue::from_str(
                    format!("cache:\n  environment:\n    retention_stale: {retention_stale}s")
                        .as_str(),
                )
                .expect("Failed to create config value");
                true
            }) {
                panic!("Failed to edit main user config file: {err}");
            }

            // Create an environment with old used_from_date
            let workdir = "github.com:test/max-date-repo";
            let mut env = UpEnvironment::new().init();
            cache
                .assign_environment(workdir, None, &mut env)
                .expect("Failed to assign environment");

            // Set used_from_date to old, but last_seen_at to recent (now)
            let old_date = "2020-01-01T00:00:00.000Z";
            CacheManager::get()
                .execute(
                    "UPDATE env_history SET used_from_date = ?1 WHERE workdir_id = ?2 AND used_until_date IS NULL",
                    &[&old_date, &workdir],
                )
                .expect("Failed to update used_from_date");

            // Trigger cleanup
            cache
                .assign_environment("github.com:test/trigger-repo", None, &mut env)
                .expect("Failed to assign environment");

            // Verify entry is still open because last_seen_at is recent
            let result: Result<i64, _> = CacheManager::get().query_one(
                "SELECT COUNT(*) FROM env_history WHERE workdir_id = ?1 AND used_until_date IS NULL",
                &[&workdir],
            );
            assert_eq!(
                result.unwrap(),
                1,
                "Entry with recent last_seen_at should still be open"
            );
        });
    }

    #[test]
    fn test_retention_disabled_with_zero() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();

            // Set retention to 0 to disable cleanup
            if let Err(err) = ConfigLoader::edit_main_user_config_file(|config_value| {
                *config_value = ConfigValue::from_str("cache:\n  environment:\n    retention: 0")
                    .expect("Failed to create config value");
                true
            }) {
                panic!("Failed to edit main user config file: {err}");
            }

            // Create and close an environment
            let workdir = "github.com:test/disabled-retention";
            let mut env = UpEnvironment::new().init();
            cache
                .assign_environment(workdir, None, &mut env)
                .expect("Failed to assign environment");

            // Close it
            cache.clear(workdir).expect("Failed to clear");

            // Set the closed date to very old
            let old_date = "2020-01-01T00:00:00.000Z";
            CacheManager::get()
                .execute(
                    "UPDATE env_history SET used_until_date = ?1 WHERE workdir_id = ?2",
                    &[&old_date, &workdir],
                )
                .expect("Failed to update used_until_date");

            // Trigger cleanup by creating another workdir
            cache
                .assign_environment("github.com:test/trigger", None, &mut env)
                .expect("Failed to assign environment");

            // Verify the old closed entry was NOT deleted (retention disabled)
            let result: Result<i64, _> = CacheManager::get().query_one(
                "SELECT COUNT(*) FROM env_history WHERE workdir_id = ?1",
                &[&workdir],
            );
            assert_eq!(
                result.unwrap(),
                1,
                "Closed entry should NOT be deleted when retention = 0"
            );
        });
    }

    #[test]
    fn test_last_seen_at_updated_on_omni_up() {
        run_with_env(&[], || {
            let cache = UpEnvironmentsCache::get();
            let workdir = "github.com:test/update-seen-repo";
            let mut env = UpEnvironment::new().init();

            // Create initial environment
            cache
                .assign_environment(workdir, None, &mut env)
                .expect("Failed to assign environment");

            // Get initial last_seen_at
            let initial_last_seen: String = CacheManager::get()
                .query_one(
                    "SELECT last_seen_at FROM env_history WHERE workdir_id = ?1 AND used_until_date IS NULL",
                    &[&workdir],
                )
                .expect("Failed to get initial last_seen_at");

            // Sleep briefly to ensure time difference
            std::thread::sleep(std::time::Duration::from_millis(10));

            // Run omni up again (assign same environment)
            cache
                .assign_environment(workdir, None, &mut env)
                .expect("Failed to assign environment");

            // Get updated last_seen_at
            let updated_last_seen: String = CacheManager::get()
                .query_one(
                    "SELECT last_seen_at FROM env_history WHERE workdir_id = ?1 AND used_until_date IS NULL",
                    &[&workdir],
                )
                .expect("Failed to get updated last_seen_at");

            // Verify last_seen_at was updated
            assert_ne!(
                initial_last_seen, updated_last_seen,
                "last_seen_at should be updated after omni up"
            );
        });
    }
}

mod up_environment {
    use super::*;

    #[test]
    fn test_new_and_init() {
        let env = UpEnvironment::new().init();
        assert!(env.versions.is_empty());
        assert!(env.paths.is_empty());
        assert!(env.env_vars.is_empty());
        assert!(!env.config_hash.is_empty());
        assert!(!env.config_modtimes.is_empty());
    }

    #[test]
    fn test_versions_for_dir() {
        let mut env = UpEnvironment::new();

        // Add versions for different directories
        env.add_version(UpVersionParams {
            backend: "backend1",
            tool: "tool1",
            plugin_name: "plugin1",
            normalized_name: "plugin-1",
            version: "1.0.0",
            bin_path: "bin/path/1",
            dirs: BTreeSet::from(["dir1".to_string()]),
            env_vars: Vec::new(),
        });
        env.add_version(UpVersionParams {
            backend: "backend2",
            tool: "tool2",
            plugin_name: "plugin2",
            normalized_name: "plugin-2",
            version: "2.0.0",
            bin_path: "bin/path/2",
            dirs: BTreeSet::from(["dir1/subdir".to_string()]),
            env_vars: Vec::new(),
        });
        env.add_version(UpVersionParams {
            backend: "backend3",
            tool: "tool3",
            plugin_name: "plugin3",
            normalized_name: "plugin-3",
            version: "3.0.0",
            bin_path: "bin/path/3",
            dirs: BTreeSet::from(["dir2".to_string()]),
            env_vars: Vec::new(),
        });

        // Test dir1 versions
        let dir1_versions = env.versions_for_dir("dir1");
        assert_eq!(dir1_versions.len(), 1);
        assert_eq!(dir1_versions[0].tool, "tool1");

        // Test dir1/subdir versions
        let subdir_versions = env.versions_for_dir("dir1/subdir");
        assert_eq!(subdir_versions.len(), 2);
        assert_eq!(subdir_versions[0].tool, "tool1");
        assert_eq!(subdir_versions[1].tool, "tool2");

        // Test dir2 versions
        let dir2_versions = env.versions_for_dir("dir2");
        assert_eq!(dir2_versions.len(), 1);
        assert_eq!(dir2_versions[0].tool, "tool3");
    }

    #[test]
    fn test_env_vars() {
        let mut env = UpEnvironment::new();

        // Test adding basic env var
        assert!(env.add_env_var("KEY1", "value1"));
        assert_eq!(env.env_vars.len(), 1);
        assert_eq!(env.env_vars[0].name, "KEY1");
        assert_eq!(env.env_vars[0].value, Some("value1".to_string()));

        // Test adding env var with operation
        assert!(env.add_env_var_operation("KEY2", "value2", EnvOperationEnum::Append));
        assert_eq!(env.env_vars[1].operation, EnvOperationEnum::Append);

        // Test adding raw env vars
        let raw_vars = vec![UpEnvVar {
            name: "KEY3".to_string(),
            value: Some("value3".to_string()),
            operation: EnvOperationEnum::Set,
        }];
        assert!(env.add_raw_env_vars(raw_vars));
        assert_eq!(env.env_vars.len(), 3);
    }

    #[test]
    fn test_paths() {
        run_with_env(&[], || {
            let mut env = UpEnvironment::new();
            let data_home_path = PathBuf::from(data_home()).join("test");
            let regular_path = PathBuf::from("/usr/local/bin");

            // Test adding single path
            assert!(env.add_path(regular_path.clone()));
            assert_eq!(env.paths.len(), 1);

            // Test data_home path gets prepended
            assert!(env.add_path(data_home_path.clone()));
            assert_eq!(env.paths[0], data_home_path);

            // Test adding multiple paths
            assert!(env.add_paths(vec![PathBuf::from("/path1"), PathBuf::from("/path2")]));
            assert_eq!(env.paths.len(), 4);
        });
    }

    #[test]
    fn test_version_management() {
        let mut env = UpEnvironment::new();

        // Test adding version
        assert!(env.add_version(UpVersionParams {
            backend: "backend1",
            tool: "tool1",
            plugin_name: "plugin1",
            normalized_name: "plugin-1",
            version: "1.0.0",
            bin_path: "bin/path/1",
            dirs: BTreeSet::from(["dir1".to_string()]),
            env_vars: Vec::new(),
        }));
        assert_eq!(env.versions.len(), 1);

        // Test adding same version doesn't duplicate
        assert!(!env.add_version(UpVersionParams {
            backend: "backend1",
            tool: "tool1",
            plugin_name: "plugin1",
            normalized_name: "plugin-1",
            version: "1.0.0",
            bin_path: "bin/path/1",
            dirs: BTreeSet::from(["dir1".to_string()]),
            env_vars: Vec::new(),
        }));
        assert_eq!(env.versions.len(), 1);

        // Test adding data path
        assert!(env.add_version_data_path("plugin-1", "1.0.0", "dir1", "/data/path"));
        assert_eq!(env.versions[0].data_path, Some("/data/path".to_string()));
    }
}

mod up_version {
    use super::*;

    #[test]
    fn test_new() {
        let version = UpVersion {
            tool: "tool1".to_string(),
            plugin_name: "plugin1".to_string(),
            normalized_name: "plugin-1".to_string(),
            backend: "backend1".to_string(),
            version: "1.0.0".to_string(),
            bin_path: "bin/path/1".to_string(),
            dir: "dir1".to_string(),
            data_path: None,
            env_vars: Vec::new(),
        };
        assert_eq!(version.tool, "tool1");
        assert_eq!(version.plugin_name, "plugin1");
        assert_eq!(version.normalized_name, "plugin-1");
        assert_eq!(version.backend, "backend1");
        assert_eq!(version.version, "1.0.0");
        assert_eq!(version.bin_path, "bin/path/1");
        assert_eq!(version.dir, "dir1");
        assert!(version.data_path.is_none());
        assert!(version.env_vars.is_empty());
    }
}

mod up_env_var {
    use super::*;

    #[test]
    fn test_from_env_operation_config() {
        let config = EnvOperationConfig {
            name: "TEST_VAR".to_string(),
            value: Some("test_value".to_string()),
            operation: EnvOperationEnum::Set,
        };

        let env_var: UpEnvVar = config.into();
        assert_eq!(env_var.name, "TEST_VAR");
        assert_eq!(env_var.value, Some("test_value".to_string()));
        assert_eq!(env_var.operation, EnvOperationEnum::Set);
    }

    #[test]
    fn test_from_env_config() {
        let config = EnvConfig {
            operations: vec![
                EnvOperationConfig {
                    name: "VAR1".to_string(),
                    value: Some("value1".to_string()),
                    operation: EnvOperationEnum::Set,
                },
                EnvOperationConfig {
                    name: "VAR2".to_string(),
                    value: Some("value2".to_string()),
                    operation: EnvOperationEnum::Append,
                },
            ],
        };

        let env_vars: Vec<UpEnvVar> = config.into();
        assert_eq!(env_vars.len(), 2);
        assert_eq!(env_vars[0].name, "VAR1");
        assert_eq!(env_vars[1].name, "VAR2");
    }
}
