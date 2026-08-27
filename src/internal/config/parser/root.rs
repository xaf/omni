use std::collections::HashMap;

use std::sync::Mutex;

use lazy_static::lazy_static;
use serde::Serialize;

use crate::internal::config::OmniConfig;
use crate::internal::config::OmniConfigLoader;
use crate::internal::user_interface::colors::StringColor;
use crate::internal::workdir;
use crate::omni_error;

lazy_static! {
    #[derive(Debug, Serialize, Clone)]
    static ref CONFIG_PER_PATH: Mutex<OmniConfigPerPath> = Mutex::new(OmniConfigPerPath::new());
}

pub fn config(path: &str) -> OmniConfig {
    let path = if path == "/" {
        path.to_owned()
    } else {
        std::fs::canonicalize(path)
            .unwrap_or(path.to_owned().into())
            .to_str()
            .unwrap()
            .to_owned()
    };

    let mut config_per_path = CONFIG_PER_PATH.lock().unwrap();
    config_per_path.get(&path).clone()
}

pub fn flush_config(path: &str) {
    if path == "/" {
        let mut config_per_path = CONFIG_PER_PATH.lock().unwrap();
        config_per_path.config.clear();
        return;
    }

    let path = std::fs::canonicalize(path)
        .unwrap_or(path.to_owned().into())
        .to_str()
        .unwrap()
        .to_owned();

    // Flush the configuration cache
    let mut config_per_path = CONFIG_PER_PATH.lock().unwrap();
    config_per_path.config.remove(&path);
}

pub fn global_config() -> OmniConfig {
    config("/")
}

#[derive(Debug, Serialize, Clone)]
pub struct OmniConfigPerPath {
    config: HashMap<String, OmniConfig>,
}

impl OmniConfigPerPath {
    pub fn new() -> Self {
        Self {
            config: HashMap::new(),
        }
    }

    pub fn get(&mut self, path: &str) -> &OmniConfig {
        // Get the git root path, if any
        let key = if path == "/" {
            path.to_owned()
        } else {
            let wd = workdir(path);
            if let Some(wd_root) = wd.root() {
                wd_root.to_owned()
            } else {
                path.to_owned()
            }
        };

        // Get the config for the path
        if !self.config.contains_key(&key) {
            let new_config = load_omni_config_with_feuilletage(&key);
            self.config.insert(key.clone(), new_config);
        }

        self.config.get(&key).unwrap()
    }
}

/// Load omni configuration using the new feuilletage-based OmniConfigLoader.
///
/// This function provides the new configuration loading path that uses feuilletage's
/// ConfigLoaderBuilder with omni's file discovery logic. It serves as a parallel
/// implementation to the existing `config()` function.
///
/// # Arguments
/// * `path` - The path to load configuration for. Use "/" for global config only,
///            or a specific path to include workdir configuration.
///
/// # Returns
/// An `OmniConfig` instance. If deserialization fails, logs the error and returns
/// a default configuration.
///
/// # Example
/// ```ignore
/// // Load global config only
/// let global = load_omni_config_with_feuilletage("/");
///
/// // Load config with workdir
/// let local = load_omni_config_with_feuilletage("/path/to/workdir");
/// ```
pub fn load_omni_config_with_feuilletage(path: &str) -> OmniConfig {
    let mut loader = if path == "/" {
        OmniConfigLoader::new_global()
    } else {
        OmniConfigLoader::new_with_workdir(path)
    };

    match loader.deserialize::<OmniConfig>() {
        Ok(config) => config,
        Err(e) => {
            omni_error!(format!("configuration deserialization error: {}", e));
            OmniConfig::default()
        }
    }
}
