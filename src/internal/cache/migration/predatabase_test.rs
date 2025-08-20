use super::*;

use std::fs;

use crate::internal::testutils::run_with_env;

fn create_json_file(filename: &str, content: &str) {
    let cache_dir_path = PathBuf::from(global_config().cache.path.clone());
    fs::create_dir_all(&cache_dir_path).expect("Failed to create cache directory");
    fs::write(cache_dir_path.join(filename), content).expect("Failed to write test file");
}

fn create_tables(conn: &Connection) {
    conn.execute_batch(include_str!("../database/sql/create_tables.sql"))
        .expect("Failed to create tables");
}

fn get_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("Failed to open in-memory database");
    create_tables(&conn);
    conn
}

mod migrate_up_environments {
    use super::*;

    #[test]
    fn test_basic_migration() {
        run_with_env(&[], || {
            let test_data = r#"{
                "workdir_env": {
                    "work1": "env1"
                },
                "versioned_env": {
                    "env1": {
                        "versions": [{"name": "test"}],
                        "paths": ["/test/path"],
                        "env_vars": [{"name": "TEST", "value": "value"}],
                        "config_modtimes": {"file1": 123},
                        "config_hash": "abc123",
                        "last_assigned_at": "2024-01-01T00:00:00Z"
                    }
                },
                "history": [
                    {
                        "wd": "work1",
                        "sha": "abc123",
                        "env": "env1",
                        "from": "2024-01-01T00:00:00Z",
                        "until": "2024-01-02T00:00:00Z"
                    }
                ],
                "updated_at": null
            }"#;
            create_json_file("up_environments.json", test_data);

            let conn = get_conn();
            migrate_up_environments(&conn).expect("Migration failed");

            // Verify env_versions table
            let env = conn
                .query_row(
                    "SELECT versions, paths, env_vars, config_hash FROM env_versions WHERE env_version_id = ?",
                    params!["env1"],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )
                .unwrap();

            assert!(env.0.contains("test")); // versions
            assert!(env.1.contains("/test/path")); // paths
            assert!(env.2.contains("TEST")); // env_vars
            assert_eq!(env.3, "abc123"); // config_hash

            // Verify workdir_env table
            let env_id: String = conn
                .query_row(
                    "SELECT env_version_id FROM workdir_env WHERE workdir_id = ?",
                    params!["work1"],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(env_id, "env1");

            // Verify env_history table
            let history = conn
                .query_row(
                    "SELECT workdir_id, head_sha, env_version_id, used_from_date, used_until_date FROM env_history",
                    params![],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
                .unwrap();

            assert_eq!(history.0, "work1");
            assert_eq!(history.1, "abc123");
            assert_eq!(history.2, "env1");
            assert_eq!(history.3, "2024-01-01T00:00:00Z");
            assert_eq!(history.4, "2024-01-02T00:00:00Z");
        });
    }

    #[test]
    fn test_invalid_file_migration() {
        run_with_env(&[], || {
            create_json_file("up_environments.json", "invalid json");

            let conn = get_conn();
            let result = migrate_up_environments(&conn);
            assert!(result.is_err(), "Invalid JSON should cause error");
        });
    }

    #[test]
    fn test_empty_file_migration() {
        run_with_env(&[], || {
            create_json_file("up_environments.json", "");

            let conn = get_conn();
            let result = migrate_up_environments(&conn);
            assert!(result.is_ok(), "Empty file should not cause error");
        });
    }

    #[test]
    fn test_no_file_migration() {
        run_with_env(&[], || {
            let conn = get_conn();
            let result = migrate_up_environments(&conn);
            assert!(result.is_ok(), "No file should not cause error");
        });
    }
}

mod migrate_omnipath {
    use super::*;

    #[test]
    fn test_basic_migration() {
        run_with_env(&[], || {
            let test_data = r#"{
                "updated_at": "2024-01-01T00:00:00Z",
                "update_error_log": "test error"
            }"#;
            create_json_file("omnipath.json", test_data);

            let conn = get_conn();
            migrate_omnipath(&conn).expect("Migration failed");

            // Verify metadata entries
            let updated_at: String = conn
                .query_row(
                    "SELECT value FROM metadata WHERE key = ?",
                    params!["omnipath.updated_at"],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(updated_at, "2024-01-01T00:00:00Z");

            let error_log: String = conn
                .query_row(
                    "SELECT value FROM metadata WHERE key = ?",
                    params!["omnipath.update_error_log"],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(error_log, "test error");
        });
    }

    #[test]
    fn test_invalid_file_migration() {
        run_with_env(&[], || {
            create_json_file("omnipath.json", "invalid json");

            let conn = get_conn();
            let result = migrate_omnipath(&conn);
            assert!(result.is_err(), "Invalid JSON should cause error");
        });
    }

    #[test]
    fn test_empty_file_migration() {
        run_with_env(&[], || {
            create_json_file("omnipath.json", "");

            let conn = get_conn();
            let result = migrate_omnipath(&conn);
            assert!(result.is_ok(), "Empty file should not cause error");
        });
    }

    #[test]
    fn test_no_file_migration() {
        run_with_env(&[], || {
            let conn = get_conn();
            let result = migrate_omnipath(&conn);
            assert!(result.is_ok(), "No file should not cause error");
        });
    }
}

mod migrate_repositories {
    use super::*;

