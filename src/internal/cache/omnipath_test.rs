use super::*;

use std::env::temp_dir as env_temp_dir;
use std::fs::write as fs_write;
use std::path::Path;

use uuid::Uuid;

use crate::internal::testutils::run_with_env;

mod omnipath_cache {
    use super::*;

    fn create_fake_log_file() -> String {
        let tempdir = env_temp_dir();
        let uuid = Uuid::new_v4();
        let log_file = tempdir.as_path().join(format!("fake_error_{uuid:x}.log"));
        fs_write(&log_file, "Test error log").expect("Failed to write to log file");
        log_file.to_string_lossy().to_string()
    }

    #[test]
    fn test_try_exclusive_update() {
        run_with_env(&[], || {
            let cache = OmniPathCache::get();

            // First attempt should succeed
            assert!(cache.try_exclusive_update(), "First update should succeed");

            // Verify updated_at was set
            let db = CacheManager::get();
            let (_expired, updated_at): (bool, String) = db
                .query_one(
                    include_str!("database/sql/omnipath_get_updated_at.sql"),
                    params![0],
                )
                .expect("Failed to get updated_at");

            assert!(!updated_at.is_empty(), "updated_at should be set");

            // Immediate second attempt should return false
            assert!(
                !cache.try_exclusive_update(),
                "Second immediate update should fail"
            );
        });
    }

    #[test]
    fn test_try_exclusive_update_invalid_timestamp() {
        run_with_env(&[], || {
            let cache = OmniPathCache::get();
            let db = CacheManager::get();

            // Insert invalid timestamp
            db.execute(
                "INSERT OR REPLACE INTO metadata (key, value) VALUES ('omnipath.updated_at', ?)",
                params!["invalid-timestamp"],
            )
            .expect("Failed to insert invalid timestamp");

            // Should handle invalid timestamp gracefully and allow update
            assert!(
                cache.try_exclusive_update(),
                "Should handle invalid timestamp"
            );
        });
    }

    #[test]
    fn test_null_timestamp() {
        run_with_env(&[], || {
            let cache = OmniPathCache::get();
            let db = CacheManager::get();

            // Insert NULL timestamp
            db.execute(
                "INSERT OR REPLACE INTO metadata (key, value) VALUES ('omnipath.updated_at', NULL)",
                params![],
            )
            .expect("Failed to insert NULL timestamp");

            // Should handle NULL timestamp gracefully
            assert!(cache.try_exclusive_update(), "Should handle NULL timestamp");
        });
    }

    #[test]
    fn test_update_error_log() {
        run_with_env(&[], || {
            let cache = OmniPathCache::get();
            let error_file = create_fake_log_file();

            // Set error log
            assert!(
                cache
                    .update_error(error_file.clone())
                    .expect("Failed to update error log"),
                "Setting error log should succeed"
            );

            // Verify error log was set
            let db = CacheManager::get();
            let stored_error: Option<String> = db
                .query_one(
                    include_str!("database/sql/omnipath_get_update_error_log.sql"),
                    params![],
                )
                .expect("Failed to get error log");

            assert_eq!(
                stored_error.as_deref(),
                Some(error_file.as_str()),
                "Stored error should match set error"
            );
        });
    }

    #[test]
    fn test_try_exclusive_update_error_log() {
        run_with_env(&[], || {
            let cache = OmniPathCache::get();
            let error_file = create_fake_log_file();

            // Initially should return None as no error is set
            assert!(
                cache.try_exclusive_update_error_log().is_none(),
                "Should return None when no error is set"
            );

            // Set error log
            cache
                .update_error(error_file.clone())
                .expect("Failed to update error log");

            // Try to exclusively get and clear error
            let retrieved_error = cache.try_exclusive_update_error_log();
            assert_eq!(
                retrieved_error.as_deref(),
                Some(error_file.as_str()),
                "Retrieved error should match set error"
            );

            // Error should be cleared now
            let db = CacheManager::get();
            let stored_error: Option<String> = db
                .query_one_optional(
                    include_str!("database/sql/omnipath_get_update_error_log.sql"),
                    params![],
                )
                .expect("Failed to get error log");

            assert!(stored_error.is_none(), "Error log should be cleared");

            // Second attempt should return None
            assert!(
                cache.try_exclusive_update_error_log().is_none(),
                "Second attempt should return None as error was cleared"
            );
        });
    }

