use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::PathBuf;

use rusqlite::params;
use rusqlite::Row;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::config;
use crate::internal::config::global_config;
use crate::internal::config::parser::EnvConfig;
use crate::internal::config::parser::EnvOperationConfig;
use crate::internal::config::parser::EnvOperationEnum;
use crate::internal::config::up::utils::get_config_mod_times;
use crate::internal::env::data_home;

use crate::internal::cache::database::FromRow;
use crate::internal::cache::database::RowExt;
use crate::internal::cache::CacheManager;
use crate::internal::cache::CacheManagerError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpEnvironmentsCache {}

impl UpEnvironmentsCache {
    pub fn get() -> Self {
        Self {}
    }

    pub fn get_env(&self, workdir_id: &str) -> Option<UpEnvironment> {
        let env: UpEnvironment = CacheManager::get()
            .query_one(
                include_str!("database/sql/up_environments_get_workdir_env.sql"),
                &[&workdir_id],
            )
            .ok()?;
        Some(env)
    }

    pub fn clear(&self, workdir_id: &str) -> Result<bool, CacheManagerError> {
        let mut cleared = false;

        let mut db = CacheManager::get();
        db.transaction(|tx| {
            // Close the history entry for the workdir
            tx.execute(
                include_str!("database/sql/up_environments_close_workdir_history.sql"),
                params![&workdir_id],
            )?;

            // Clear the environment for the workdir
            tx.execute(
                include_str!("database/sql/up_environments_clear_workdir_env.sql"),
                params![&workdir_id],
            )?;

            // Check if the row was cleared
            cleared = tx.changes() == 1;

            Ok(())
        })?;

        Ok(cleared)
    }

    pub fn assign_environment(
        &self,
        workdir_id: &str,
        head_sha: Option<String>,
        environment: &mut UpEnvironment,
    ) -> Result<(bool, bool, String), CacheManagerError> {
        let mut new_env: bool = true;
        let mut replace_env: bool = true;
        let env_hash = environment.hash_string();
        let env_version_id = format!("{workdir_id}%{env_hash}");
        let cache_env_config = global_config().cache.environment;

        let mut db = CacheManager::get();
        db.transaction(|tx| {
            // Check if the environment with the given id already exists
            new_env = match tx.query_one::<bool>(
                include_str!("database/sql/up_environments_check_env_version_exists.sql"),
                params![&env_version_id],
            ) {
                Ok(found) => !found,
                Err(CacheManagerError::SqlError(rusqlite::Error::QueryReturnedNoRows)) => true,
                Err(err) => return Err(err),
            };

            if new_env {
                // Insert the environment version
                tx.execute(
                    include_str!("database/sql/up_environments_insert_env_version.sql"),
                    params![
                        &env_version_id,
                        serde_json::to_string(&environment.versions)?,
                        serde_json::to_string(&environment.paths)?,
                        serde_json::to_string(&environment.env_vars)?,
                        serde_json::to_string(&environment.config_modtimes)?,
                        environment.config_hash,
                    ],
                )?;
            }

            // Check if this is a new active environment for the work directory
            replace_env = match tx.query_one::<String>(
                include_str!("database/sql/up_environments_get_workdir_env.sql"),
                params![&workdir_id],
            ) {
                Ok(current_env_version_id) => current_env_version_id != env_version_id,
                Err(CacheManagerError::SqlError(rusqlite::Error::QueryReturnedNoRows)) => true,
                Err(err) => return Err(err),
            };

            if replace_env {
                // Assign the environment to the workdir
                tx.execute(
                    include_str!("database/sql/up_environments_set_workdir_env.sql"),
                    params![&workdir_id, &env_version_id],
                )?;
            }

            // Check if the currently active open entry is for a different
            // env_version_id or head_sha, in which case we can close the current
            // entry and open a new one
            let replace_history: bool = match tx.query_one::<(String, Option<String>)>(
                include_str!("database/sql/up_environments_get_workdir_history_open.sql"),
                params![&workdir_id],
            ) {
                Ok((current_env_version_id, current_head_sha)) => {
                    current_env_version_id != env_version_id || current_head_sha != head_sha
                }
                Err(CacheManagerError::SqlError(rusqlite::Error::QueryReturnedNoRows)) => true,
                Err(err) => return Err(err),
            };

            if replace_history {
                // Close any open history entry for the workdir
                tx.execute(
                    include_str!("database/sql/up_environments_close_workdir_history.sql"),
                    params![&workdir_id],
                )?;

                // Add an open history entry for the workdir
                tx.execute(
                    include_str!("database/sql/up_environments_add_workdir_history.sql"),
                    params![&workdir_id, &env_version_id, &head_sha],
                )?;
            }

            // Cleanup history
            tx.execute(
                include_str!("database/sql/up_environments_cleanup_history_duplicate_opens.sql"),
                [],
            )?;
            tx.execute(
                include_str!("database/sql/up_environments_cleanup_history_retention.sql"),
                params![&cache_env_config.retention],
            )?;
            tx.execute(
                include_str!("database/sql/up_environments_cleanup_history_max_per_workdir.sql"),
                params![&cache_env_config.max_per_workdir],
            )?;
            tx.execute(
                include_str!("database/sql/up_environments_cleanup_history_max_total.sql"),
                params![&cache_env_config.max_total],
            )?;
            tx.execute(
                include_str!("database/sql/up_environments_delete_orphaned_env.sql"),
                [],
            )?;

            Ok(())
        })?;

        Ok((new_env, replace_env, env_version_id))
    }

