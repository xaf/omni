use super::*;
use crate::internal::cache::database::get_conn;
use crate::internal::testutils::run_with_env;

mod github_release_operation_cache {
    use super::*;
    use time::OffsetDateTime;

    #[test]
    fn test_add_and_get_releases() {
        run_with_env(&[], || {
            let cache = GithubReleaseOperationCache::get();
            let repository = "test/repo";

            // Create test releases
            let releases = GithubReleases {
                releases: vec![GithubReleaseVersion {
                    tag_name: "v1.0.0".to_string(),
                    name: "Release 1.0.0".to_string(),
                    draft: false,
                    prerelease: false,
                    assets: vec![],
                }],
                fetched_at: OffsetDateTime::now_utc(),
            };

            // Test adding releases
            assert!(cache
                .add_releases(repository, &releases)
                .expect("Failed to add releases"));

            // Test retrieving releases
            let retrieved = cache
                .get_releases(repository)
                .expect("Failed to get releases");
            assert_eq!(retrieved.releases.len(), 1);
            assert_eq!(retrieved.releases[0].tag_name, "v1.0.0");

            // Test retrieving non-existent repository
            let non_existent = cache.get_releases("non/existent");
            assert!(non_existent.is_none());
        });
    }

    #[test]
    fn test_add_and_list_installed() {
        run_with_env(&[], || {
            let cache = GithubReleaseOperationCache::get();
            let repository = "test/repo";
            let version = "v1.0.0";

            // Test adding installed version
            assert!(cache
                .add_installed(repository, version)
                .expect("Failed to add installed version"));

            // Test listing installed versions
            let installed = cache.list_installed().expect("Failed to list installed");
            assert_eq!(installed.len(), 1);
            assert_eq!(installed[0].repository, repository);
            assert_eq!(installed[0].version, version);

            // Test adding duplicate installed version
            assert!(cache
                .add_installed(repository, version)
                .expect("Failed to add duplicate installed version"));

            // Verify no duplicates in list
            let installed = cache.list_installed().expect("Failed to list installed");
            assert_eq!(installed.len(), 1);
        });
    }

