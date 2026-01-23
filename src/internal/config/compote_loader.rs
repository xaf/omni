//! Compote configuration loader adapter for omni.
//!
//! This module provides an adapter that wraps compote's ConfigLoaderBuilder
//! with omni's file discovery logic for system, user, and workdir configuration files.

use std::path::PathBuf;

use compote::de::MutabilityInfo;
use compote::Error as ConfigError;
use compote::ConfigLoaderBuilder;
use compote::FromContextValue;
use compote::Level;
use itertools::Itertools;

use crate::internal::env::config_home;
use crate::internal::env::user_home;
use crate::internal::env::xdg_config_home;
use crate::internal::workdir;

/// Workdir configuration file names (same as loader.rs).
pub const WORKDIR_CONFIG_FILES: [&str; 2] = [".omni.yaml", ".omni/config.yaml"];

/// Returns system configuration files for the given prefix.
///
/// Looks for:
/// - `/etc/omni/{prefix}.yaml`
/// - `/etc/omni/{prefix}.d/*.yaml` (sorted lexicographically)
///
/// # Arguments
/// * `prefix` - Either "pre" or "post" for system pre/post configuration
pub fn system_config_files(prefix: &str) -> Vec<String> {
    let mut config_files = vec![];

    // Check for single file /etc/omni/(pre/post).yaml
    let file = format!("/etc/omni/{prefix}.yaml");
    if PathBuf::from(&file).is_file() {
        config_files.push(file);
    }

    // Use a glob pattern to check in /etc/omni/(pre/post).d/<file>.yaml
    // and apply the files in lexicographical order
    let glob_pattern = format!("/etc/omni/{prefix}.d/*.yaml");
    if let Ok(entries) = glob::glob(&glob_pattern) {
        for path in entries.into_iter().flatten().sorted() {
            if !path.is_file() {
                continue;
            }

            config_files.push(path.to_string_lossy().to_string());
        }
    }

    config_files
}

