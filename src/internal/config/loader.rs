use std::collections::HashMap;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::exit;
use std::sync::Mutex;

use itertools::Itertools;
use lazy_static::lazy_static;

use crate::internal::config::ConfigExtendStrategy;
use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigSource;
use crate::internal::config::ConfigValue;
use crate::internal::config::omni_config_loader;
use crate::internal::env::config_home;
use crate::internal::env::user_home;
use crate::internal::env::xdg_config_home;
use crate::internal::git::path_entry_config;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::omni_error;
use crate::omni_print;

use config_value::FileDefinition;

lazy_static! {
    #[derive(Debug)]
    static ref CONFIG_LOADER_PER_PATH: Mutex<ConfigLoaderPerPath> = Mutex::new(ConfigLoaderPerPath::new());

    #[derive(Debug)]
    static ref CONFIG_LOADER_GLOBAL: ConfigLoader = ConfigLoader::new();
}

pub const WORKDIR_CONFIG_FILES: [&str; 2] = [".omni.yaml", ".omni/config.yaml"];

pub fn config_loader(path: &str) -> ConfigLoader {
    let path = if path == "/" {
        path.to_owned()
    } else {
        std::fs::canonicalize(path)
            .unwrap_or(path.to_owned().into())
            .to_str()
            .unwrap()
            .to_owned()
    };

    let mut config_loader_per_path = CONFIG_LOADER_PER_PATH.lock().unwrap();
    config_loader_per_path.get(&path).clone()
}

pub fn flush_config_loader(path: &str) {
    if path == "/" {
        let mut config_loader_per_path = CONFIG_LOADER_PER_PATH.lock().unwrap();
        config_loader_per_path.loaders.clear();
        return;
    }

    let path = std::fs::canonicalize(path)
        .unwrap_or(path.to_owned().into())
        .to_str()
        .unwrap()
        .to_owned();

    let mut config_loader_per_path = CONFIG_LOADER_PER_PATH.lock().unwrap();
    config_loader_per_path.loaders.remove(&path);
}

pub fn global_config_loader() -> ConfigLoader {
    config_loader("/")
}

#[derive(Debug)]
pub struct ConfigLoaderPerPath {
    loaders: HashMap<String, ConfigLoader>,
}

impl ConfigLoaderPerPath {
    fn new() -> Self {
        Self {
            loaders: HashMap::new(),
        }
    }

