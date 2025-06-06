use std::collections::BTreeSet;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;

use normalize_path::NormalizePath;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::cache::utils as cache_utils;
use crate::internal::commands::utils::abs_path;
use crate::internal::config::parser::ConfigErrorHandler;
use crate::internal::config::up::mise::PostInstallFuncArgs;
use crate::internal::config::up::utils::data_path_dir_hash;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::UpConfigMise;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::env::current_dir;
use crate::internal::workdir;
use crate::internal::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UpConfigGolangSerialized {
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version_file: Option<String>,
    #[serde(default, skip_serializing_if = "cache_utils::is_false")]
    upgrade: bool,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    dirs: BTreeSet<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UpConfigGolang {
    pub version: Option<String>,
    pub version_file: Option<String>,
    pub upgrade: bool,
    pub dirs: BTreeSet<String>,
    #[serde(skip)]
    pub backend: OnceCell<UpConfigMise>,
}

impl Serialize for UpConfigGolang {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        let mut serialized = UpConfigGolangSerialized {
            version: self.version.clone(),
            version_file: self.version_file.clone(),
            upgrade: self.upgrade,
            dirs: self.dirs.clone(),
        };

        if serialized.version.is_none() && serialized.version_file.is_none() {
            serialized.version = Some("latest".to_string());
        }

        serialized.serialize(serializer)
    }
}

impl Default for UpConfigGolang {
    fn default() -> Self {
        Self {
            version: None,
            version_file: None,
            upgrade: false,
            backend: OnceCell::new(),
            dirs: BTreeSet::new(),
        }
    }
}

impl UpConfigGolang {
    pub fn new_any_version() -> Self {
        Self {
            version: Some("*".to_string()),
            ..Default::default()
        }
    }

    pub fn from_config_value(
        config_value: Option<&ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let mut version = None;
        let mut version_file = None;
        let mut dirs = BTreeSet::new();
        let mut upgrade = false;

        if let Some(config_value) = config_value {
            if let Some(value) = config_value.as_str() {
                version = Some(value.to_string());
            } else if let Some(value) = config_value.as_float() {
                version = Some(value.to_string());
            } else if let Some(value) = config_value.as_integer() {
                version = Some(value.to_string());
            } else {
                if let Some(value) =
                    config_value.get_as_str_or_none("version", &error_handler.with_key("version"))
                {
                    version = Some(value.to_string());
                } else if let Some(value) = config_value
                    .get_as_str_or_none("version_file", &error_handler.with_key("version_file"))
                {
                    version_file = Some(value.to_string());
                }

                let list_dirs =
                    config_value.get_as_str_array("dir", &error_handler.with_key("dir"));
                for value in list_dirs {
                    dirs.insert(
                        PathBuf::from(value)
                            .normalize()
                            .to_string_lossy()
                            .to_string(),
                    );
                }

                if let Some(value) =
                    config_value.get_as_bool_or_none("upgrade", &error_handler.with_key("upgrade"))
                {
                    upgrade = value;
                }
            }
        }

        Self {
            backend: OnceCell::new(),
            version,
            version_file,
            upgrade,
            dirs,
        }
    }

    pub fn up(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        self.backend()?.up(options, environment, progress_handler)
    }

    pub fn commit(&self, options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        self.backend()?.commit(options, env_version_id)
    }

    pub fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        self.backend()?.down(progress_handler)
    }

    pub fn was_upped(&self) -> bool {
        self.backend().is_ok_and(|backend| backend.was_upped())
    }

    pub fn data_paths(&self) -> Vec<PathBuf> {
        self.backend()
            .map_or(vec![], |backend| backend.data_paths())
    }

    pub fn backend(&self) -> Result<&UpConfigMise, UpError> {
        self.backend.get_or_try_init(|| {
            let version = if let Some(version) = &self.version {
                version.clone()
            } else if let Some(version) = self.extract_version_from_gomod()? {
                version
            } else {
                "latest".to_string()
            };

            let mut backend =
                UpConfigMise::new("go", version.as_ref(), self.dirs.clone(), self.upgrade);
            backend.add_detect_version_func(detect_version_from_gomod);
            backend.add_post_install_func(setup_individual_gopath);

            Ok(backend)
        })
    }

    pub fn version(&self) -> Result<String, UpError> {
        self.backend()?.version()
    }

    fn extract_version_from_gomod(&self) -> Result<Option<String>, UpError> {
        if self.version_file.is_none() {
            return Ok(None);
        }

        extract_version_from_gomod_file(self.version_file.as_ref().unwrap().clone())
    }
}

fn detect_version_from_gomod(_tool_name: String, path: PathBuf) -> Option<String> {
    let version_file_path = path.join("go.mod");
    if !version_file_path.exists() || version_file_path.is_dir() {
        return None;
    }

    extract_version_from_gomod_file(version_file_path).unwrap_or(None)
}

fn extract_version_from_gomod_file(
    version_file: impl AsRef<Path>,
) -> Result<Option<String>, UpError> {
    // Get the version file abs path
    let version_file = abs_path(version_file);

    // Open the file and read it line by line
    let file = File::open(version_file.clone());
    if let Err(err) = &file {
        return Err(UpError::Exec(format!(
            "failed to open version file ({}): {}",
            version_file.display(),
            err,
        )));
    }

    let file = file.unwrap();
    let reader = BufReader::new(file);

    // Prepare the regex to extract the version
    let goversion = regex::Regex::new(r"(?m)^go (?<version>\d+\.\d+(?:\.\d+)?)$")
        .expect("failed to compile regex");

    for line in reader.lines() {
        if line.is_err() {
            continue;
        }
        let line = line.unwrap();

        // Check if the line contains the version, we use simple string matching first
        // as it is way faster than regex
        if line.starts_with("go ") {
            // Try and match the regex to extract the version
            if let Some(captures) = goversion.captures(&line) {
                // Get the version
                let version = captures.name("version").unwrap().as_str().to_string();

                // Return the version
                return Ok(Some(version));
            }
        }
    }

    // Return None if we didn't find the version
    Err(UpError::Exec(format!(
        "no version found in version file ({})",
        version_file.display(),
    )))
}

fn setup_individual_gopath(
    _options: &UpOptions,
    environment: &mut UpEnvironment,
    _progress_handler: &UpProgressHandler,
    args: &PostInstallFuncArgs,
) -> Result<(), UpError> {
    if args.fqtn.tool() != "go" {
        panic!(
            "setup_individual_gopath called with wrong tool: {}",
            args.fqtn.tool()
        );
    }

    // Get the data path for the work directory
    let workdir = workdir(".");

    let data_path = match workdir.data_path() {
        Some(data_path) => data_path,
        None => {
            return Err(UpError::Exec(format!(
                "failed to get data path for {}",
                current_dir().display()
            )));
        }
    };

    // Handle each version individually
    for version in &args.versions {
        for dir in &version.dirs {
            let gopath_dir = data_path_dir_hash(dir);

            let normalized_name = args.fqtn.normalized_plugin_name()?;

            let gopath = data_path
                .join(&normalized_name)
                .join(&version.version)
                .join(&gopath_dir);

            environment.add_version_data_path(
                &normalized_name,
                &version.version,
                dir,
                &gopath.to_string_lossy(),
            );
        }
    }

    Ok(())
}