    #[cfg(test)]
    pub fn environment_ids(&self) -> BTreeSet<String> {
        let environment_ids: Vec<String> = CacheManager::get()
            .query_as(
                include_str!("database/sql/up_environments_get_env_ids.sql"),
                &[],
            )
            .unwrap();
        environment_ids.into_iter().collect()
    }
}

/// The environment configuration for a work directory
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpEnvironment {
    /// The versions of the tools to be loaded in the environment
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<UpVersion>,
    /// The paths to add to the PATH environment variable
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<PathBuf>,
    /// The environment variables to set
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_vars: Vec<UpEnvVar>,
    /// The modification times of the configuration files
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config_modtimes: BTreeMap<String, u64>,
    /// The hash of the configuration files
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub config_hash: String,
}

impl Hash for UpEnvironment {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.versions.hash(state);
        self.paths.hash(state);
        self.env_vars.hash(state);
        self.config_modtimes.hash(state);
        self.config_hash.hash(state);
    }
}

impl FromRow for UpEnvironment {
    fn from_row(row: &Row) -> Result<Self, CacheManagerError> {
        let versions_json: String = row.get("versions")?;

        let versions: Vec<UpVersion> = match serde_json::from_str(&versions_json) {
            Ok(versions) => versions,
            Err(_err) => {
                let old_versions: Vec<OldUpVersion> = serde_json::from_str(&versions_json)?;
                old_versions.iter().map(|v| v.to_owned().into()).collect()
            }
        };

        let paths_json: String = row.get("paths")?;
        let paths: Vec<PathBuf> = serde_json::from_str(&paths_json)?;

        let env_vars_json: String = row.get("env_vars")?;
        let env_vars: Vec<UpEnvVar> = serde_json::from_str(&env_vars_json)?;

        let config_modtimes_json: String = row.get("config_modtimes")?;
        let config_modtimes: BTreeMap<String, u64> = serde_json::from_str(&config_modtimes_json)?;

        let config_hash: String = row.get("config_hash")?;

        Ok(Self {
            versions,
            paths,
            env_vars,
            config_modtimes,
            config_hash,
        })
    }
}

impl UpEnvironment {
    pub fn new() -> Self {
        Self {
            versions: Vec::new(),
            paths: Vec::new(),
            env_vars: Vec::new(),
            config_modtimes: BTreeMap::new(),
            config_hash: String::new(),
        }
    }

    pub fn init(mut self) -> Self {
        self.config_hash = config(".").up_hash();
        self.config_modtimes = get_config_mod_times(".");
        self
    }

    pub fn hash_string(&self) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    pub fn versions_for_dir(&self, dir: &str) -> Vec<UpVersion> {
        let mut versions: BTreeMap<String, UpVersion> = BTreeMap::new();

        for version in self.versions.iter() {
            // Check if that version applies to the requested dir
            if !version.dir.is_empty()
                && dir != version.dir
                && !dir.starts_with(format!("{}/", version.dir).as_str())
            {
                continue;
            }

            // If there is already a version, check if the current one's dir is more specific
            if let Some(existing_version) = versions.get(&version.tool) {
                if existing_version.dir.len() > version.dir.len() {
                    continue;
                }
            }

            versions.insert(version.tool.clone(), version.clone());
        }

        versions.values().cloned().collect()
    }

    pub fn add_env_var<T>(&mut self, key: T, value: T) -> bool
    where
        T: AsRef<str>,
    {
        self.add_env_var_operation(key, value, EnvOperationEnum::Set)
    }

    pub fn add_env_var_operation<T>(
        &mut self,
        key: T,
        value: T,
        operation: EnvOperationEnum,
    ) -> bool
    where
        T: AsRef<str>,
    {
        let up_env_var = UpEnvVar {
            name: key.as_ref().to_string(),
            value: Some(value.as_ref().to_string()),
            operation,
        };

        self.env_vars.push(up_env_var);

        true
    }