    #[test]
    fn test_basic_migration() {
        run_with_env(&[], || {
            let test_data = r#"{
                "trusted": ["repo1", "repo2"],
                "fingerprints": {
                    "repo1": {
                        "git": {"commit": "abc123"},
                        "hash": {"sha256": "def456"}
                    }
                }
            }"#;
            create_json_file("repositories.json", test_data);

            let conn = get_conn();
            migrate_repositories(&conn).expect("Migration failed");

            // Verify trusted repositories
            let count: i32 = conn
                .query_row(
                    "SELECT COUNT(*) FROM workdir_trusted WHERE workdir_id IN ('repo1', 'repo2')",
                    params![],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 2);

            // Verify fingerprints
            let fingerprints: Vec<(String, String, String)> = conn
                .prepare("SELECT workdir_id, fingerprint_type, fingerprint FROM workdir_fingerprints")
                .unwrap()
                .query_map(params![], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();

            assert_eq!(fingerprints.len(), 2);
            assert!(fingerprints.iter().any(|f| f.0 == "repo1" && f.1 == "git"));
            assert!(fingerprints.iter().any(|f| f.0 == "repo1" && f.1 == "hash"));
        });
    }

    #[test]
    fn test_invalid_file_migration() {
        run_with_env(&[], || {
            create_json_file("repositories.json", "invalid json");

            let conn = get_conn();
            let result = migrate_repositories(&conn);
            assert!(result.is_err(), "Invalid JSON should cause error");
        });
    }

    #[test]
    fn test_empty_file_migration() {
        run_with_env(&[], || {
            create_json_file("repositories.json", "");

            let conn = get_conn();
            let result = migrate_repositories(&conn);
            assert!(result.is_ok(), "Empty file should not cause error");
        });
    }

    #[test]
    fn test_no_file_migration() {
        run_with_env(&[], || {
            let conn = get_conn();
            let result = migrate_repositories(&conn);
            assert!(result.is_ok(), "No file should not cause error");
        });
    }
}

mod migrate_prompts {
    use super::*;

    #[test]
    fn test_basic_migration() {
        run_with_env(&[], || {
            let test_data = r#"{
                "answers": [
                    {
                        "id": "prompt1",
                        "org": "testorg",
                        "repo": "testrepo",
                        "answer": {"key": "value"}
                    }
                ]
            }"#;
            create_json_file("prompts.json", test_data);

            let conn = get_conn();
            migrate_prompts(&conn).expect("Migration failed");

            let result = conn
                .query_row(
                    "SELECT prompt_id, organization, repository, answer FROM prompts",
                    params![],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )
                .unwrap();

            assert_eq!(result.0, "prompt1");
            assert_eq!(result.1, "testorg");
            assert_eq!(result.2, "testrepo");
            assert!(result.3.contains("key"));
            assert!(result.3.contains("value"));
        });
    }

    #[test]
    fn test_invalid_file_migration() {
        run_with_env(&[], || {
            create_json_file("prompts.json", "invalid json");

            let conn = get_conn();
            let result = migrate_prompts(&conn);
            assert!(result.is_err(), "Invalid JSON should cause error");
        });
    }

    #[test]
    fn test_empty_file_migration() {
        run_with_env(&[], || {
            create_json_file("prompts.json", "");

            let conn = get_conn();
            let result = migrate_prompts(&conn);
            assert!(result.is_ok(), "Empty file should not cause error");
        });
    }

    #[test]
    fn test_no_file_migration() {
        run_with_env(&[], || {
            let conn = get_conn();
            let result = migrate_prompts(&conn);
            assert!(result.is_ok(), "No file should not cause error");
        });
    }
}

// Helper functions for date handling tests
mod date_handling {
    use super::*;

    #[test]
    fn test_handle_date_string() {
        assert_eq!(
            handle_date_string("2024-01-01T00:00:00Z"),
            "2024-01-01T00:00:00Z"
        );
        assert_eq!(handle_date_string(""), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn test_handle_optional_date_string() {
        assert_eq!(
            handle_optional_date_string(&Some("2024-01-01T00:00:00Z".to_string())),
            Some("2024-01-01T00:00:00Z".to_string())
        );
        assert_eq!(handle_optional_date_string(&Some("".to_string())), None);
        assert_eq!(handle_optional_date_string(&None::<String>), None);
    }
}