use rusqlite::params;
use serde::Deserialize;
use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::internal::cache::database::FromRow;
use crate::internal::cache::database::RowExt;
use crate::internal::cache::utils;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;
use crate::internal::config::global_config;
use crate::internal::config::up::utils::VersionMatcher;
use crate::internal::env::now as omni_now;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MiseOperationCache {}

impl MiseOperationCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn updated_mise(&self) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("database/sql/mise_operation_updated_mise.sql"),
            &[],
        )?;
        Ok(updated > 0)
    }

    pub fn updated_mise_plugin(&self, plugin: &str) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("database/sql/mise_operation_updated_plugin.sql"),
            params![plugin],
        )?;
        Ok(updated > 0)
    }

    pub fn set_mise_plugin_versions(
        &self,
        plugin: &str,
        versions: MisePluginVersions,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let updated = db.execute(
            include_str!("database/sql/mise_operation_updated_plugin_versions.sql"),
            params![plugin, serde_json::to_string(&versions.versions)?],
        )?;
        Ok(updated > 0)
    }

    pub fn should_update_mise(&self) -> bool {
        let db = CacheManager::get();
        let should_update: bool = db
            .query_row(
                include_str!("database/sql/mise_operation_should_update_mise.sql"),
                params![global_config().cache.mise.update_expire],
                |row| row.get(0),
            )
            .unwrap_or(true);
        should_update
    }

    pub fn should_update_mise_plugin(&self, plugin: &str) -> bool {
        let db = CacheManager::get();
        let should_update: bool = db
            .query_row(
                include_str!("database/sql/mise_operation_should_update_plugin.sql"),
                params![plugin, global_config().cache.mise.plugin_update_expire,],
                |row| row.get(0),
            )
            .unwrap_or(true);
        should_update
    }

    pub fn get_mise_plugin_versions(&self, plugin: &str) -> Option<MisePluginVersions> {
        let db = CacheManager::get();
        let versions: Option<MisePluginVersions> = db
            .query_one(
                include_str!("database/sql/mise_operation_get_plugin_versions.sql"),
                params![plugin],
            )
            .unwrap_or_default();
        versions
    }

    pub fn add_installed(
        &self,
        tool: &str,
        plugin_name: &str,
        normalized_name: &str,
        version: &str,
        bin_path: &str,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/mise_operation_add_installed.sql"),
            params![
                tool,
                plugin_name,
                normalized_name,
                version,
                serde_json::to_string(&vec![bin_path])?
            ],
        )?;
        Ok(inserted > 0)
    }

    pub fn add_required_by(
        &self,
        env_version_id: &str,
        normalized_name: &str,
        version: &str,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/mise_operation_add_required_by.sql"),
            params![normalized_name, version, env_version_id],
        )?;
        Ok(inserted > 0)
    }

    pub fn cleanup<F>(&self, mut delete_func: F) -> Result<(), CacheManagerError>
    where
        F: FnMut(&str, &str) -> Result<(), CacheManagerError>,
    {
        let mut db = CacheManager::get();

        let config = global_config();
        let grace_period = config.cache.mise.cleanup_after;

        db.transaction(|tx| {
            // Get the list of tools and versions that can be deleted
            let deletable_tools: Vec<DeletableMiseTool> = tx.query_as(
                include_str!("database/sql/mise_operation_list_removable.sql"),
                params![&grace_period],
            )?;

            for tool in deletable_tools {
                // Do the physical deletion of the tool and version
                delete_func(&tool.tool, &tool.version)?;

                // Add the deletion of that tool and version to the transaction
                tx.execute(
                    include_str!("database/sql/mise_operation_remove.sql"),
                    params![tool.tool, tool.version],
                )?;
            }

            Ok(())
        })?;

        db.execute(
            include_str!("database/sql/mise_operation_cleanup_versions.sql"),
            params![&config.cache.mise.plugin_versions_retention],
        )?;

        Ok(())
    }
}

#[derive(Debug)]
struct DeletableMiseTool {
    tool: String,
    version: String,
}

impl FromRow for DeletableMiseTool {
    fn from_row(row: &rusqlite::Row) -> Result<Self, CacheManagerError> {
        let tool: String = row.get(0)?;
        let version: String = row.get(1)?;
        Ok(Self { tool, version })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MisePluginVersions {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<String>,
    #[serde(
        default = "utils::origin_of_time",
        with = "time::serde::rfc3339",
        skip_serializing_if = "utils::is_origin_of_time"
    )]
    pub fetched_at: OffsetDateTime,
}

impl FromRow for MisePluginVersions {
    fn from_row(row: &rusqlite::Row) -> Result<Self, CacheManagerError> {
        let versions_json: String = row.get(0)?;
        let versions: Vec<String> = serde_json::from_str(&versions_json)?;

        let fetched_at_str: String = row.get(1)?;
        let fetched_at = OffsetDateTime::parse(&fetched_at_str, &Rfc3339)?;

        Ok(Self {
            versions,
            fetched_at,
        })
    }
}

impl MisePluginVersions {
    pub fn new(versions: Vec<String>) -> Self {
        Self {
            versions,
            fetched_at: omni_now(),
        }
    }

    pub fn is_fresh(&self) -> bool {
        self.fetched_at >= omni_now()
    }

    pub fn is_stale(&self, ttl: u64) -> bool {
        let duration = time::Duration::seconds(ttl as i64);
        self.fetched_at + duration < OffsetDateTime::now_utc()
    }

    pub fn get(&self, matcher: &VersionMatcher) -> Option<String> {
        self.versions
            .iter()
            .rev()
            .find(|v| matcher.matches(v))
            .cloned()
    }
}

#[cfg(test)]
#[path = "mise_operation_test.rs"]
mod tests;