    pub fn add_raw_env_vars(&mut self, env_vars: Vec<UpEnvVar>) -> bool {
        self.env_vars.extend(env_vars);
        true
    }

    pub fn add_path(&mut self, path: PathBuf) -> bool {
        self.paths.retain(|p| p != &path);

        // Prepend anything that starts with the data_home()
        if path.starts_with(data_home()) {
            self.paths.insert(0, path);
        } else {
            self.paths.push(path);
        }

        true
    }

    pub fn add_paths(&mut self, paths: Vec<PathBuf>) -> bool {
        for path in paths {
            self.add_path(path);
        }
        true
    }

    pub fn add_simple_version(
        &mut self,
        backend: &str,
        tool: &str,
        version: &str,
        bin_path: &str,
        dirs: BTreeSet<String>,
    ) -> bool {
        self.add_version(backend, tool, "", "", version, bin_path, dirs)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_version(
        &mut self,
        backend: &str,
        tool: &str,
        plugin_name: &str,
        normalized_name: &str,
        version: &str,
        bin_path: &str,
        dirs: BTreeSet<String>,
    ) -> bool {
        let mut dirs = dirs;
        if dirs.is_empty() {
            dirs.insert("".to_string());
        }

        for exists in self.versions.iter() {
            if exists.backend == backend
                && exists.normalized_name == normalized_name
                && exists.version == version
            {
                dirs.remove(&exists.dir);
                if dirs.is_empty() {
                    break;
                }
            }
        }

        if dirs.is_empty() {
            return false;
        }

        for dir in dirs {
            self.versions.push(UpVersion::new(
                tool,
                plugin_name,
                normalized_name,
                backend,
                version,
                bin_path,
                &dir,
            ));
        }

        true
    }

    pub fn add_version_data_path(
        &mut self,
        normalized_name: &str,
        version: &str,
        dir: &str,
        data_path: &str,
    ) -> bool {
        for exists in self.versions.iter_mut() {
            if exists.normalized_name == normalized_name
                && exists.version == version
                && exists.dir == dir
            {
                exists.data_path = Some(data_path.to_string());
                return true;
            }
        }

        false
    }
}

// TODO: deprecated, remove after leaving time to migrate to the new UpVersion
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OldUpVersion {
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_real_name: Option<String>,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash)]
pub struct UpVersion {
    pub tool: String,
    pub plugin_name: String,
    pub normalized_name: String,
    #[serde(default, skip_serializing_if = "is_default_backend")]
    pub backend: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bin_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_path: Option<String>,
}

fn is_default_backend(backend: &str) -> bool {
    backend.is_empty() || backend == "default"
}

impl From<OldUpVersion> for UpVersion {
    fn from(args: OldUpVersion) -> Self {
        Self {
            tool: args.tool_real_name.unwrap_or(args.tool.clone()),
            plugin_name: args.tool.clone(),
            normalized_name: args.tool,
            backend: "".to_string(),
            version: args.version,
            bin_path: "bin".to_string(),
            dir: args.dir,
            data_path: args.data_path,
        }
    }
}

impl UpVersion {
    pub fn new(
        tool: &str,
        plugin_name: &str,
        normalized_name: &str,
        backend: &str,
        version: &str,
        bin_path: &str,
        dir: &str,
    ) -> Self {
        Self {
            tool: tool.to_string(),
            plugin_name: plugin_name.to_string(),
            normalized_name: normalized_name.to_string(),
            backend: backend.to_string(),
            version: version.to_string(),
            bin_path: bin_path.to_string(),
            dir: dir.to_string(),
            data_path: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash)]
pub struct UpEnvVar {
    #[serde(
        rename = "n",
        alias = "name",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub name: String,
    #[serde(
        rename = "v",
        alias = "value",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub value: Option<String>,
    #[serde(
        rename = "o",
        alias = "operation",
        default,
        skip_serializing_if = "EnvOperationEnum::is_default"
    )]
    pub operation: EnvOperationEnum,
}

impl From<EnvOperationConfig> for UpEnvVar {
    fn from(env_op: EnvOperationConfig) -> Self {
        Self {
            name: env_op.name,
            value: env_op.value,
            operation: env_op.operation,
        }
    }
}

impl From<EnvConfig> for Vec<UpEnvVar> {
    fn from(env_config: EnvConfig) -> Self {
        env_config
            .operations
            .into_iter()
            .map(|operation| operation.into())
            .collect()
    }
}

#[cfg(test)]
#[path = "up_environments_test.rs"]
mod tests;
