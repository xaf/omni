use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as StdCommand;

use normalize_path::NormalizePath;
use once_cell::sync::Lazy;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use tokio::process::Command as TokioCommand;
use walkdir::WalkDir;

use crate::internal::cache::mise_operation::MisePluginVersions;
use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::cache::utils as cache_utils;
use crate::internal::cache::CacheManagerError;
use crate::internal::cache::MiseOperationCache;
use crate::internal::config;
use crate::internal::config::global_config;
use crate::internal::config::up::homebrew::HomebrewInstall;
use crate::internal::config::up::utils::data_path_dir_hash;
use crate::internal::config::up::utils::force_remove_dir_all;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::CommandExt;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::utils::VersionMatcher;
use crate::internal::config::up::utils::VersionParser;
use crate::internal::config::up::UpConfigGithubRelease;
use crate::internal::config::up::UpConfigHomebrew;
use crate::internal::config::up::UpConfigNix;
use crate::internal::config::up::UpConfigTool;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::config::utils::is_executable;
use crate::internal::config::ConfigValue;
use crate::internal::dynenv::update_dynamic_env_for_command_from_env;
use crate::internal::env::cache_home;
use crate::internal::env::data_home;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::omni_warning;

static MISE_PATH: Lazy<String> = Lazy::new(|| format!("{}/mise", data_home()));
static MISE_BIN: Lazy<String> = Lazy::new(|| format!("{}/bin/mise", *MISE_PATH));
static MISE_CACHE_PATH: Lazy<String> = Lazy::new(|| format!("{}/mise", cache_home()));

type DetectVersionFunc = fn(tool_real_name: String, path: PathBuf) -> Option<String>;
type PostInstallFunc = fn(
    options: &UpOptions,
    environment: &mut UpEnvironment,
    progress_handler: &dyn ProgressHandler,
    args: &PostInstallFuncArgs,
) -> Result<(), UpError>;

/// A struct representing the arguments that will be passed to the post-install
/// functions as they are being called.
pub struct PostInstallFuncArgs {
    pub config_value: Option<ConfigValue>,
    pub tool: String,
    pub tool_real_name: String,
    pub requested_version: String,
    pub versions: Vec<MiseToolUpVersion>,
}

pub fn mise_path() -> String {
    (*MISE_PATH).clone()
}

pub fn mise_cache_path() -> String {
    (*MISE_CACHE_PATH).clone()
}

fn mise_bin() -> &'static str {
    MISE_BIN.as_str()
}

fn configure_mise_command<T>(command: &mut T) -> &mut T
where
    T: CommandExt,
{
    command.env_remove_prefix("MISE_");
    command.env_remove("INSTALL_PREFIX");
    command.env_remove("DESTDIR");
    command.env("MISE_CONFIG_DIR", mise_path());
    command.env("MISE_DATA_DIR", mise_path());
    command.env("MISE_CACHE_DIR", mise_cache_path());
    command.env("MISE_LIBGIT2", "false");
    command.env("MISE_RUSTUP_HOME", format!("{}/rustup", mise_path()));
    command.env("MISE_CARGO_HOME", format!("{}/cargo", mise_path()));
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    command.current_dir("/tmp");
    command
}

fn mise_async_command() -> TokioCommand {
    let mut command = TokioCommand::new(mise_bin());
    configure_mise_command(&mut command);
    command
}

fn mise_sync_command() -> StdCommand {
    let mut command = StdCommand::new(mise_bin());
    configure_mise_command(&mut command);
    command
}

pub fn mise_tool_path(tool: &str, version: &str) -> String {
    format!("{}/installs/{}/{}", mise_path(), tool, version)
}

fn is_mise_installed() -> bool {
    is_executable(Path::new(mise_bin()))
}

fn install_mise(options: &UpOptions, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
    let cache = MiseOperationCache::get();

    let (fail_on_error, migrate_from_asdf) = if !is_mise_installed() {
        progress_handler.progress("installing mise".to_string());

        // Check if we need to perform any migration, which is true if the `data_home()/asdf`
        // directory exists and the `mise_path()` directory does not exist
        let should_migrate =
            !Path::new(&mise_path()).exists() && Path::new(&data_home()).join("asdf").exists();

        (true, should_migrate)
    } else if cache.should_update_mise() {
        // Run `mise --version` to check if mise has an update available
        let mut command = mise_sync_command();
        command.arg("--version");
        let output = command
            .output()
            .map_err(|err| UpError::Exec(format!("failed to check mise version: {}", err)))?;

        // If stderr contains `mise self-update`, this means we need to update mise
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("mise self-update") {
            progress_handler.progress("updating mise".to_string());

            (false, false)
        } else {
            if let Err(err) = cache.updated_mise() {
                progress_handler.progress(format!(
                    "failed to update cache for last mise update check: {}",
                    err
                ));
            }

            return Ok(());
        }
    } else {
        return Ok(());
    };

    let gh_release = UpConfigGithubRelease::new_latest_version("jdx/mise");

    // We create a fake environment since we do not want to add this
    // github release to the environment, we just want a mise binary
    // to install other tools
    let mut fake_env = UpEnvironment::new();

    let subhandler = progress_handler.subhandler(&"mise: ".light_black());
    gh_release.up(options, &mut fake_env, &subhandler)?;

    // Check that the mise binary is installed
    let install_path = gh_release.install_path()?;
    let install_bin = install_path.join("mise");
    if !install_bin.exists() || !is_executable(&install_bin) {
        let errmsg = "failed to install mise: binary not found".to_string();
        if fail_on_error {
            return Err(UpError::Exec(errmsg));
        } else {
            progress_handler.progress(errmsg);
            return Ok(());
        }
    }

    // Create the directory for the mise binary
    let mise_bin_dest = Path::new(mise_bin());
    if let Err(err) = std::fs::create_dir_all(
        mise_bin_dest
            .parent()
            .expect("failed to get parent of mise binary"),
    ) {
        let errmsg = format!("failed to create mise binary directory: {}", err);
        if fail_on_error {
            return Err(UpError::Exec(format!(
                "failed to create mise binary directory: {}",
                err
            )));
        } else {
            progress_handler.progress(errmsg);
            return Ok(());
        }
    }

    // Now copy the mise binary to the correct location
    if let Err(err) = std::fs::copy(&install_bin, mise_bin_dest) {
        let errmsg = format!("failed to copy mise binary: {}", err);
        if fail_on_error {
            return Err(UpError::Exec(errmsg));
        } else {
            progress_handler.progress(errmsg);
            return Ok(());
        }
    }

    if let Err(err) = cache.updated_mise() {
        progress_handler.progress(format!(
            "failed to update cache for last mise update: {}",
            err
        ));
    }

    if migrate_from_asdf {
        if let Err(err) = migrate_asdf_to_mise() {
            progress_handler.progress(format!("failed to migrate from asdf to mise: {}", err));
        }
    }

    Ok(())
}