/// Returns user configuration file paths.
///
/// Returns paths to (in order):
/// - `~/.omni.yaml`
/// - `$XDG_CONFIG_HOME/omni.yaml`
/// - `$OMNI_CONFIG_HOME/config.yaml`
/// - `$OMNI_CONFIG_HOME/config-dev.yaml` (only in debug builds)
/// - `$OMNI_CONFIG` (if set)
pub fn user_config_files() -> Vec<String> {
    vec![
        format!("{}/.omni.yaml", user_home()),
        format!("{}/omni.yaml", xdg_config_home()),
        format!("{}/config.yaml", config_home()),
        if cfg!(debug_assertions) {
            format!("{}/config-dev.yaml", config_home())
        } else {
            "".to_owned()
        },
        std::env::var("OMNI_CONFIG").unwrap_or("".to_owned()),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<String>>()
}

/// Returns workdir configuration file paths for the given path.
///
/// Looks for `.omni.yaml` and `.omni/config.yaml` in the workdir root.
///
/// # Arguments
/// * `path` - The path to search from (will be resolved to workdir root)
pub fn workdir_config_files(path: &str) -> Vec<String> {
    let wd = workdir(path);
    let wd_root = if let Some(wd_root) = wd.root() {
        wd_root
    } else {
        path
    };

    let mut workdir_files = vec![];
    for workdir_config_file in WORKDIR_CONFIG_FILES.iter() {
        workdir_files.push(format!("{wd_root}/{workdir_config_file}"));
    }

    workdir_files
}

/// Omni configuration loader wrapping compote's ConfigLoaderBuilder.
///
/// Provides omni-specific file discovery logic while using compote for
/// the actual configuration loading, merging, and deserialization.
///
/// # Example
///
/// ```no_run
/// use compote::FromContextValue;
///
/// #[derive(Debug, compote::Config)]
/// struct MyConfig {
///     #[compote(default = "default")]
///     name: String,
/// }
///
/// // Load global config (system_pre + user + system_post)
/// let config: MyConfig = OmniConfigLoader::new_global()
///     .deserialize()
///     .unwrap();
///
/// // Load config with workdir
/// let config: MyConfig = OmniConfigLoader::new_with_workdir(".")
///     .deserialize()
///     .unwrap();
/// ```
pub struct OmniConfigLoader {
    builder: ConfigLoaderBuilder,
    loaded_files: Vec<String>,
}

impl OmniConfigLoader {
    /// Creates a new empty OmniConfigLoader.
    pub fn new() -> Self {
        Self {
            builder: ConfigLoaderBuilder::new(),
            loaded_files: Vec::new(),
        }
    }

    /// Loads system pre-configuration files.
    ///
    /// Loads `/etc/omni/pre.yaml` and `/etc/omni/pre.d/*.yaml`.
    pub fn load_system_pre(&mut self) -> &mut Self {
        self.load_files(system_config_files("pre"), Level::System)
    }

    /// Loads user configuration files.
    ///
    /// Loads user config files from standard locations.
    pub fn load_user(&mut self) -> &mut Self {
        self.load_files(user_config_files(), Level::User)
    }

    /// Loads system post-configuration files.
    ///
    /// Loads `/etc/omni/post.yaml` and `/etc/omni/post.d/*.yaml`.
    pub fn load_system_post(&mut self) -> &mut Self {
        self.load_files(system_config_files("post"), Level::System)
    }

    /// Loads workdir configuration files.
    ///
    /// Loads `.omni.yaml` and `.omni/config.yaml` from the workdir root.
    pub fn load_workdir(&mut self, path: &str) -> &mut Self {
        self.load_files(workdir_config_files(path), Level::Local)
    }

    /// Creates a new OmniConfigLoader with global configuration loaded.
    ///
    /// Loads: system_pre + user + system_post
    pub fn new_global() -> Self {
        let mut loader = Self::new();
        loader.load_system_pre();
        loader.load_user();
        loader.load_system_post();
        loader
    }

    /// Creates a new OmniConfigLoader from a single file.
    ///
    /// # Arguments
    /// * `file` - The path to the configuration file
    /// * `level` - The Level to associate with values from this file
    pub fn new_from_file(file: &str, level: Level) -> Self {
        let mut loader = Self::new();
        loader.load_files(vec![file.to_string()], level);
        loader
    }

    /// Creates a new OmniConfigLoader from a list of files with their levels.
    ///
    /// # Arguments
    /// * `files` - A list of (file_path, Level) pairs
    pub fn new_from_files(files: Vec<(String, Level)>) -> Self {
        let mut loader = Self::new();
        for (file, level) in files {
            loader.load_files(vec![file], level);
        }
        loader
    }

    /// Creates a new OmniConfigLoader with global and workdir configuration loaded.
    ///
    /// Loads: system_pre + user + system_post + workdir
    pub fn new_with_workdir(path: &str) -> Self {
        let mut loader = Self::new_global();
        loader.load_workdir(path);
        loader
    }

    /// Deserializes the loaded configuration into the target type.
    ///
    /// This enforces `mutable_by` constraints from the target type during merge.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn deserialize<T: FromContextValue + MutabilityInfo>(
        &mut self,
    ) -> Result<T, ConfigError> {
        self.builder.deserialize()
    }

    /// Returns the merged configuration as a compote Config.
    ///
    /// This builds the Config without enforcing struct-level `mutable_by` constraints.
    /// Use this when you need access to the raw merged configuration value.
    ///
    /// # Errors
    ///
    /// Returns an error if building fails.
    pub fn build(&mut self) -> Result<compote::Config, ConfigError> {
        // We need to consume and rebuild the builder since build() consumes self
        let builder = std::mem::take(&mut self.builder);
        builder.build()
    }

    /// Returns the list of files that were successfully loaded.
    pub fn loaded_files(&self) -> &[String] {
        &self.loaded_files
    }

    /// Checks if any user configuration file was loaded.
    ///
    /// This is useful to determine if the user has a config file set up,
    /// which can be used to trigger first-run setup flows.
    pub fn has_user_config(&self) -> bool {
        let user_config_files = user_config_files();
        for user_config_file in user_config_files {
            if self.loaded_files.contains(&user_config_file) {
                return true;
            }
        }
        false
    }

    /// Loads multiple configuration files with the given level.
    fn load_files(&mut self, files: Vec<String>, level: Level) -> &mut Self {
        // We need to take the builder, modify it, and put it back
        // because load_file consumes and returns self
        let mut builder = std::mem::take(&mut self.builder);

        for file in files {
            let file_path = PathBuf::from(&file);
            let before_count = builder.loaded_files().len();

            builder = builder.load_file(&file_path, level.clone());

            // Check if the file was actually loaded
            if builder.loaded_files().len() > before_count {
                self.loaded_files.push(file);
            }
        }

        self.builder = builder;
        self
    }
}

impl Default for OmniConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_config_files_returns_empty_when_no_files() {
        // This test assumes /etc/omni/nonexistent.yaml doesn't exist
        let files = system_config_files("nonexistent");
        assert!(files.is_empty());
    }

    #[test]
    fn test_user_config_files_contains_expected_paths() {
        let files = user_config_files();
        // Should at least contain the home directory config
        assert!(files.iter().any(|f| f.contains(".omni.yaml")));
    }
}
