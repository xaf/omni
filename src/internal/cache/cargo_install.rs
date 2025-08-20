use rusqlite::params;
use rusqlite::Row;
use serde::Deserialize;
use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::internal::cache::database::FromRow;
use crate::internal::cache::database::RowExt;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;
use crate::internal::config::global_config;
use crate::internal::env::now as omni_now;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CargoInstallOperationCache {}

impl CargoInstallOperationCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn add_versions(
        &self,
        crate_name: &str,
        versions: &CargoInstallVersions,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/cargo_install_operation_add_versions.sql"),
            params![crate_name, serde_json::to_string(&versions.versions)?],
        )?;
        Ok(inserted > 0)
    }

    pub fn get_versions(&self, crate_name: &str) -> Option<CargoInstallVersions> {
        let db = CacheManager::get();
        let versions: Option<CargoInstallVersions> = db
            .query_one(
                include_str!("database/sql/cargo_install_operation_get_versions.sql"),
                params![crate_name],
            )
            .ok();
        versions
    }

    pub fn add_installed(
        &self,
        crate_name: &str,
        version: &str,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/cargo_install_operation_add.sql"),
            params![crate_name, version],
        )?;
        Ok(inserted > 0)
    }

    pub fn add_required_by(
        &self,
        env_version_id: &str,
        crate_name: &str,
        version: &str,
    ) -> Result<bool, CacheManagerError> {
        let db = CacheManager::get();
        let inserted = db.execute(
            include_str!("database/sql/cargo_install_operation_add_required_by.sql"),
            params![crate_name, version, env_version_id],
        )?;
        Ok(inserted > 0)
    }

    pub fn list_installed(&self) -> Result<Vec<CargoInstalled>, CacheManagerError> {
        let db = CacheManager::get();
        let installed: Vec<CargoInstalled> = db.query_as(
            include_str!("database/sql/cargo_install_operation_list_installed.sql"),
            params![],
        )?;
        Ok(installed)
    }

    pub fn cleanup(&self) -> Result<(), CacheManagerError> {
        let config = global_config();
        let db = CacheManager::get();

        db.execute(
            include_str!("database/sql/cargo_install_operation_cleanup_installed.sql"),
            params![&config.cache.cargo_install.cleanup_after],
        )?;

        db.execute(
            include_str!("database/sql/cargo_install_operation_cleanup_versions.sql"),
            params![&config.cache.cargo_install.versions_retention],
        )?;

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CargoInstalled {
    pub crate_name: String,
    pub version: String,
}

impl FromRow for CargoInstalled {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        Ok(Self {
            crate_name: row.get("crate")?,
            version: row.get("version")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CargoInstallVersions {
    #[serde(alias = "Versions")]
    pub versions: Vec<String>,
    #[serde(default = "OffsetDateTime::now_utc", with = "time::serde::rfc3339")]
    pub fetched_at: OffsetDateTime,
}

impl CargoInstallVersions {
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
}

impl FromRow for CargoInstallVersions {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        let versions_str: String = row.get("versions")?;
        let versions: Vec<String> = serde_json::from_str(&versions_str)?;

        let fetched_at_str: String = row.get("fetched_at")?;
        let fetched_at: OffsetDateTime = OffsetDateTime::parse(&fetched_at_str, &Rfc3339)?;

        Ok(Self {
            versions,
            fetched_at,
        })
    }
}

#[cfg(test)]
#[path = "cargo_install_test.rs"]
mod tests;