fn migrate_asdf_to_mise() -> Result<(), UpError> {
    let asdf_path = Path::new(&data_home()).join("asdf");

    let mise_path = mise_path();
    let mise_path = Path::new(&mise_path);

    // Migrating from asdf to mise involves:
    // - move asdf/installs contents to mise/installs
    // - for go installs:
    //   - asdf 'golang' becomes mise 'go' (asdf/installs/golang => mise/installs/go)
    //   - need to move the contents of asdf/installs/go/<version>/go to mise/installs/go/<version>
    //   - need to add a `go` symlink in mise/installs/go/<version>/go => mise/installs/go/<version>
    // - for all file in asdf/shims, create a symlink in mise/shims that targets mise/bin/mise

    // First, move the asdf installs to mise
    let asdf_installs = asdf_path.join("installs");
    let mise_installs = mise_path.join("installs");
    if let Err(err) = std::fs::rename(&asdf_installs, &mise_installs) {
        return Err(UpError::Exec(format!(
            "failed to move asdf installs to mise: {}",
            err
        )));
    }

    // Create a symlink to from the asdf installs to the mise installs; use a relative path
    // for the symlink to make sure it works when the data directory is moved
    if let Err(err) = symlink(PathBuf::from("../mise/installs"), &asdf_installs) {
        return Err(UpError::Exec(format!("failed to create symlink: {}", err)));
    }

    // Now, move the go installs to the correct location
    let go_asdf_path = mise_installs.join("golang");
    let go_mise_path = mise_installs.join("go");
    if let Err(err) = std::fs::rename(&go_asdf_path, &go_mise_path) {
        return Err(UpError::Exec(format!(
            "failed to rename 'golang' to 'go': {}",
            err
        )));
    }

    // Now, move the contents of the go installs to the correct location
    // and create the symlink; we just need to list all installed versions
    let tmpdir = go_mise_path.join("TMP");
    if let Ok(entries) = glob::glob(&go_mise_path.join("*").to_string_lossy()) {
        for entry in entries.flatten().filter(|entry| entry.is_dir()) {
            let inner_go = entry.join("go");

            // Move the inner 'go' directory
            if let Err(err) = std::fs::rename(&inner_go, &tmpdir) {
                return Err(UpError::Exec(format!(
                    "failed to move 'go' directory: {}",
                    err
                )));
            }

            // Remove the outer directory
            if let Err(err) = force_remove_dir_all(&entry) {
                return Err(UpError::Exec(format!(
                    "failed to remove outer directory: {}",
                    err
                )));
            }

            // Move the inner 'go' directory back, but as the outer directory
            if let Err(err) = std::fs::rename(&tmpdir, &entry) {
                return Err(UpError::Exec(format!(
                    "failed to move 'go' directory back: {}",
                    err
                )));
            }

            // Now create an inner 'go' symlink to the outer directory
            if let Err(err) = symlink("./", &inner_go) {
                return Err(UpError::Exec(format!("failed to create symlink: {}", err)));
            }
        }
    }

    // Finally, create the shims
    let asdf_shims = asdf_path.join("shims");
    let mise_shims = mise_path.join("shims");
    let mise_bin = Path::new(mise_bin());

    if !mise_shims.exists() {
        if let Err(err) = std::fs::create_dir_all(&mise_shims) {
            return Err(UpError::Exec(format!(
                "failed to create mise shims directory: {}",
                err
            )));
        }
    }

    if let Ok(entries) = glob::glob(&asdf_shims.join("*").to_string_lossy()) {
        for entry in entries.flatten().filter(|entry| entry.is_file()) {
            let filename = match entry.file_name() {
                Some(filename) => filename.to_string_lossy().to_string(),
                None => continue,
            };

            let shim = mise_shims.join(&filename);
            if shim.exists() {
                continue;
            }

            if let Err(err) = symlink(mise_bin, &shim) {
                return Err(UpError::Exec(format!("failed to create symlink: {}", err)));
            }
        }
    }

    Ok(())
}

fn list_mise_tool_versions(
    tool: &str,
    list_type: &str,
    path: Option<PathBuf>,
) -> Result<MiseLsOutput, UpError> {
    let mut mise_list = mise_sync_command();
    mise_list.arg("ls");
    if list_type == "installed" {
        mise_list.arg("--installed");
    } else if list_type == "current" {
        mise_list.arg("--current");
    } else {
        return Err(UpError::Exec(format!("unknown list type: {}", list_type)));
    }
    mise_list.arg("--offline");
    mise_list.arg("--json");
    mise_list.arg("--quiet");
    mise_list.arg(tool);

    mise_list.stdout(std::process::Stdio::piped());
    mise_list.stderr(std::process::Stdio::null());

    if let Some(path) = path {
        mise_list.env(
            "MISE_TRUSTED_CONFIG_PATHS",
            path.to_string_lossy().to_string(),
        );
        mise_list.current_dir(path);
    }

    let output = mise_list.output().map_err(|err| {
        UpError::Exec(format!(
            "failed to list installed versions for {}: {}",
            tool, err
        ))
    })?;

    if !output.status.success() {
        return Err(UpError::Exec(format!(
            "failed to list installed versions for {} ({}): {}",
            tool,
            output.status,
            String::from_utf8_lossy(&output.stderr),
        )));
    }

    let stdout = String::from_utf8(output.stdout).unwrap();
    let versions: MiseLsOutput = match serde_json::from_str(&stdout) {
        Ok(versions) => versions,
        Err(err) => {
            return Err(UpError::Exec(format!(
                "failed to parse mise ls output: {}",
                err
            )));
        }
    };

    Ok(versions)
}

