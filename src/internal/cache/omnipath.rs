use std::path::PathBuf;

use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::database::RowExt;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;
use crate::internal::config::global_config;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OmniPathCache {}

impl OmniPathCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn try_exclusive_update(&self) -> bool {
        let mut db = CacheManager::get();
        db.transaction(|tx| {
            // Read the current updated_at timestamp
            let updated_at: Option<(bool, Option<String>)> = tx.query_one_optional(
                include_str!("database/sql/omnipath_get_updated_at.sql"),
                params![global_config().path_repo_updates.interval],
            )?;

            let updated_at = match updated_at {
                Some((true, updated_at)) => updated_at,
                Some((false, _)) => {
                    return Err(CacheManagerError::Other("update not required".to_string()))
                }
                None => None,
            };

            // Update the updated_at timestamp
            let updated = tx.execute(
                include_str!("database/sql/omnipath_set_updated_at.sql"),
                params![updated_at],
            )?;

            Ok(updated > 0)
        })
        .unwrap_or_default()
    }

    pub fn update_error(&self, update_error_log: String) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("database/sql/omnipath_set_update_error_log.sql"),
            params![update_error_log],
        )?;
        Ok(updated > 0)
    }

    pub fn try_exclusive_update_error_log(&self) -> Option<String> {
        let mut db = CacheManager::get();
        db.transaction(|tx| {
            // Read the current update_error_log
            let update_error_log: Option<String> = tx.query_one_optional(
                include_str!("database/sql/omnipath_get_update_error_log.sql"),
                params![],
            )?;

            let update_error_log = match update_error_log {
                Some(update_error_log) => update_error_log,
                None => {
                    return Err(CacheManagerError::Other(
                        "update_error_log not found".to_string(),
                    ))
                }
            };

            // Clear the update_error_log
            let deleted = tx.execute(
                include_str!("database/sql/omnipath_clear_update_error_log.sql"),
                params![],
            )?;

            if deleted == 0 {
                return Err(CacheManagerError::Other(
                    "could not delete update_error_log".to_string(),
                ));
            }

            // Check if the file is not empty
            let update_error_log = update_error_log.trim();
            if update_error_log.is_empty() {
                // We return 'Ok' because we want the transaction to commit,
                // since we've already cleared the update_error_log
                return Ok(None);
            }

            // Make sure the file exists before returning it
            let file_path = PathBuf::from(&update_error_log);
            if !file_path.exists() {
                // We return 'Ok' because we want the transaction to commit,
                // since we've already cleared the update_error_log
                return Ok(None);
            }

            // If we get here, we can return the update_error_log
            // since it is an actual file
            Ok(Some(update_error_log.to_string()))
        })
        .unwrap_or_default()
    }
}

#[cfg(test)]
#[path = "omnipath_test.rs"]
mod tests;