    #[test]
    fn test_try_exclusive_update_error_log_empty() {
        run_with_env(&[], || {
            let cache = OmniPathCache::get();
            let db = CacheManager::get();

            // Add empty error log
            db.execute(
                "INSERT OR REPLACE INTO metadata (key, value) VALUES ('omnipath.update_error_log', '')",
                params![],
            ).expect("Failed to insert empty error log");

            // Check that the value is stored
            let stored_error: Option<String> = db
                .query_one(
                    include_str!("database/sql/omnipath_get_update_error_log.sql"),
                    params![],
                )
                .expect("Failed to get error log");
            assert_eq!(
                stored_error.as_deref(),
                Some(""),
                "Stored error should be empty"
            );

            // Check that None gets returned
            assert!(
                cache.try_exclusive_update_error_log().is_none(),
                "Should return None when empty error is set"
            );

            // Check that the entry is not stored anymore
            let stored_error: Option<String> = db
                .query_one_optional(
                    include_str!("database/sql/omnipath_get_update_error_log.sql"),
                    params![],
                )
                .expect("Failed to get error log");
            assert!(stored_error.is_none(), "Error log should be cleared");
        });
    }

    #[test]
    fn test_try_exclusive_update_error_log_not_exists() {
        run_with_env(&[], || {
            let cache = OmniPathCache::get();
            let db = CacheManager::get();

            // Make sure we have a file that does not exist, or it will fail
            // the test for the wrong reasons
            let error_file_base = "/this/file/does/not/exist.log";
            let mut error_file = error_file_base.to_string();
            while Path::new(&error_file).exists() {
                error_file = format!("{}.{:x}", error_file_base, Uuid::new_v4());
            }

            // Store the entry in the cache
            assert!(
                cache.update_error(error_file.clone()).is_ok(),
                "Failed to update error log"
            );

            // Check that the value is stored
            let stored_error: Option<String> = db
                .query_one(
                    include_str!("database/sql/omnipath_get_update_error_log.sql"),
                    params![],
                )
                .expect("Failed to get error log");
            assert_eq!(
                stored_error.as_deref(),
                Some(error_file.as_str()),
                "Stored error should match set error"
            );

            // Check that None gets returned
            assert!(
                cache.try_exclusive_update_error_log().is_none(),
                "Should return None when error log does not exist"
            );

            // Check that the entry is not stored anymore
            let stored_error: Option<String> = db
                .query_one_optional(
                    include_str!("database/sql/omnipath_get_update_error_log.sql"),
                    params![],
                )
                .expect("Failed to get error log");
            assert!(stored_error.is_none(), "Error log should be cleared");
        });
    }

    #[test]
    fn test_sequential_error_log_access() {
        run_with_env(&[], || {
            let cache1 = OmniPathCache::get();
            let cache2 = OmniPathCache::get();
            let error_file = create_fake_log_file();

            // Set error using first cache instance
            cache1
                .update_error(error_file.clone())
                .expect("Failed to update error log");

            // First instance gets and clears the error
            let error1 = cache1.try_exclusive_update_error_log();
            assert_eq!(
                error1.as_deref(),
                Some(error_file.as_str()),
                "First instance should get error"
            );

            // Second instance should get None as error was cleared
            let error2 = cache2.try_exclusive_update_error_log();
            assert!(error2.is_none(), "Second instance should get None");
        });
    }

    #[test]
    fn test_update_error_overwrite() {
        run_with_env(&[], || {
            let cache = OmniPathCache::get();

            // Set first error
            let error1 = create_fake_log_file();
            cache
                .update_error(error1.clone())
                .expect("Failed to update first error");

            // Set second error
            let error2 = create_fake_log_file();
            cache
                .update_error(error2.clone())
                .expect("Failed to update second error");

            // Verify only latest error is stored
            let retrieved_error = cache.try_exclusive_update_error_log();
            assert_eq!(
                retrieved_error.as_deref(),
                Some(error2.as_str()),
                "Should only retrieve latest error"
            );
        });
    }
}