    #[test]
    fn test_add_required_by() {
        run_with_env(&[], || {
            let cache = GithubReleaseOperationCache::get();
            let repository = "test/repo";
            let version = "v1.0.0";
            let env_version_id = "test-env-id";

            // Add environment version first for foreign key constraint
            let conn = get_conn();
            conn.execute(
                include_str!("database/sql/up_environments_insert_env_version.sql"),
                params![env_version_id, "{}", "[]", "[]", "{}", "hash"],
            )
            .expect("Failed to add environment version");

            // Try adding required_by without installed - should fail
            let result = cache.add_required_by(env_version_id, repository, version);
            assert!(result.is_err(), "Should fail without installed version");

            // Add installed version
            cache
                .add_installed(repository, version)
                .expect("Failed to add installed version");

            // Now add required_by - should succeed
            assert!(cache
                .add_required_by(env_version_id, repository, version)
                .expect("Failed to add required by relationship"));

            // Verify the relationship exists
            let required_exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM github_release_required_by WHERE repository = ?1 AND version = ?2 AND env_version_id = ?3)",
                    params![repository, version, env_version_id],
                    |row| row.get(0),
                )
                .expect("Failed to query required by relationship");
            assert!(required_exists);
        });
    }

    #[test]
    fn test_multiple_required_by() {
        run_with_env(&[], || {
            let cache = GithubReleaseOperationCache::get();
            let repository = "test/repo";
            let version = "v1.0.0";
            let env_version_ids = vec!["env-1", "env-2", "env-3"];

            // Add installed version first
            cache
                .add_installed(repository, version)
                .expect("Failed to add installed version");

            // Add environments
            let conn = get_conn();
            for env_id in &env_version_ids {
                conn.execute(
                    include_str!("database/sql/up_environments_insert_env_version.sql"),
                    params![env_id, "{}", "[]", "[]", "{}", "hash"],
                )
                .expect("Failed to add environment version");
            }

            // Add requirements for each environment
            for env_id in &env_version_ids {
                assert!(cache
                    .add_required_by(env_id, repository, version)
                    .expect("Failed to add requirement"));
            }

            // Verify requirements
            for env_id in &env_version_ids {
                let required: bool = conn
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM github_release_required_by WHERE repository = ?1 AND version = ?2 AND env_version_id = ?3)",
                        params![repository, version, env_id],
                        |row| row.get(0),
                    )
                    .expect("Failed to query requirement");
                assert!(required, "Requirement for {env_id} should exist");
            }

            // Verify installed version still exists
            let installed = cache.list_installed().expect("Failed to list installed");
            assert_eq!(installed.len(), 1);
            assert_eq!(installed[0].repository, repository);
            assert_eq!(installed[0].version, version);
        });
    }

    #[test]
    fn test_cleanup() {
        run_with_env(&[], || {
            let cache = GithubReleaseOperationCache::get();

            // Add two repositories
            let repo1 = "test/repo1";
            let repo2 = "test/repo2";
            let version = "v1.0.0";

            // Add installations
            cache
                .add_installed(repo1, version)
                .expect("Failed to add repo1 installation");
            cache
                .add_installed(repo2, version)
                .expect("Failed to add repo2 installation");

            let conn = get_conn();

            // Set repo1's last_required_at to old date (should be cleaned up)
            conn.execute(
                "UPDATE github_release_installed SET last_required_at = '1970-01-01T00:00:00.000Z' WHERE repository = ?1",
                params![repo1],
            )
            .expect("Failed to update last_required_at for repo1");

            // Keep repo2's last_required_at recent (should not be cleaned up)
            conn.execute(
                "UPDATE github_release_installed SET last_required_at = datetime('now') WHERE repository = ?1",
                params![repo2],
            )
            .expect("Failed to update last_required_at for repo2");

            // Run cleanup
            cache.cleanup().expect("Failed to cleanup");

            // Verify repo1 was cleaned up
            let repo1_exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM github_release_installed WHERE repository = ?1)",
                    params![repo1],
                    |row| row.get(0),
                )
                .expect("Failed to query repo1");
            assert!(
                !repo1_exists,
                "Old installation should have been cleaned up"
            );

            // Verify repo2 still exists
            let repo2_exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM github_release_installed WHERE repository = ?1)",
                    params![repo2],
                    |row| row.get(0),
                )
                .expect("Failed to query repo2");
            assert!(
                repo2_exists,
                "Recent installation should not have been cleaned up"
            );

            // Verify through list_installed
            let installed = cache.list_installed().expect("Failed to list installed");
            assert_eq!(installed.len(), 1);
            assert_eq!(installed[0].repository, repo2);
        });
    }

    #[test]
    fn test_update_releases() {
        run_with_env(&[], || {
            let cache = GithubReleaseOperationCache::get();
            let repository = "test/repo";

            // Create initial releases
            let releases1 = GithubReleases {
                releases: vec![GithubReleaseVersion {
                    tag_name: "v1.0.0".to_string(),
                    name: "Release 1.0.0".to_string(),
                    draft: false,
                    prerelease: false,
                    assets: vec![],
                }],
                fetched_at: OffsetDateTime::now_utc(),
            };

            // Add initial releases
            assert!(cache
                .add_releases(repository, &releases1)
                .expect("Failed to add initial releases"));

            // Create updated releases
            let releases2 = GithubReleases {
                releases: vec![
                    GithubReleaseVersion {
                        tag_name: "v1.0.0".to_string(),
                        name: "Release 1.0.0".to_string(),
                        draft: false,
                        prerelease: false,
                        assets: vec![],
                    },
                    GithubReleaseVersion {
                        tag_name: "v1.1.0".to_string(),
                        name: "Release 1.1.0".to_string(),
                        draft: false,
                        prerelease: false,
                        assets: vec![],
                    },
                ],
                fetched_at: OffsetDateTime::now_utc(),
            };

            // Update releases
            assert!(cache
                .add_releases(repository, &releases2)
                .expect("Failed to update releases"));

            // Verify updated releases
            let retrieved = cache
                .get_releases(repository)
                .expect("Failed to get releases");
            assert_eq!(retrieved.releases.len(), 2);
            assert!(retrieved.releases.iter().any(|r| r.tag_name == "v1.1.0"));
        });
    }

    #[test]
    fn test_multiple_versions_same_repository() {
        run_with_env(&[], || {
            let cache = GithubReleaseOperationCache::get();
            let repository = "test/repo";
            let versions = vec!["v1.0.0", "v1.1.0", "v2.0.0"];

            // Add multiple versions
            for version in &versions {
                assert!(cache
                    .add_installed(repository, version)
                    .expect("Failed to add installed version"));
            }

            // Verify all versions are listed
            let installed = cache.list_installed().expect("Failed to list installed");
            assert_eq!(installed.len(), versions.len());

            for version in versions {
                assert!(
                    installed
                        .iter()
                        .any(|i| i.repository == repository && i.version == version),
                    "Version {version} should be in installed list"
                );
            }
        });
    }

    #[test]
    fn test_required_by_multiple_versions() {
        run_with_env(&[], || {
            let cache = GithubReleaseOperationCache::get();
            let repository = "test/repo";
            let versions = vec!["v1.0.0", "v1.1.0"];
            let env_id = "test-env";

            // Add environment
            let conn = get_conn();
            conn.execute(
                include_str!("database/sql/up_environments_insert_env_version.sql"),
                params![env_id, "{}", "[]", "[]", "{}", "hash"],
            )
            .expect("Failed to add environment version");

            // Add installations and requirements
            for version in &versions {
                // Add installation
                assert!(cache
                    .add_installed(repository, version)
                    .expect("Failed to add installed version"));

                // Add requirement
                assert!(cache
                    .add_required_by(env_id, repository, version)
                    .expect("Failed to add requirement"));
            }

            // Verify all requirements exist
            for version in versions {
                let required: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM github_release_required_by WHERE repository = ?1 AND version = ?2 AND env_version_id = ?3)",
                params![repository, version, env_id],
                |row| row.get(0),
            )
            .expect("Failed to query requirement");
                assert!(required, "Requirement for version {version} should exist");
            }
        });
    }

    #[test]
    fn test_cleanup_cascade() {
        run_with_env(&[], || {
            let cache = GithubReleaseOperationCache::get();

            struct TestCase {
                repository: &'static str,
                version: &'static str,
                env_id: &'static str,
                remove_env: bool,
                remove_install: bool,
            }

            let tests = vec![
                TestCase {
                    repository: "test/repo1",
                    version: "v1.0.0",
                    env_id: "test-env1",
                    remove_env: false,
                    remove_install: true,
                },
                TestCase {
                    repository: "test/repo2",
                    version: "v1.0.0",
                    env_id: "test-env2",
                    remove_env: true,
                    remove_install: false,
                },
            ];

            let conn = get_conn();

            for test in &tests {
                // Add environment
                conn.execute(
                    include_str!("database/sql/up_environments_insert_env_version.sql"),
                    params![test.env_id, "{}", "[]", "[]", "{}", "hash"],
                )
                .expect("Failed to add environment version");

                // Add installation
                cache
                    .add_installed(test.repository, test.version)
                    .unwrap_or_else(|_| {
                        panic!("Failed to add installed version for {}", test.repository)
                    });

                // Add requirement
                cache
                    .add_required_by(test.env_id, test.repository, test.version)
                    .unwrap_or_else(|_| {
                        panic!("Failed to add requirement for {}", test.repository)
                    });

                // Check that the requirement exists
                let required: bool = conn
                    .query_row(
                        concat!(
                            "SELECT EXISTS(",
                            "  SELECT 1 FROM github_release_required_by ",
                            "  WHERE repository = ?1 AND version = ?2 AND env_version_id = ?3",
                            ")",
                        ),
                        params![test.repository, test.version, test.env_id],
                        |row| row.get(0),
                    )
                    .expect("Failed to query requirement");
                assert!(required, "Requirement for {} should exist", test.repository);

                if test.remove_env {
                    // Remove environment
                    conn.execute(
                        "DELETE FROM env_versions WHERE env_version_id = ?1",
                        params![test.env_id],
                    )
                    .expect("Failed to remove environment");
                }

                if test.remove_install {
                    // Remove installation
                    conn.execute(
                        "DELETE FROM github_release_installed WHERE repository = ?1 AND version = ?2",
                        params![test.repository, test.version],
                    ).expect("Failed to remove installation");
                }

                // Verify that the requirement has been cleaned up
                let requirement_exists: bool = conn
                    .query_row(
                        concat!(
                            "SELECT EXISTS(",
                            "  SELECT 1 FROM github_release_required_by ",
                            "  WHERE repository = ?1 AND version = ?2 AND env_version_id = ?3",
                            ")",
                        ),
                        params![test.repository, test.version, test.env_id],
                        |row| row.get(0),
                    )
                    .expect("Failed to query requirement");
                assert!(
                    !requirement_exists,
                    "Requirement should be cleaned up via cascade"
                );
            }
        });
    }
}