fn list_mise_installed_versions(tool: &str) -> Result<MiseLsOutput, UpError> {
    list_mise_tool_versions(tool, "installed", None)
}

fn list_mise_current_versions(tool: &str, path: PathBuf) -> Result<MiseLsOutput, UpError> {
    list_mise_tool_versions(tool, "current", Some(path))
}

fn is_mise_tool_version_installed(tool: &str, version: &str) -> bool {
    match list_mise_installed_versions(tool) {
        Ok(versions) => versions.has_version(version),
        Err(_err) => false,
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
struct MiseLsOutput {
    versions: Vec<MiseLsVersion>,
}

impl MiseLsOutput {
    fn versions(&self) -> Vec<String> {
        self.versions
            .iter()
            .filter_map(|v| match v.version.as_str() {
                "system" => None,
                _ => Some(v.version.clone()),
            })
            .collect()
    }

    fn requested_versions(&self, path: &PathBuf) -> Vec<String> {
        self.versions
            .iter()
            .filter(|v| match v.source {
                Some(ref source) => match source.path.parent() {
                    Some(ref parent) => parent == path,
                    None => false,
                },
                None => false,
            })
            .filter_map(|v| match v.requested_version {
                Some(ref version) if version != "system" => Some(version.clone()),
                _ => None,
            })
            .collect()
    }

    fn has_version(&self, version: &str) -> bool {
        self.versions.iter().any(|v| v.version == version)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct MiseLsVersion {
    version: String,
    requested_version: Option<String>,
    source: Option<MiseLsVersionSource>,
    // install_path: String,
    // symlinked_to: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MiseLsVersionSource {
    #[serde(rename = "type")]
    version_type: String,
    path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UpConfigMiseParams {
    pub tool_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct UpConfigMise {
    /// The name of the tool to install.
    #[serde(skip)]
    pub tool: String,

    /// The real name of the tool, if different than tool
    #[serde(skip)]
    pub tool_real_name: Option<String>,

    /// The URL to use to install the tool.
    #[serde(skip)]
    pub tool_url: Option<String>,

    /// The URL passed as parameter to override the location
    /// of the tool; this is stored as a separate parameter
    /// to make sure it can be dumped when looking at the
    /// configuration.
    #[serde(rename = "url", default, skip_serializing_if = "Option::is_none")]
    pub override_tool_url: Option<String>,

    /// The version of the tool to install, as specified in the config file.
    pub version: String,

    /// Whether to always upgrade the tool or use the latest matching
    /// already installed version.
    #[serde(default, skip_serializing_if = "cache_utils::is_false")]
    pub upgrade: bool,

    /// A list of directories to install the tool for.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub dirs: BTreeSet<String>,

    /// A list of functions to run to detect the version of the tool.
    /// The functions will be called with the following parameters:
    /// - tool: the name of the tool
    /// - path: the path currently being searched
    ///   The functions should return the version of the tool if found, or None
    ///   if not found.
    ///   The functions will be called in order, and the first one to return a
    ///   version will be used.
    ///   If no function returns a version, the version will be considered not
    ///   found.
    #[serde(skip)]
    detect_version_funcs: Vec<DetectVersionFunc>,

    /// A list of functions to run after installing a version of the tool.
    /// This is useful for tools that require additional steps after installing
    /// a version, such as installing plugins or running post-install scripts.
    /// The functions will be called with the following parameters:
    /// - progress_handler: a progress handler to use to report progress
    /// - tool: the name of the tool
    /// - versions: MiseToolUpVersion objects describing the versions that were
    ///             up-ed, with the following fields:
    ///     - version: the version of the tool that was installed
    ///     - installed: whether the tool was installed or already installed
    ///     - paths: the relative paths where the tool version was installed
    #[serde(skip)]
    post_install_funcs: Vec<PostInstallFunc>,

    /// The actual version of the tool that has to be installed.
    #[serde(skip)]
    actual_version: OnceCell<String>,

    /// The actual versions of the tool that have been installed.
    /// This is only used when the version is "auto".
    #[serde(skip)]
    actual_versions: OnceCell<BTreeMap<String, BTreeSet<String>>>,

    /// The configuration value that was used to create this object.
    #[serde(skip)]
    config_value: Option<ConfigValue>,

    /// Whether the up operation succeeded. If unset, the operation has not
    /// been attempted yet.
    #[serde(skip)]
    up_succeeded: OnceCell<bool>,

    /// The tool object representing the dependencies for this mise tool.
    #[serde(skip)]
    deps: OnceCell<Box<UpConfigTool>>,
}

impl UpConfigMise {
    pub fn new_any_version(tool: &str) -> Self {
        Self {
            tool: tool.to_string(),
            version: "*".to_string(),
            ..Default::default()
        }
    }

    pub fn new(tool: &str, version: &str, dirs: BTreeSet<String>, upgrade: bool) -> Self {
        Self {
            tool: tool.to_string(),
            version: version.to_string(),
            upgrade,
            dirs: dirs.clone(),
            ..Default::default()
        }
    }

    pub fn add_detect_version_func(&mut self, func: DetectVersionFunc) {
        self.detect_version_funcs.push(func);
    }

    pub fn add_post_install_func(&mut self, func: PostInstallFunc) {
        self.post_install_funcs.push(func);
    }

    fn new_from_auto(&self, version: &str, dirs: BTreeSet<String>) -> Self {
        UpConfigMise {
            tool: self.tool.clone(),
            tool_url: self.tool_url.clone(),
            version: version.to_string(),
            dirs: dirs.clone(),
            ..UpConfigMise::default()
        }
    }

    pub fn from_config_value(tool: &str, config_value: Option<&ConfigValue>) -> Self {
        Self::from_config_value_with_params(tool, config_value, UpConfigMiseParams::default())
    }

    pub fn from_config_value_with_params(
        tool: &str,
        config_value: Option<&ConfigValue>,
        params: UpConfigMiseParams,
    ) -> Self {
        let mut version = "latest".to_string();
        let mut upgrade = false;
        let mut dirs = BTreeSet::new();
        let mut override_tool_url = None;

        if let Some(config_value) = config_value {
            if let Some(value) = config_value.as_str() {
                version = value.to_string();
            } else if let Some(value) = config_value.as_float() {
                version = value.to_string();
            } else if let Some(value) = config_value.as_integer() {
                version = value.to_string();
            } else {
                if let Some(value) = config_value.get_as_str_forced("version") {
                    version = value.to_string();
                }

                if let Some(value) = config_value.get_as_str("dir") {
                    dirs.insert(
                        PathBuf::from(value)
                            .normalize()
                            .to_string_lossy()
                            .to_string(),
                    );
                } else if let Some(array) = config_value.get_as_array("dir") {
                    for value in array {
                        if let Some(value) = value.as_str_forced() {
                            dirs.insert(
                                PathBuf::from(value)
                                    .normalize()
                                    .to_string_lossy()
                                    .to_string(),
                            );
                        }
                    }
                }

                if let Some(url) = config_value.get_as_str_forced("url") {
                    let set_override = match &params.tool_url {
                        None => true,
                        Some(tool_url) => url != *tool_url,
                    };
                    if set_override {
                        override_tool_url = Some(url.to_string());
                    }
                }

                if let Some(value) = config_value.get_as_bool_forced("upgrade") {
                    upgrade = value;
                }
            }
        }

        let (tool, tool_real_name, tool_url) = match &override_tool_url {
            Some(url) => {
                let tool_real_name = Some(tool.to_string());

                // Hash the URL into sha256
                let mut hasher = Sha256::new();
                hasher.update(url.as_bytes());
                let hash = format!("{:x}", hasher.finalize());
                let short_hash = &hash[0..8];

                let tool = format!("{}-{}", tool, short_hash);
                let tool_url = Some(url.to_string());

                (tool, tool_real_name, tool_url)
            }
            None => (tool.to_string(), None, params.tool_url.clone()),
        };

        UpConfigMise {
            tool,
            tool_real_name,
            tool_url,
            override_tool_url,
            version,
            upgrade,
            dirs,
            config_value: config_value.cloned(),
            ..UpConfigMise::default()
        }
    }

    pub fn name(&self) -> String {
        match &self.tool_real_name {
            Some(tool) => tool.to_string(),
            None => self.tool.clone(),
        }
    }

    fn update_cache(
        &self,
        environment: &mut UpEnvironment,
        progress_handler: &dyn ProgressHandler,
    ) {
        let version = match self.version() {
            Ok(version) => version,
            Err(_err) => return,
        };

        progress_handler.progress("updating cache".to_string());

        let cache = MiseOperationCache::get();
        if let Err(err) = cache.add_installed(&self.tool, &version, self.tool_real_name.as_deref())
        {
            progress_handler.progress(format!("failed to update tool cache: {}", err));
            return;
        }

        // Update environment
        let mut dirs = self.dirs.clone();
        if dirs.is_empty() {
            dirs.insert("".to_string());
        }

        environment.add_version(
            &self.tool,
            self.tool_real_name.as_deref(),
            &version,
            dirs.clone(),
        );

        progress_handler.progress("updated cache".to_string());
    }

    pub fn up(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        if self.up_succeeded.get().is_some() {
            return Err(UpError::Exec("up operation already attempted".to_string()));
        }

        let result = self.run_up(options, environment, progress_handler);
        if let Err(err) = self.up_succeeded.set(result.is_ok()) {
            omni_warning!(format!("failed to record status of up operation: {}", err));
        }

        result
    }

    pub fn commit(&self, _options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        let versions = if let Some(version) = self.actual_version.get() {
            vec![version]
        } else if let Some(versions) = self.actual_versions.get() {
            versions.iter().map(|(version, _)| version).collect()
        } else {
            return Err(UpError::Exec("failed to get version".to_string()));
        };

        let cache = MiseOperationCache::get();
        for version in versions.iter() {
            if let Err(err) = cache.add_required_by(env_version_id, &self.tool, version) {
                return Err(UpError::Cache(err.to_string()));
            }
        }

        Ok(())
    }

    pub fn was_upped(&self) -> bool {
        matches!(self.up_succeeded.get(), Some(true))
    }

    fn run_up(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        progress_handler.init(format!("{} ({}):", self.name(), self.version).light_blue());

        // Make sure that dependencies are installed
        let subhandler = progress_handler.subhandler(&"deps: ".light_black());
        self.deps().up(options, environment, &subhandler)?;
        update_dynamic_env_for_command_from_env(".", environment);

        if let Err(err) = install_mise(options, progress_handler) {
            progress_handler.error();
            return Err(err);
        }

        if let Err(err) = self.install_plugin(progress_handler) {
            progress_handler.error();
            return Err(err);
        }

        if self.version == "auto" {
            return self.run_up_auto(options, environment, progress_handler);
        }

        match self.resolve_and_install_version(options, progress_handler) {
            Ok(installed) => {
                let version = self.version()?;

                self.update_cache(environment, progress_handler);

                if !self.post_install_funcs.is_empty() {
                    let post_install_versions = vec![MiseToolUpVersion {
                        version: version.clone(),
                        dirs: if self.dirs.is_empty() {
                            vec!["".to_string()].into_iter().collect()
                        } else {
                            self.dirs.clone()
                        },
                        installed,
                    }];

                    let post_install_func_args = PostInstallFuncArgs {
                        config_value: self.config_value.clone(),
                        tool: self.tool.clone(),
                        tool_real_name: self.name(),
                        requested_version: self.version.clone(),
                        versions: post_install_versions,
                    };

                    for func in self.post_install_funcs.iter() {
                        if let Err(err) = func(
                            options,
                            environment,
                            progress_handler,
                            &post_install_func_args,
                        ) {
                            progress_handler.error_with_message(format!("error: {}", err));
                            return Err(err);
                        }
                    }
                }

                let msg = if installed {
                    format!("{} {} installed", self.name(), version).green()
                } else {
                    format!("{} {} already installed", self.name(), version).light_black()
                };
                progress_handler.success_with_message(msg);

                Ok(())
            }
            Err(err) => {
                progress_handler.error_with_message(format!("error: {}", err));
                Err(err)
            }
        }
    }

    fn run_up_auto(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        progress_handler.progress("detecting required versions and paths".to_string());

        let mut detected_versions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        // Get the current directory
        let current_dir = std::env::current_dir().expect("failed to get current directory");

        let mut search_dirs = self.dirs.clone();
        if search_dirs.is_empty() {
            search_dirs.insert("".to_string());
        }

        let mut detect_version_funcs = self.detect_version_funcs.clone();
        detect_version_funcs.push(detect_version_from_mise);
        detect_version_funcs.push(detect_version_from_asdf_version_file);
        detect_version_funcs.push(detect_version_from_tool_version_file);

        for search_dir in search_dirs.iter() {
            // For safety, we remove any leading slashes from the search directory,
            // as we only want to search in the workdir
            let mut search_dir = search_dir.clone();
            while search_dir.starts_with('/') {
                search_dir.remove(0);
            }

            // Append the search directory to the current directory, since we are
            // at the root of the workdir
            let search_path = current_dir.join(search_dir);

            for entry in WalkDir::new(&search_path)
                .follow_links(true)
                .into_iter()
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    if !entry.file_type().is_dir() {
                        return None;
                    }

                    // Get the path of the entry after search_dir
                    let entry_path = entry.path().strip_prefix(&search_path).ok()?;

                    // Ignore the `vendor` directories in the relative path
                    if entry_path.components().any(|component| {
                        component == std::path::Component::Normal("vendor".as_ref())
                    }) {
                        return None;
                    }

                    Some(entry)
                })
            {
                for detect_version_func in detect_version_funcs.iter() {
                    if let Some(detected_version) =
                        detect_version_func(self.tool.clone(), entry.path().to_path_buf())
                    {
                        let mut dir = entry
                            .path()
                            .strip_prefix(&current_dir)
                            .expect("failed to strip prefix")
                            .to_string_lossy()
                            .to_string();
                        while dir.starts_with('/') {
                            dir.remove(0);
                        }
                        while dir.ends_with('/') {
                            dir.pop();
                        }

                        progress_handler.progress(format!(
                            "detected required version {} {}",
                            detected_version,
                            if dir.is_empty() {
                                "at root".to_string()
                            } else {
                                format!("in {}", dir)
                            }
                        ));

                        if let Some(dirs) = detected_versions.get_mut(&detected_version) {
                            dirs.insert(dir);
                        } else {
                            let mut dirs = BTreeSet::new();
                            dirs.insert(dir);
                            detected_versions.insert(detected_version.to_string(), dirs);
                        }

                        break;
                    }
                }
            }
        }

        if detected_versions.is_empty() {
            progress_handler.success_with_message("no version detected".to_string());
            return Ok(());
        }

        let mut installed_versions = Vec::new();
        let mut already_installed_versions = Vec::new();
        let mut all_versions = BTreeMap::new();

        for (version, dirs) in detected_versions.iter() {
            let mise = self.new_from_auto(version, dirs.clone());
            let installed = match mise.resolve_and_install_version(options, progress_handler) {
                Ok(installed) => installed,
                Err(err) => {
                    progress_handler.error_with_message(format!("error: {}", err));
                    return Err(err);
                }
            };

            let version = mise.version().expect("failed to get version");
            all_versions.insert(version.clone(), dirs.clone());
            if installed {
                installed_versions.push(version.clone());
            } else {
                already_installed_versions.push(version.clone());
            }

            mise.update_cache(environment, progress_handler);
        }

        self.actual_versions
            .set(all_versions.clone())
            .expect("failed to set installed versions");

        if !self.post_install_funcs.is_empty() {
            let post_install_versions = all_versions
                .iter()
                .map(|(version, dirs)| MiseToolUpVersion {
                    version: version.clone(),
                    dirs: dirs.clone(),
                    installed: installed_versions.contains(version),
                })
                .collect::<Vec<MiseToolUpVersion>>();

            let post_install_func_args = PostInstallFuncArgs {
                config_value: self.config_value.clone(),
                tool: self.tool.clone(),
                tool_real_name: self.name(),
                requested_version: self.version.clone(),
                versions: post_install_versions,
            };

            for func in self.post_install_funcs.iter() {
                if let Err(err) = func(
                    options,
                    environment,
                    progress_handler,
                    &post_install_func_args,
                ) {
                    progress_handler.error_with_message(format!("error: {}", err));
                    return Err(err);
                }
            }
        }

        let mut msgs = Vec::new();

        if !installed_versions.is_empty() {
            msgs.push(
                format!(
                    "{} {} installed",
                    self.name(),
                    installed_versions
                        .iter()
                        .map(|version| version.to_string())
                        .collect::<Vec<String>>()
                        .join(", ")
                )
                .green(),
            );
        }

        if !already_installed_versions.is_empty() {
            msgs.push(
                format!(
                    "{} {} already installed",
                    self.name(),
                    already_installed_versions
                        .iter()
                        .map(|version| version.to_string())
                        .collect::<Vec<String>>()
                        .join(", ")
                )
                .light_black(),
            );
        }

        progress_handler.success_with_message(msgs.join(", "));

        Ok(())
    }

    pub fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        self.deps().down(progress_handler)
    }

    fn list_versions(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<MisePluginVersions, UpError> {
        let cache = MiseOperationCache::get();
        let cached_versions = if options.read_cache {
            if let Some(versions) = cache.get_mise_plugin_versions(&self.tool) {
                let versions = versions.clone();
                let config = global_config();
                let expire = config.cache.mise.plugin_versions_expire;
                if !versions.is_stale(expire) {
                    progress_handler.progress("using cached version list".light_black());
                    return Ok(versions);
                }
                Some(versions)
            } else {
                None
            }
        } else {
            None
        };

        progress_handler.progress("refreshing versions list".to_string());
        match self.list_versions_from_plugin(progress_handler) {
            Ok(versions) => {
                if options.write_cache {
                    progress_handler.progress("updating cache with version list".to_string());
                    if let Err(err) = cache.set_mise_plugin_versions(&self.tool, versions.clone()) {
                        progress_handler.progress(format!("failed to update cache: {}", err));
                    }
                }

                Ok(versions)
            }
            Err(err) => {
                if let Some(cached_versions) = cached_versions {
                    progress_handler.progress(format!(
                        "{}; {}",
                        format!("error refreshing version list: {}", err).red(),
                        "using cached data".light_black()
                    ));
                    Ok(cached_versions)
                } else {
                    Err(err)
                }
            }
        }
    }

    fn list_versions_from_plugin(
        &self,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<MisePluginVersions, UpError> {
        self.update_plugin(progress_handler)?;

        progress_handler.progress(format!("listing available versions for {}", self.name()));

        let mut mise_list_all = mise_sync_command();
        mise_list_all.arg("ls-remote");
        mise_list_all.arg(self.tool.clone());

        let output = mise_list_all.output().map_err(|err| {
            UpError::Exec(format!(
                "failed to list versions for {}: {}",
                self.name(),
                err
            ))
        })?;

        if !output.status.success() {
            return Err(UpError::Exec(format!(
                "failed to list versions for {} (exit: {}): {}",
                self.name(),
                output.status,
                String::from_utf8_lossy(&output.stderr),
            )));
        }

        let stdout = String::from_utf8(output.stdout).unwrap();
        let versions = stdout
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect::<Vec<String>>();

        Ok(MisePluginVersions::new(versions))
    }

    fn list_installed_versions_from_plugin(
        &self,
        _progress_handler: &dyn ProgressHandler,
    ) -> Result<MisePluginVersions, UpError> {
        let versions = list_mise_installed_versions(&self.tool)?;
        Ok(MisePluginVersions::new(versions.versions()))
    }

    fn resolve_version(&self, versions: &MisePluginVersions) -> Result<String, UpError> {
        self.resolve_version_from_str(&self.version, versions)
    }

    fn latest_version(&self, versions: &MisePluginVersions) -> Result<String, UpError> {
        let version_str = self.resolve_version_from_str("latest", versions)?;
        Ok(VersionParser::parse(&version_str)
            .expect("failed to parse version string")
            .major()
            .to_string())
    }

    fn resolve_version_from_str(
        &self,
        match_version: &str,
        versions: &MisePluginVersions,
    ) -> Result<String, UpError> {
        let matcher = VersionMatcher::new(match_version);

        let version = versions.get(&matcher).ok_or_else(|| {
            UpError::Exec(format!(
                "no {} version found matching {}",
                self.name(),
                match_version,
            ))
        })?;

        Ok(version)
    }

    pub fn version(&self) -> Result<String, UpError> {
        self.actual_version
            .get()
            .map(|version| version.to_string())
            .ok_or_else(|| UpError::Exec("actual version not set".to_string()))
    }

    fn is_plugin_installed(&self) -> bool {
        let mut mise_plugin_list = mise_sync_command();
        mise_plugin_list.arg("plugins");
        mise_plugin_list.arg("ls");
        mise_plugin_list.stderr(std::process::Stdio::null());

        if let Ok(output) = mise_plugin_list.output() {
            if output.status.success() {
                let stdout = String::from_utf8(output.stdout).unwrap();
                return stdout.lines().any(|line| line.trim() == self.tool);
            }
        }

        false
    }

    fn install_plugin(&self, progress_handler: &dyn ProgressHandler) -> Result<(), UpError> {
        if self.tool_url.is_none() {
            // No need to install default plugins with mise, we only install
            // custom plugins that have a provided url
            return Ok(());
        }

        if self.is_plugin_installed() {
            return Ok(());
        }

        progress_handler.progress(format!("installing {} plugin", self.tool));

        let mut mise_plugin_add = mise_async_command();
        mise_plugin_add.arg("plugins");
        mise_plugin_add.arg("install");
        mise_plugin_add.arg(self.tool.clone());
        if let Some(tool_url) = &self.tool_url {
            mise_plugin_add.arg(tool_url.clone());
        }

        run_progress(
            &mut mise_plugin_add,
            Some(progress_handler),
            RunConfig::default(),
        )
    }

    fn update_plugin(&self, progress_handler: &dyn ProgressHandler) -> Result<(), UpError> {
        if self.tool_url.is_none() {
            // No need to update default plugins with mise, we only update
            // custom plugins that have a provided url
            return Ok(());
        }

        if !MiseOperationCache::get().should_update_mise_plugin(&self.tool) {
            return Ok(());
        }

        progress_handler.progress(format!("updating {} plugin", self.tool));

        let mut mise_plugin_update = mise_async_command();
        mise_plugin_update.arg("plugins");
        mise_plugin_update.arg("update");
        mise_plugin_update.arg(self.tool.clone());

        run_progress(
            &mut mise_plugin_update,
            Some(progress_handler),
            RunConfig::default(),
        )?;

        // Update the cache
        let cache = MiseOperationCache::get();
        if let Err(err) = cache.updated_mise_plugin(&self.tool) {
            return Err(UpError::Cache(err.to_string()));
        }

        Ok(())
    }

    fn is_version_installed(&self, version: &str) -> bool {
        is_mise_tool_version_installed(&self.tool, version)
    }

    fn upgrade_version(&self, options: &UpOptions) -> bool {
        self.upgrade || options.upgrade || config(".").up_command.upgrade
    }

    fn resolve_and_install_version(
        &self,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<bool, UpError> {
        let mut versions = None;

        // If the options do not include upgrade, then we can try using
        // an already-installed version if any matches the requirements
        if !self.upgrade_version(options) {
            if let Ok(installed_versions) =
                self.list_installed_versions_from_plugin(progress_handler)
            {
                let resolve_str = match self.version.as_str() {
                    "latest" => {
                        let list_versions = self.list_versions(options, progress_handler)?;
                        versions = Some(list_versions.clone());
                        let latest = self.latest_version(&list_versions)?;
                        progress_handler.progress(
                            format!("considering installed versions matching {}", latest)
                                .light_black(),
                        );
                        latest
                    }
                    _ => self.version.clone(),
                };

                match self.resolve_version_from_str(&resolve_str, &installed_versions) {
                    Ok(installed_version) => {
                        progress_handler.progress("found matching installed version".to_string());
                        return self.install_version(&installed_version, options, progress_handler);
                    }
                    Err(_err) => {
                        progress_handler.progress("no matching version installed".to_string());
                    }
                }
            }
        }

        let versions = match versions {
            Some(versions) => versions,
            None => self.list_versions(options, progress_handler)?,
        };
        let version = match self.resolve_version(&versions) {
            Ok(available_version) => available_version,
            Err(err) => {
                // If the versions are not fresh of now, and we failed to
                // resolve the version, we should try to refresh the
                // version list and try again
                if options.read_cache && !versions.is_fresh() {
                    progress_handler.progress("no matching version found in cache".to_string());

                    let versions = self.list_versions(
                        &UpOptions {
                            read_cache: false,
                            ..options.clone()
                        },
                        progress_handler,
                    )?;

                    self.resolve_version(&versions).inspect_err(|err| {
                        progress_handler.error_with_message(err.message());
                    })?
                } else {
                    progress_handler.error_with_message(err.message());
                    return Err(err);
                }
            }
        };

        // Try installing the version found
        let mut install_version = self.install_version(&version, options, progress_handler);
        if install_version.is_err() && !options.fail_on_upgrade {
            // If we get here and there is an issue installing the version,
            // list all installed versions and check if one of those could
            // fit the requirement, in which case we can fallback to it
            let installed_versions = self.list_installed_versions_from_plugin(progress_handler)?;
            match self.resolve_version(&installed_versions) {
                Ok(installed_version) => {
                    progress_handler.progress(format!(
                        "falling back to installed version {}",
                        installed_version.light_yellow()
                    ));
                    install_version =
                        self.install_version(&installed_version, options, progress_handler);
                }
                Err(_err) => {}
            }
        }

        install_version
    }

    fn install_version(
        &self,
        version: &str,
        _options: &UpOptions,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<bool, UpError> {
        let installed = if self.is_version_installed(version) {
            progress_handler.progress(format!("using {} {}", self.name(), version.light_yellow()));

            false
        } else {
            progress_handler.progress(format!(
                "installing {} {}",
                self.name(),
                version.light_yellow()
            ));

            let mut mise_install = mise_async_command();
            mise_install.arg("install");
            mise_install.arg(format!("{}@{}", self.tool, version));

            run_progress(
                &mut mise_install,
                Some(progress_handler),
                RunConfig::default(),
            )?;

            true
        };

        self.actual_version.set(version.to_string()).map_err(|_| {
            let errmsg = "failed to set actual version".to_string();
            UpError::Exec(errmsg)
        })?;

        Ok(installed)
    }

    pub fn data_paths(&self) -> Vec<PathBuf> {
        let workdir = workdir(".");

        let wd_data_path = match workdir.data_path() {
            Some(wd_data_path) => wd_data_path,
            None => return vec![],
        };

        let mut dirs_per_version = BTreeMap::new();

        if let Some(version) = self.actual_version.get() {
            let dirs = match self.dirs.is_empty() {
                true => vec!["".to_string()].into_iter().collect(),
                false => self.dirs.clone(),
            };

            dirs_per_version.insert(version.clone(), dirs);
        }

        if let Some(versions) = self.actual_versions.get() {
            for (version, dirs) in versions.iter() {
                dirs_per_version.insert(version.clone(), dirs.clone());
            }
        }

        let mut data_paths = BTreeSet::new();
        let tool_data_path = wd_data_path.join(&self.tool);
        for (version, dirs) in dirs_per_version.iter() {
            let version_data_path = tool_data_path.join(version);

            for dir in dirs {
                let hashed_dir = data_path_dir_hash(dir);
                data_paths.insert(version_data_path.join(&hashed_dir));
            }
        }

        // Add also all data paths from dependencies
        data_paths.extend(self.deps().data_paths());

        data_paths.into_iter().collect()
    }

    pub fn cleanup(progress_handler: &dyn ProgressHandler) -> Result<Option<String>, UpError> {
        let mut uninstalled = Vec::new();

        let cache = MiseOperationCache::get();
        cache
            .cleanup(|tool, version| {
                if is_mise_tool_version_installed(tool, version) {
                    progress_handler.progress(format!("uninstalling {} {}", tool, version));

                    let mut mise_uninstall = mise_async_command();
                    mise_uninstall.arg("uninstall");
                    mise_uninstall.arg(format!("{}@{}", tool, version));

                    if let Err(err) = run_progress(
                        &mut mise_uninstall,
                        Some(progress_handler),
                        RunConfig::default(),
                    ) {
                        progress_handler.error_with_message(format!(
                            "failed to uninstall {} {}",
                            tool, version,
                        ));
                        return Err(CacheManagerError::Other(err.to_string()));
                    }

                    uninstalled.push(format!("{}:{}", tool, version));
                }

                Ok(())
            })
            .map_err(|err| UpError::Cache(err.to_string()))?;

        if uninstalled.is_empty() {
            Ok(None)
        } else {
            let uninstalled = uninstalled
                .iter()
                .map(|tool| tool.light_blue().to_string())
                .collect::<Vec<_>>();
            Ok(Some(format!("uninstalled {}", uninstalled.join(", "))))
        }
    }

    fn deps(&self) -> &UpConfigTool {
        self.deps
            .get_or_init(|| {
                Box::new(UpConfigTool::Any(vec![
                    self.deps_using_homebrew(),
                    self.deps_using_nix(),
                ]))
            })
            .as_ref()
    }

    fn deps_using_homebrew(&self) -> UpConfigTool {
        let mut homebrew_install = vec![
            HomebrewInstall::new_formula("autoconf"),
            // HomebrewInstall::new_formula("automake"),
            HomebrewInstall::new_formula("coreutils"),
            HomebrewInstall::new_formula("curl"),
            // HomebrewInstall::new_formula("libtool"),
            HomebrewInstall::new_formula("libyaml"),
            HomebrewInstall::new_formula("openssl@3"),
            HomebrewInstall::new_formula("readline"),
            // HomebrewInstall::new_formula("unixodbc"),
        ];

        match self.name().as_str() {
            "python" => {
                homebrew_install.extend(vec![
                    HomebrewInstall::new_formula("pkg-config"),
                    // HomebrewInstall::new_formula("sqlite"),
                    // HomebrewInstall::new_formula("xz"),
                ]);
            }
            "rust" => {
                homebrew_install.extend(vec![
                    HomebrewInstall::new_formula("libgit2"),
                    HomebrewInstall::new_formula("libssh2"),
                    HomebrewInstall::new_formula("llvm"),
                    HomebrewInstall::new_formula("pkg-config"),
                ]);
            }
            _ => {}
        }

        UpConfigTool::Homebrew(UpConfigHomebrew {
            install: homebrew_install,
            tap: vec![],
        })
    }

    fn deps_using_nix(&self) -> UpConfigTool {
        let mut nix_packages = vec!["gawk", "gnused", "openssl", "readline"];

        match self.tool.as_str() {
            "python" => {
                nix_packages.extend(vec![
                    "bzip2",
                    "gcc",
                    "gdbm",
                    "gnumake",
                    "libffi",
                    "ncurses",
                    "pkg-config",
                    "sqlite",
                    "xz",
                    "zlib",
                ]);
            }
            "ruby" => {
                nix_packages.extend(vec!["libyaml"]);
            }
            _ => {}
        }

        UpConfigTool::Nix(UpConfigNix::new_from_packages(
            nix_packages.into_iter().map(|p| p.to_string()).collect(),
        ))
    }
}

fn detect_version_from_mise(tool_name: String, path: PathBuf) -> Option<String> {
    // Check that there is at least one of the known mise configuration files
    // in the directory
    static MISE_CONFIG_FILES: [&str; 7] = [
        "mise.toml",
        ".mise.toml",
        "mise/config.toml",
        ".mise/config.toml",
        "config/mise.toml",
        ".config/mise.toml",
        ".tool-versions",
    ];

    // Skip if none of the known configuration files are present
    if !MISE_CONFIG_FILES
        .iter()
        .any(|file| path.join(file).exists())
    {
        return None;
    }

    match list_mise_current_versions(&tool_name, path.clone()) {
        Ok(v) => {
            let versions = v.requested_versions(&path);
            if versions.is_empty() {
                return None;
            }

            let version = versions[0].clone();
            Some(version)
        }
        Err(_err) => None,
    }
}

fn detect_version_from_asdf_version_file(tool_name: String, path: PathBuf) -> Option<String> {
    let version_file_path = path.join(".tool-versions");
    if !version_file_path.exists() || version_file_path.is_dir() {
        return None;
    }

    // Read the contents of the file
    match std::fs::read_to_string(&version_file_path) {
        Ok(contents) => {
            let tool_name = tool_name.to_lowercase();

            // Read line by line
            for line in contents.lines() {
                // Trim all leading and trailing whitespaces
                let line = line.trim();

                // Go to next line if the line does not start by the tool name
                if !line.starts_with(&tool_name) {
                    continue;
                }

                // Split the line by whitespace
                let mut parts = line.split_whitespace();

                // Remove first entry
                parts.next();

                // Find the first part that contains only digits and dots, starting with a digit;
                // any other version format is not supported by omni
                for part in parts {
                    if part.chars().all(|c| c.is_ascii_digit() || c == '.')
                        && part.starts_with(|c: char| c.is_ascii_digit())
                    {
                        return Some(part.to_string());
                    }
                }
            }
        }
        Err(_err) => {}
    };

    None
}

fn detect_version_from_tool_version_file(tool_name: String, path: PathBuf) -> Option<String> {
    let tool_name = tool_name.to_lowercase();
    let version_file_prefixes = match tool_name.as_str() {
        "golang" => vec!["go", "golang"],
        "nodejs" => vec!["node", "nodejs"],
        _ => vec![tool_name.as_str()],
    };

    for version_file_prefix in version_file_prefixes {
        let version_file_path = path.join(format!(".{}-version", version_file_prefix));
        if !version_file_path.exists() || version_file_path.is_dir() {
            continue;
        }

        // Read the contents of the file
        match std::fs::read_to_string(&version_file_path) {
            Ok(contents) => {
                // Strip contents of all leading or trailing whitespaces
                let version = contents.trim();
                if !version.is_empty() {
                    return Some(version.to_string());
                }
            }
            Err(_err) => {}
        };
    }

    None
}

#[derive(Debug, Clone)]
pub struct MiseToolUpVersion {
    pub version: String,
    pub dirs: BTreeSet<String>,

    #[allow(dead_code)]
    pub installed: bool,
}