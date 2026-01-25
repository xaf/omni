use std::io;
use std::path::PathBuf;

use itertools::Itertools;

use crate::internal::config::ConfigScope;
use crate::internal::env::config_home;
use crate::internal::env::user_home;
use crate::internal::env::xdg_config_home;
use crate::internal::workdir;

pub const WORKDIR_CONFIG_FILES: [&str; 2] = [".omni.yaml", ".omni/config.yaml"];

/// Configuration loader with static utility methods.
///
/// This struct provides static methods for:
/// - Discovering configuration file locations
/// - Editing configuration files using compote
///
/// For loading configuration, use `OmniConfigLoader` from `compote_loader.rs`.
#[derive(Debug, Clone)]
pub struct ConfigLoader;

impl ConfigLoader {
    /// Returns all configuration files in load order with their scopes.
    ///
    /// Order: system pre -> user -> system post -> workdir
    pub fn all_config_files() -> Vec<(String, ConfigScope)> {
        let mut config_files = vec![];

        config_files.extend(
            Self::system_config_files("pre")
                .into_iter()
                .map(|f| (f, ConfigScope::System)),
        );
        config_files.extend(
            Self::user_config_files()
                .into_iter()
                .map(|f| (f, ConfigScope::User)),
        );
        config_files.extend(
            Self::system_config_files("post")
                .into_iter()
                .map(|f| (f, ConfigScope::System)),
        );

        let wd = workdir(".");
        if let Some(wd_root) = wd.root() {
            for workdir_config_file in WORKDIR_CONFIG_FILES.iter() {
                let file = PathBuf::from(wd_root).join(workdir_config_file);
                if file.exists() {
                    config_files.push((file.to_string_lossy().to_string(), ConfigScope::Workdir));
                }
            }
        }

        config_files
    }

    /// Returns the list of user configuration file paths.
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

    /// Returns the list of system configuration file paths for a given prefix.
    fn system_config_files(prefix: &str) -> Vec<String> {
        let mut config_files = vec![];

        // Check for single file /etc/omni/(pre/post).yaml
        let file = format!("/etc/omni/{prefix}.yaml");
        if PathBuf::from(&file).is_file() {
            config_files.push(file);
        }

        // Check glob pattern /etc/omni/(pre/post).d/*.yaml in lexicographical order
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

    /// Edit the first writeable user config file using compote's Config API.
    ///
    /// Searches for user config files in reverse order and edits the first
    /// one that is writeable (either exists with write permission, or can be
    /// created because the parent directory is writeable).
    pub fn edit_main_user_config_file_compote<F>(edit_fn: F) -> io::Result<()>
    where
        F: FnOnce(&mut compote::Config) -> bool,
    {
        let candidates = Self::user_config_files()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        match compote::edit_first_writeable(&candidates, edit_fn)? {
            Some(_path) => Ok(()),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no writeable user config file found",
            )),
        }
    }

    /// Edit a workdir config file using compote's Config API.
    pub fn edit_workdir_config_file_compote<F>(file_path: &str, edit_fn: F) -> io::Result<()>
    where
        F: FnOnce(&mut compote::Config) -> bool,
    {
        compote::edit_file(file_path, edit_fn).map(|_| ())
    }

    /// Edit a user config file using compote's Config API (specific file).
    pub fn edit_user_config_file_compote<F>(file_path: &str, edit_fn: F) -> io::Result<()>
    where
        F: FnOnce(&mut compote::Config) -> bool,
    {
        compote::edit_file(file_path, edit_fn).map(|_| ())
    }
}
