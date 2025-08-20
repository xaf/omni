use super::*;
use crate::internal::cache::database::get_conn;
use crate::internal::testutils::run_with_env;

mod homebrew_operation_cache {
    use super::*;

    #[test]
    fn test_add_tap_and_required_by() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let tap_name = "test/tap";
            let env_version_id = "test-env-id";

            // Test adding tap
            assert!(cache.add_tap(tap_name, true).expect("Failed to add tap"));

            // Before adding a required_by, we need to add the environment version
            // otherwise the foreign key constraint will fail
            let conn = get_conn();
            conn.execute(
                include_str!("database/sql/up_environments_insert_env_version.sql"),
                params![env_version_id, "{}", "[]", "[]", "{}", "hash"],
            )
            .expect("Failed to add environment version");

            // Test adding tap required by environment
            assert!(cache
                .add_tap_required_by(env_version_id, tap_name)
                .expect("Failed to add tap requirement"));
        });
    }

    #[test]
    fn test_tap_update() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let tap_name = "test/tap";

            // Initially should update
            assert!(cache.should_update_tap(tap_name));

            // Update tap
            assert!(cache
                .updated_tap(tap_name)
                .expect("Failed to update tap timestamp"));

            // Should not update immediately after
            assert!(!cache.should_update_tap(tap_name));
        });
    }

    #[test]
    fn test_add_install_and_required_by() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let install_name = "test-formula";
            let env_version_id = "test-env-id";

            // Before adding a required_by, we need to add the environment version
            // otherwise the foreign key constraint will fail
            let conn = get_conn();
            conn.execute(
                include_str!("database/sql/up_environments_insert_env_version.sql"),
                params![env_version_id, "{}", "[]", "[]", "{}", "hash"],
            )
            .expect("Failed to add environment version");

            for is_cask in &[false, true] {
                for version in &[Some("1.0.0".to_string()), None] {
                    // Test adding install
                    assert!(cache
                        .add_install(install_name, version.clone(), *is_cask, true)
                        .expect("Failed to add install"));

                    // Test adding install required by environment
                    assert!(cache
                        .add_install_required_by(
                            env_version_id,
                            install_name,
                            version.clone(),
                            *is_cask
                        )
                        .expect("Failed to add install requirement"));
                }
            }
        });
    }

    #[test]
    fn test_install_update() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let install_name = "test-formula";

            for is_cask in &[false, true] {
                for version in &[Some("1.0.0".to_string()), None] {
                    // Initially should update
                    assert!(cache.should_update_install(
                        install_name,
                        version.clone(),
                        *is_cask
                    ));

                    // Update install
                    assert!(cache
                        .updated_install(install_name, version.clone(), *is_cask)
                        .expect("Failed to update install timestamp"));

                    // Should not update immediately after
                    assert!(!cache.should_update_install(
                        install_name,
                        version.clone(),
                        *is_cask
                    ));
                }
            }
        });
    }

    #[test]
    fn test_install_check() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let install_name = "test-formula";

            for is_cask in &[false, true] {
                for version in &[Some("1.0.0".to_string()), None] {
                    // Initially should check
                    assert!(cache.should_check_install(
                        install_name,
                        version.clone(),
                        *is_cask
                    ));

                    // Check install
                    assert!(cache
                        .checked_install(install_name, version.clone(), *is_cask)
                        .expect("Failed to check install"));

                    // Should not check immediately after
                    assert!(!cache.should_check_install(
                        install_name,
                        version.clone(),
                        *is_cask
                    ));
                }
            }
        });
    }

    #[test]
    fn test_homebrew_bin_path() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let test_path = "/usr/local/bin/brew".to_string();

            // Initially no bin path
            assert!(cache.homebrew_bin_path().is_none());

            // Set bin path
            assert!(cache
                .set_homebrew_bin_path(test_path.clone())
                .expect("Failed to set homebrew bin path"));

            // Verify bin path
            assert_eq!(cache.homebrew_bin_path(), Some(test_path));
        });
    }

    #[test]
    fn test_homebrew_update_operations() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();

            // Initially should update
            assert!(cache.should_update_homebrew());

            // Update homebrew
            assert!(cache
                .updated_homebrew()
                .expect("Failed to update homebrew timestamp"));

            // Should not update immediately after
            assert!(!cache.should_update_homebrew());
        });
    }

    #[test]
    fn test_update_install_status() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let install_name = "test-formula";

            for is_cask in &[false, true] {
                for version in &[Some("1.0.0".to_string()), None] {
                    // Add install as not installed
                    assert!(cache
                        .add_install(install_name, version.clone(), *is_cask, false)
                        .expect("Failed to add install"));

                    // Update to installed
                    assert!(cache
                        .add_install(install_name, version.clone(), *is_cask, true)
                        .expect("Failed to update install status"));

                    // Verify status
                    let conn = get_conn();
                    let installed: bool = conn
                        .query_row(
                            concat!(
                                "SELECT installed FROM homebrew_install ",
                                "WHERE name = ?1 AND version = COALESCE(?2, '__NULL__') AND cask = ?3"
                            ),
                            params![install_name, version.clone(), *is_cask],
                            |row| row.get(0),
                        )
                        .expect("Failed to query install status");
                    assert!(installed);
                }
            }
        });
    }

    #[test]
    fn test_update_tap_status() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let tap_name = "test/tap";

            // Add tap as not tapped
            assert!(cache.add_tap(tap_name, false).expect("Failed to add tap"));

            // Update to tapped
            assert!(cache
                .add_tap(tap_name, true)
                .expect("Failed to update tap status"));

            // Verify status
            let conn = get_conn();
            let tapped: bool = conn
                .query_row(
                    "SELECT tapped FROM homebrew_tap WHERE name = ?1",
                    params![tap_name],
                    |row| row.get(0),
                )
                .expect("Failed to query tap status");
            assert!(tapped);
        });
    }

    #[test]
    fn test_install_bin_paths() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let install_name = "test-formula";
            let bin_paths = vec!["/usr/local/bin/test".to_string()];

            for is_cask in &[false, true] {
                for version in &[Some("1.0.0".to_string()), None] {
                    // Initially no bin paths
                    assert!(cache
                        .homebrew_install_bin_paths(install_name, version.clone(), *is_cask)
                        .is_none());

                    // Set bin paths
                    assert!(cache
                        .set_homebrew_install_bin_paths(
                            install_name,
                            version.clone(),
                            *is_cask,
                            bin_paths.clone()
                        )
                        .expect("Failed to set install bin paths"));

                    // Verify bin paths
                    assert_eq!(
                        cache.homebrew_install_bin_paths(
                            install_name,
                            version.clone(),
                            *is_cask
                        ),
                        Some(bin_paths.clone())
                    );
                }
            }
        });
    }

    #[test]
    fn test_empty_bin_paths() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let install_name = "test-formula";
            let empty_paths: Vec<String> = vec![];

            // Set empty bin paths
            assert!(cache
                .set_homebrew_install_bin_paths(install_name, None, false, empty_paths.clone())
                .expect("Failed to set empty install bin paths"));

            // Verify empty bin paths
            assert_eq!(
                cache.homebrew_install_bin_paths(install_name, None, false),
                Some(empty_paths)
            );
        });
    }

    #[test]
    fn test_multiple_env_requirements() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let tap_name = "test/tap";
            let install_name = "test-formula";
            let env_version_ids = vec!["env-1", "env-2", "env-3"];

            // Add environments
            let conn = get_conn();
            for env_id in &env_version_ids {
                conn.execute(
                    include_str!("database/sql/up_environments_insert_env_version.sql"),
                    params![env_id, "{}", "[]", "[]", "{}", "hash"],
                )
                .expect("Failed to add environment version");
            }

            // Add tap and install
            cache.add_tap(tap_name, true).expect("Failed to add tap");
            cache
                .add_install(install_name, None, false, true)
                .expect("Failed to add install");

            // Add requirements for each environment
            for env_id in &env_version_ids {
                assert!(cache
                    .add_tap_required_by(env_id, tap_name)
                    .expect("Failed to add tap requirement"));
                assert!(cache
                    .add_install_required_by(env_id, install_name, None, false)
                    .expect("Failed to add install requirement"));
            }

            // Verify requirements
            for env_id in &env_version_ids {
                let tap_required: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM homebrew_tap_required_by WHERE env_version_id = ?1 AND name = ?2",
                params![env_id, tap_name],
                |row| row.get(0),
            )
            .expect("Failed to query tap requirement");
                assert_eq!(tap_required, 1);

                let install_required: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM homebrew_install_required_by WHERE env_version_id = ?1 AND name = ?2",
                params![env_id, install_name],
                |row| row.get(0),
            )
            .expect("Failed to query install requirement");
                assert_eq!(install_required, 1);
            }
        });
    }

    #[test]
    fn test_cleanup() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();

            // Add some test data
            cache
                .add_install("formula1", None, false, true)
                .expect("Failed to add formula1");
            cache
                .add_install("formula2", None, false, false)
                .expect("Failed to add formula2");
            cache
                .add_install("formula3", None, true, true)
                .expect("Failed to add formula3");
            cache
                .add_install("formula4", None, true, true)
                .expect("Failed to add formula4");

            cache.add_tap("tap1", true).expect("Failed to add tap1");
            cache.add_tap("tap2", false).expect("Failed to add tap2");
            cache.add_tap("tap3", true).expect("Failed to add tap3");
            cache.add_tap("tap4", false).expect("Failed to add tap4");

            // Override the 'last_required_at' timestamp for formula1 and formula2
            // to simulate that they haven't been required in a while, and do the same
            // for tap1 and tap2
            let conn = get_conn();
            conn.execute(
                concat!(
                    "UPDATE homebrew_install ",
                    "SET last_required_at = '1970-01-01T00:00:00.000Z' ",
                    "WHERE name IN ('formula1', 'formula2')",
                ),
                [],
            )
            .expect("Failed to update homebrew_install last_required_at");
            conn.execute(
                concat!(
                    "UPDATE homebrew_tap ",
                    "SET last_required_at = '1970-01-01T00:00:00.000Z' ",
                    "WHERE name IN ('tap1', 'tap2')",
                ),
                [],
            )
            .expect("Failed to update homebrew_tap last_required_at");

            // Create tracking variables for our mock delete functions
            let deleted_installs = std::cell::RefCell::new(Vec::new());
            let deleted_taps = std::cell::RefCell::new(Vec::new());

            // Run cleanup
            cache
                .cleanup(
                    |name, version, is_cask, _progress| {
                        deleted_installs.borrow_mut().push((
                            name.to_string(),
                            version.map(String::from),
                            is_cask,
                        ));
                        Ok(())
                    },
                    |name, _progress| {
                        deleted_taps.borrow_mut().push(name.to_string());
                        Ok(())
                    },
                )
                .expect("Failed to cleanup");

            // Verify deleted items
            assert_eq!(deleted_installs.borrow().len(), 1); // Only installed formula should be physically deleted
            assert_eq!(deleted_taps.borrow().len(), 1); // Only tapped tap should be physically deleted
        });
    }

    #[test]
    fn test_cleanup_with_versions() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let install_name = "test-formula";

            // Add versioned installs
            let mut expected_delete = std::collections::HashSet::new();
            for is_cask in &[false, true] {
                for version in &[Some("1.0.0".to_string()), Some("2.0.0".to_string()), None] {
                    cache
                        .add_install(install_name, version.clone(), *is_cask, true)
                        .expect("Failed to add install");

                    expected_delete.insert((
                        install_name.to_string(),
                        version.clone(),
                        *is_cask,
                    ));
                }
            }

            // Override last_required_at
            let conn = get_conn();
            conn.execute(
                "UPDATE homebrew_install SET last_required_at = '1970-01-01T00:00:00.000Z'",
                [],
            )
            .expect("Failed to update last_required_at");

            // Track deleted versions
            let deleted_versions = std::cell::RefCell::new(Vec::new());

            // Run cleanup
            cache
                .cleanup(
                    |name, version, is_cask, _progress| {
                        deleted_versions.borrow_mut().push((
                            name.to_string(),
                            version.map(String::from),
                            is_cask,
                        ));
                        Ok(())
                    },
                    |_name, _progress| Ok(()),
                )
                .expect("Failed to cleanup");

            // Verify all versions were deleted
            assert_eq!(deleted_versions.borrow().len(), expected_delete.len());
            while let Some((name, version, is_cask)) = deleted_versions.borrow_mut().pop() {
                assert!(
                    expected_delete.remove(&(name.clone(), version.clone(), is_cask)),
                    "Unexpected deletion of name={name}, version={version:?}, cask={is_cask}",
                );
            }

            if !expected_delete.is_empty() {
                let (name, version, is_cask) = expected_delete.iter().next().unwrap();
                panic!("Expected deletion of name={name}, version={version:?}, cask={is_cask}");
            }
        });
    }

    #[test]
    fn test_cleanup_with_failing_delete_install() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();

            // Add test data
            cache
                .add_install("formula1", None, false, true)
                .expect("Failed to add formula");

            // Override the 'last_required_at' timestamp for formula1
            let conn = get_conn();
            conn.execute(
                concat!(
                    "UPDATE homebrew_install ",
                    "SET last_required_at = '1970-01-01T00:00:00.000Z' ",
                    "WHERE name = 'formula1'",
                ),
                [],
            )
            .expect("Failed to update homebrew_install last_required_at");

            // Run cleanup with failing delete functions
            let result = cache.cleanup(
                |_name, _version, _is_cask, _progress| {
                    Err(CacheManagerError::Other(
                        "Failed to delete formula".to_string(),
                    ))
                },
                |_name, _progress| Ok(()),
            );

            // Verify cleanup failed
            assert!(result.is_err(), "Cleanup should have failed");

            // Verify data still exists (transaction rolled back)
            let exists = conn
                .query_row(
                    "SELECT COUNT(*) FROM homebrew_install WHERE name = 'formula1'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("Failed to query homebrew_install");
            assert_eq!(exists, 1);
        });
    }

    #[test]
    fn test_cleanup_with_failing_delete_tap() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();

            // Add test data
            cache.add_tap("tap1", true).expect("Failed to add tap1");

            // Override the 'last_required_at' timestamp for tap1
            let conn = get_conn();
            conn.execute(
                concat!(
                    "UPDATE homebrew_tap ",
                    "SET last_required_at = '1970-01-01T00:00:00.000Z' ",
                    "WHERE name = 'tap1'",
                ),
                [],
            )
            .expect("Failed to update homebrew_tap last_required_at");

            // Run cleanup with failing delete functions
            let result = cache.cleanup(
                |_name, _version, _is_cask, _progress| Ok(()),
                |_name, _progress| {
                    Err(CacheManagerError::Other("Failed to delete tap".to_string()))
                },
            );

            // Verify cleanup failed
            assert!(result.is_err(), "Cleanup should have failed");

            // Verify data still exists (transaction rolled back)
            let exists = conn
                .query_row(
                    "SELECT COUNT(*) FROM homebrew_tap WHERE name = 'tap1'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("Failed to query homebrew_tap");
            assert_eq!(exists, 1);
        });
    }

    #[test]
    fn test_add_duplicate_tap() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let tap_name = "test/tap";

            // Add tap first time
            assert!(cache.add_tap(tap_name, true).expect("Failed to add tap"));

            // Add same tap again
            assert!(cache
                .add_tap(tap_name, true)
                .expect("Failed to add tap again"));

            // Check the tap is still there only once
            let conn = get_conn();
            let exists = conn
                .query_row(
                    "SELECT COUNT(*) FROM homebrew_tap WHERE name = ?1",
                    params![tap_name],
                    |row| row.get::<_, i64>(0),
                )
                .expect("Failed to query homebrew_tap");
            assert_eq!(exists, 1);
        });
    }

    #[test]
    fn test_add_duplicate_install() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let install_name = "test-formula";

            for is_cask in &[false, true] {
                for version in &[Some("1.0.0".to_string()), None] {
                    // Add install first time
                    assert!(cache
                        .add_install(install_name, version.clone(), *is_cask, true)
                        .expect("Failed to add install"));

                    // Add same install again
                    assert!(cache
                        .add_install(install_name, version.clone(), *is_cask, true)
                        .expect("Failed to add install again"));

                    // Check the install is still there only once
                    let conn = get_conn();
                    let exists = conn
                        .query_row(
                            concat!(
                                "SELECT COUNT(*) FROM homebrew_install ",
                                "WHERE name = ?1 AND version = ?2 AND cask = ?3",
                            ),
                            params![
                                install_name,
                                version.clone().unwrap_or("__NULL__".to_string()),
                                *is_cask
                            ],
                            |row| row.get::<_, i64>(0),
                        )
                        .expect("Failed to query homebrew_install");
                    assert_eq!(exists, 1, "Install name={install_name:?}, version={version:?}, cask={is_cask} should exist exactly once, found {exists}");
                }
            }
        });
    }

    #[test]
    fn test_install_with_different_versions() {
        run_with_env(&[], || {
            let cache = HomebrewOperationCache::get();
            let install_name = "test-formula";
            let is_cask = false;

            // Add install with version 1.0.0
            assert!(cache
                .add_install(install_name, Some("1.0.0".to_string()), is_cask, true)
                .expect("Failed to add install 1.0.0"));

            // Add same install with version 2.0.0
            assert!(cache
                .add_install(install_name, Some("2.0.0".to_string()), is_cask, true)
                .expect("Failed to add install 2.0.0"));

            // Add same install with no version
            assert!(cache
                .add_install(install_name, None, is_cask, true)
                .expect("Failed to add install without version"));

            // Check all versions exist
            let conn = get_conn();
            for version in &[Some("1.0.0".to_string()), Some("2.0.0".to_string()), None] {
                let exists = conn
                    .query_row(
                        concat!(
                            "SELECT COUNT(*) FROM homebrew_install ",
                            "WHERE version = ?2 AND name = ?1",
                        ),
                        params![
                            install_name,
                            version.clone().unwrap_or("__NULL__".to_string())
                        ],
                        |row| row.get::<_, i64>(0),
                    )
                    .expect("Failed to query homebrew_install");
                assert_eq!(exists, 1, "Version {version:?} not found");
            }
        });
    }
}