    pub fn get(&mut self, path: &str) -> &ConfigLoader {
        if !self.loaders.contains_key(path) {
            let config_loader = if path == "/" {
                ConfigLoader::new_global()
            } else {
                self.get("/").get_local(path)
            };

            self.loaders.insert(path.to_owned(), config_loader);
        }

        self.loaders.get(path).unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct ConfigLoader {
    pub loaded_config_files: Vec<String>,
    pub raw_config: ConfigValue,
}

impl ConfigLoader {
    fn new() -> Self {
        Self::new_global()
    }

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

    fn user_config_files() -> Vec<String> {
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

    pub fn has_user_config(&self) -> bool {
        let user_config_files = Self::user_config_files();
        for user_config_file in user_config_files {
            if self.loaded_config_files.contains(&user_config_file) {
                return true;
            }
        }
        false
    }

    fn system_config_files(prefix: &str) -> Vec<String> {
        let mut config_files = vec![];

        // We can just check for a single file /etc/omni/(pre/post).yaml
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

    fn new_global() -> Self {
        let mut new_config_loader = Self {
            loaded_config_files: vec![],
            raw_config: ConfigValue::empty_with(ConfigSource::Default, ConfigScope::Default),
        };

        new_config_loader
            .import_config_files(Self::system_config_files("pre"), ConfigScope::System);

        new_config_loader.import_config_files(Self::user_config_files(), ConfigScope::User);

        new_config_loader
            .import_config_files(Self::system_config_files("post"), ConfigScope::System);

        new_config_loader
    }

    pub fn new_empty() -> Self {
        Self {
            loaded_config_files: vec![],
            raw_config: ConfigValue::new_null_with(ConfigSource::Null, ConfigScope::Null),
        }
    }

    pub fn new_from_file(file: &str, scope: ConfigScope) -> Self {
        let mut loader = Self::new_empty();
        loader.import_config_file(file, scope);
        loader
    }

    #[deprecated(since = "0.1.0", note = "Use edit_main_user_config_file_compote instead")]
    #[allow(deprecated)]
    pub fn edit_main_user_config_file<F>(edit_fn: F) -> io::Result<()>
    where
        F: FnOnce(&mut ConfigValue) -> bool,
    {
        // We will use the list of user config files, but in reverse, to
        // look for the first user configuration file that we can write
        // into, and that would be the LAST loaded file when reading
        // the configuration
        let config_files = Self::user_config_files()
            .into_iter()
            .rev()
            .map(PathBuf::from)
            .collect::<Vec<PathBuf>>();

        // We first try and look for a file that already exists and that
        // we can simply edit
        let mut found_file: Option<PathBuf> = None;
        for config_file in &config_files {
            if let Ok(metadata) = config_file.metadata() {
                if config_file.is_file() {
                    let permissions = metadata.permissions();
                    let mode = permissions.mode();
                    let has_read_access = mode & 0o400 == 0o400;
                    let has_write_access = mode & 0o200 == 0o200;

                    if has_read_access && has_write_access {
                        found_file = Some(config_file.clone());
                        break;
                    }
                }
            }
        }

        // But if we don't find any, we want to find the first path that exists
        // and is writeable, by looking at the same file list, but checking their
        // parents until finding one that exists. If the parent exists but does
        // not have write permissions, we will not be able to write to the file
        // so we can skip to the next file path
        if found_file.is_none() {
            'outer: for config_file in &config_files {
                let mut parent = config_file.clone();
                parent.pop();

                while !parent.exists() {
                    parent.pop();
                }

                if parent.is_dir() {
                    if let Ok(metadata) = parent.metadata() {
                        let permissions = metadata.permissions();
                        let mode = permissions.mode();
                        let has_write_access = mode & 0o200 == 0o200;

                        if has_write_access {
                            found_file = Some(config_file.clone());
                            break 'outer;
                        }
                    }
                }
            }
        }

        // If we get here and we still have no file, we can raise an error
        if found_file.is_none() {
            omni_error!("unable to find a writeable user config file");
            exit(1);
        }
        let found_file = found_file.unwrap();
        let file_path = format!("{}", found_file.display());

        Self::edit_user_config_file(file_path, edit_fn)
    }

    #[deprecated(since = "0.1.0", note = "Use edit_user_config_file_compote instead")]
    #[allow(deprecated)]
    pub fn edit_user_config_file<F>(file_path: String, edit_fn: F) -> io::Result<()>
    where
        F: FnOnce(&mut ConfigValue) -> bool,
    {
        Self::edit_config_file(file_path, ConfigScope::User, edit_fn)
    }

    #[allow(dead_code)]
    #[deprecated(since = "0.1.0", note = "Use edit_workdir_config_file_compote instead")]
    #[allow(deprecated)]
    pub fn edit_workdir_config_file<F>(file_path: String, edit_fn: F) -> io::Result<()>
    where
        F: FnOnce(&mut ConfigValue) -> bool,
    {
        Self::edit_config_file(file_path, ConfigScope::Workdir, edit_fn)
    }

    #[deprecated(since = "0.1.0", note = "Use the compote-based edit functions instead")]
    pub fn edit_config_file<F>(file_path: String, scope: ConfigScope, edit_fn: F) -> io::Result<()>
    where
        F: FnOnce(&mut ConfigValue) -> bool,
    {
        let loader = omni_config_loader()
            .with_default_extend_strategy(ConfigExtendStrategy::Raw);

        let source = ConfigSource::File(file_path.clone());

        // Use the auto-detecting edit_file - it will detect YAML/JSON format automatically
        loader.edit_file(&file_path, source, scope, edit_fn)
    }

    /// Edit using compote's Config API - finds first writeable user config
    pub fn edit_main_user_config_file_compote<F>(edit_fn: F) -> io::Result<()>
    where
        F: FnOnce(&mut compote::Config) -> bool,
    {
        let candidates = Self::user_config_files()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        match compote::Config::edit_first_writeable(&candidates, edit_fn)? {
            Some(_path) => Ok(()),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no writeable user config file found",
            )),
        }
    }

    /// Edit a workdir config file using compote's Config API
    pub fn edit_workdir_config_file_compote<F>(file_path: &str, edit_fn: F) -> io::Result<()>
    where
        F: FnOnce(&mut compote::Config) -> bool,
    {
        compote::Config::edit_file(file_path, edit_fn).map(|_| ())
    }

    /// Edit a user config file using compote's Config API (specific file)
    pub fn edit_user_config_file_compote<F>(file_path: &str, edit_fn: F) -> io::Result<()>
    where
        F: FnOnce(&mut compote::Config) -> bool,
    {
        compote::Config::edit_file(file_path, edit_fn).map(|_| ())
    }

    // fn new_local_only(path: &str) -> Self {
    // ConfigLoader::new_empty().get_local(path)
    // }

    pub fn get_local(&self, path: &str) -> Self {
        let mut new_config_loader = Self {
            loaded_config_files: self.loaded_config_files.clone(),
            raw_config: self.raw_config.clone(),
        };

        let wd = workdir(path);
        let wd_root = if let Some(wd_root) = wd.root() {
            wd_root
        } else {
            path
        };

        let mut workdir_config_files = vec![];
        for workdir_config_file in WORKDIR_CONFIG_FILES.iter() {
            workdir_config_files.push(format!("{wd_root}/{workdir_config_file}"));
        }

        new_config_loader.import_config_files(workdir_config_files, ConfigScope::Workdir);

        new_config_loader
    }

    pub fn import_config_files(&mut self, config_files: Vec<String>, scope: ConfigScope) {
        // Prepare files that haven't been loaded yet
        let files_to_load: Vec<_> = config_files
            .iter()
            .filter(|f| !self.loaded_config_files.contains(f))
            .map(|f| {
                let path_entry_config = path_entry_config(f);
                let source = if path_entry_config.package.is_some() {
                    ConfigSource::Package(path_entry_config)
                } else {
                    ConfigSource::File(f.to_string())
                };
                FileDefinition::new(f.as_str())
                    .with_source(source)
                    .with_scope(scope.clone())
            })
            .collect();

        // Use the generic load_and_merge_files method
        let loader = omni_config_loader();
        match loader.load_and_merge_files(&mut self.raw_config, files_to_load) {
            Ok(loaded_files) => {
                // Add successfully loaded files to the list
                self.loaded_config_files.extend(loaded_files);
            }
            Err(err) => {
                // Individual file errors are handled by load_and_merge_files
                // This only catches unexpected errors
                omni_print!(format!(
                    "{} {}",
                    "configuration error: unable to load files:".red(),
                    err
                ));
                exit(1);
            }
        }
    }

    pub fn import_config_file(&mut self, config_file: &str, scope: ConfigScope) {
        self.import_config_file_with_strategy(config_file, scope, ConfigExtendStrategy::Default)
    }

    pub fn import_config_file_with_strategy(
        &mut self,
        config_file: &str,
        scope: ConfigScope,
        strategy: ConfigExtendStrategy,
    ) {
        // Check if file exists first
        if !PathBuf::from(config_file).exists() {
            return;
        }

        // Determine source type (package or file)
        let path_entry_config = path_entry_config(config_file);
        let source = if path_entry_config.package.is_some() {
            ConfigSource::Package(path_entry_config)
        } else {
            ConfigSource::File(config_file.to_string())
        };

        // Use the generic load_and_merge_file_with_strategy from config-value
        let loader = omni_config_loader();
        match loader.load_and_merge_file_with_strategy(
            &mut self.raw_config,
            config_file,
            source,
            scope,
            strategy,
        ) {
            Ok(_) => {
                self.loaded_config_files.push(config_file.to_string());
            }
            Err(err) => {
                omni_print!(format!(
                    "{} {}",
                    format!("configuration error: unable to parse {config_file}:").red(),
                    err
                ));
                exit(1);
            }
        }
    }
}
