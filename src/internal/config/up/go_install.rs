use std::collections::BTreeSet;
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

use itertools::Itertools;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use time::macros::format_description;
use time::PrimitiveDateTime;
use tokio::process::Command as TokioCommand;

use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::cache::up_environments::UpVersionParams;
use crate::internal::cache::GoInstallOperationCache;
use crate::internal::cache::GoInstallVersions;
use crate::internal::config::config;
use crate::internal::config::global_config;
use crate::internal::config::up::mise_tool_path;
use crate::internal::config::up::utils::cleanup_path;
use crate::internal::config::up::utils::directory::force_remove_all;
use crate::internal::config::up::utils::directory::safe_rename;
use crate::internal::config::up::utils::get_command_output;
use crate::internal::config::up::utils::progress_handler::ProgressHandler;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::utils::VersionMatcher;
use crate::internal::config::up::utils::VersionParser;
use crate::internal::config::up::utils::VersionParserOptions;
use crate::internal::config::up::UpConfigGolang;
use crate::internal::config::up::UpConfigTool;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::env::data_home;
use crate::internal::env::tmpdir_cleanup_prefix;
use crate::internal::user_interface::StringColor;

cfg_if::cfg_if! {
    if #[cfg(test)] {
        fn go_install_bin_path() -> PathBuf {
            PathBuf::from(data_home()).join("go-install")
        }
    } else {
        use once_cell::sync::Lazy;

        static GO_INSTALL_BIN_PATH: Lazy<PathBuf> = Lazy::new(|| PathBuf::from(data_home()).join("go-install"));

        fn go_install_bin_path() -> PathBuf {
            GO_INSTALL_BIN_PATH.clone()
        }
    }
}

pub fn go_install_tool_path(package: &str, version: &str) -> PathBuf {
    go_install_bin_path().join(package).join(version)
}

// ============================================================================
// UpConfigGoInstalls - Wrapper for polymorphic input handling
// ============================================================================
//
// Supports the following input formats:
// 1. String: "github.com/owner/tool@v1.0.0" -> single tool
// 2. Array: ["tool1", "tool2"] -> multiple tools
// 3. Object with "path" key: {path: "tool", version: "1.0"} -> single tool
// 4. Object without "path" key: {"tool1": "v1", "tool2": {version: "v2"}} -> map-to-vec
//
// Uses compote's transparent + allow_single + allow_map to handle all formats.
// ============================================================================
#[derive(Debug, Clone, Default, compote::Config)]
#[compote(transparent)]
pub struct UpConfigGoInstalls {
    #[compote(default, allow_single, allow_map(order_by = "path"))]
    tools: Vec<UpConfigGoInstall>,
}

// Stub Deserialize implementation for compatibility with UpConfigTool enum.
// Actual deserialization is done through compote's FromContextValue.
impl<'de> Deserialize<'de> for UpConfigGoInstalls {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let _ = serde::de::IgnoredAny::deserialize(deserializer)?;
        Ok(Self::default())
    }
}

impl UpConfigGoInstalls {
    pub fn up(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        if self.tools.len() == 1 {
            progress_handler.init(self.tools[0].desc().light_blue());
        } else {
            progress_handler.init("go install:".light_blue());
            if self.tools.is_empty() {
                progress_handler.error_with_message("no import path".to_string());
                return Err(UpError::Config(
                    "at least one import path required".to_string(),
                ));
            }
        }

        if !global_config()
            .up_command
            .operations
            .is_operation_allowed("go-install")
        {
            let errmsg = "go-install operation is not allowed".to_string();
            progress_handler.error_with_message(errmsg.clone());
            return Err(UpError::Config(errmsg));
        }

        let go_bin = GoBin::get(options, progress_handler)?;

        let num = self.tools.len();
        for (idx, tool) in self.tools.iter().enumerate() {
            let subhandler = if self.tools.len() == 1 {
                progress_handler
            } else {
                &progress_handler.subhandler(
                    &format!(
                        "[{current:padding$}/{total:padding$}] {tool} ",
                        current = idx + 1,
                        total = num,
                        padding = format!("{num}").len(),
                        tool = tool.desc(),
                    )
                    .light_yellow(),
                )
            };
            tool.up(options, environment, subhandler, &go_bin)
                .inspect_err(|_err| {
                    progress_handler.error();
                })?;
        }

        if self.tools.len() != 1 {
            progress_handler.success_with_message(self.get_up_message());
        }

        Ok(())
    }

    pub fn commit(&self, options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        for tool in &self.tools {
            if tool.was_upped() {
                tool.commit(options, env_version_id)?;
            }
        }

        Ok(())
    }

    pub fn was_upped(&self) -> bool {
        self.tools.iter().any(|tool| tool.was_upped())
    }

    fn get_up_message(&self) -> String {
        let count: HashMap<GoInstallHandled, usize> = self
            .tools
            .iter()
            .map(|tool| tool.handling())
            .fold(HashMap::new(), |mut map, item| {
                *map.entry(item).or_insert(0) += 1;
                map
            });
        let handled: Vec<String> = self
            .tools
            .iter()
            .filter_map(|tool| match tool.handling() {
                GoInstallHandled::Handled | GoInstallHandled::Noop => Some(format!(
                    "{}@{}",
                    tool.path,
                    tool.actual_version
                        .get()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "?".to_string())
                )),
                _ => None,
            })
            .sorted()
            .collect();

        if handled.is_empty() {
            return "nothing done".to_string();
        }

        let mut numbers = vec![];

        if let Some(count) = count.get(&GoInstallHandled::Handled) {
            numbers.push(format!("{count} installed").green());
        }

        if let Some(count) = count.get(&GoInstallHandled::Noop) {
            numbers.push(format!("{count} already installed").light_black());
        }

        if numbers.is_empty() {
            return "nothing done".to_string();
        }

        format!(
            "{} {}",
            numbers.join(", "),
            format!("({})", handled.join(", ")).light_black().italic(),
        )
    }

    pub fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        if self.tools.len() == 1 {
            return self.tools[0].down(progress_handler);
        }

        progress_handler.init("go install:".light_blue());
        progress_handler.progress("updating dependencies".to_string());

        let num = self.tools.len();
        for (idx, tool) in self.tools.iter().enumerate() {
            let subhandler = progress_handler.subhandler(
                &format!(
                    "[{current:padding$}/{total:padding$}] ",
                    current = idx + 1,
                    total = num,
                    padding = format!("{num}").len(),
                )
                .light_yellow(),
            );
            tool.down(&subhandler)?;
        }

        progress_handler.success_with_message("dependencies cleaned".light_green());

        Ok(())
    }

    pub fn cleanup(progress_handler: &UpProgressHandler) -> Result<Option<String>, UpError> {
        progress_handler.init("go install:".light_blue());

        let cache = GoInstallOperationCache::get();

        // Cleanup removable tools from the database
        cache.cleanup().map_err(|err| {
            let msg = format!("failed to cleanup go install cache: {err}");
            progress_handler.progress(msg.clone());
            UpError::Cache(msg)
        })?;

        // List tools that should exist
        let expected_tools = cache.list_installed().map_err(|err| {
            let msg = format!("failed to list go-installed tools: {err}");
            progress_handler.progress(msg.clone());
            UpError::Cache(msg)
        })?;

        let expected_paths = expected_tools
            .iter()
            .map(|install| {
                go_install_bin_path()
                    .join(&install.path)
                    .join(&install.version)
            })
            .collect::<Vec<PathBuf>>();

        let (root_removed, num_removed, removed_paths) = cleanup_path(
            go_install_bin_path(),
            expected_paths,
            progress_handler,
            true,
        )?;

        if root_removed {
            return Ok(Some("removed all go install".to_string()));
        }

        if num_removed == 0 {
            return Ok(None);
        }

        // We want to go over the paths that were removed to
        // return a proper message about the go install
        // that were removed
        let removed_tools = removed_paths
            .iter()
            .filter_map(|path| {
                // Path should starts with the bin path if it is a go-install tool
                let rest_of_path = match path.strip_prefix(go_install_bin_path()) {
                    Ok(rest_of_path) => rest_of_path,
                    Err(_) => return None,
                };

                // Path should have at least 2 components after stripping the bin path
                // (1) the import path and (2) the version
                let parts = rest_of_path.components().collect::<Vec<_>>();
                if parts.len() < 2 {
                    return None;
                }

                let mut parts = parts
                    .into_iter()
                    .map(|part| part.as_os_str().to_string_lossy().to_string())
                    .collect::<Vec<String>>();

                let version = if parts.last().unwrap().starts_with('v') {
                    Some(parts.pop().unwrap())
                } else {
                    None
                };

                let path = parts.join("/");

                Some((path, version))
            })
            .collect::<Vec<_>>();

        if removed_tools.is_empty() {
            return Ok(Some(format!(
                "removed {} go-installed tool{}",
                num_removed.light_yellow(),
                if num_removed > 1 { "s" } else { "" }
            )));
        }

        let removed_tools = removed_tools
            .iter()
            .map(|(path, version)| match version {
                Some(version) => format!("{}@{}", path.light_yellow(), version.light_yellow()),
                None => format!("{} (all versions)", path.light_yellow(),),
            })
            .collect::<Vec<_>>();

        Ok(Some(format!("removed {}", removed_tools.join(", "))))
    }
}

#[derive(Debug, Serialize, Clone, Eq, PartialEq, Hash)]
pub enum GoInstallHandled {
    Handled,
    Noop,
    Unhandled,
}

#[derive(Debug, Clone, Error)]
pub enum GoInstallError {
    #[error("invalid path: {0}")]
    InvalidImportPath(String),
}

// ============================================================================
// UpConfigGoInstall - Using compote derive with post_process
// ============================================================================
//
// Input formats supported via compote attributes:
// - String: "github.com/foo@v1.0" -> {path: "github.com/foo", version: "v1.0"}
// - allow_map format: {"github.com/foo": "v1.0"} -> {path, version}
// - Object format: {path: "...", version: "...", ...}
//
// The post_process function handles:
// 1. Splitting path@version into separate path and version fields
// 2. Validating the import path format
// 3. Setting exact = true when version is set but exact wasn't explicitly provided
// ============================================================================
#[derive(Debug, Clone, compote::Config)]
#[compote(
    allow_map(key = path, scalar_as = version),
    scalar_as = "path",
    post_process = "finalize_go_install"
)]
struct UpConfigGoInstall {
    /// The path to call the `go install` command on,
    /// e.g. `github.com/owner/path`
    pub path: String,

    /// The version of the tool to install, which will be used after the `@` in the
    /// `go install` command, e.g. `github.com/owner/path@<version>`
    #[compote(default)]
    pub version: Option<String>,

    /// Whether to install the exact version specified in the `version` field;
    /// if `true`, there will be no check for the available versions and the
    /// `go install` command will be called with the version specified;
    /// if `false`, the latest version that matches the version will be installed.
    /// Defaults to `version.is_some()` (set in post_process).
    #[compote(default)]
    pub exact: bool,

    /// Whether to always upgrade the tool or use the latest matching
    /// already installed version.
    #[compote(default)]
    pub upgrade: bool,

    /// Whether to install the pre-release version of the tool
    /// if it is the most recent matching version
    #[compote(default)]
    pub prerelease: bool,

    /// Whether to allow versions containing build details
    /// (e.g. 1.2.3+build)
    #[compote(default)]
    pub build: bool,

    /// A list of directories to make the binary available for
    #[compote(default, rename = "dir", allow_single)]
    pub dirs: BTreeSet<String>,

    /// In case there was an error while parsing the configuration, this field
    /// will contain the error message
    #[compote(skip)]
    config_error: Option<String>,

    /// The actual version that was installed
    #[compote(skip)]
    actual_version: OnceCell<String>,

    /// Whether this tool was handled during the up operation
    #[compote(skip)]
    was_handled: OnceCell<GoInstallHandled>,
}

/// Post-process function for UpConfigGoInstall.
/// Handles path@version splitting, validation, and conditional default for `exact`.
fn finalize_go_install<S: compote::CustomSource, L: compote::CustomLevel>(
    config: &mut UpConfigGoInstall,
    source: &compote::ContextValue<S, L>,
    error_tracker: &mut compote::ErrorTracker,
) -> Result<(), compote::Error> {
    // Handle path@version splitting
    if let Some(at_pos) = config.path.find('@') {
        let (path, ver) = config.path.split_at(at_pos);
        let ver = &ver[1..]; // Skip the '@'

        if config.version.is_some() {
            error_tracker.record_invalid_value(
                "version should not be specified in both path@version format and version field",
            );
            config.config_error = Some(
                "version should not be specified in both path@version format and version field"
                    .to_string(),
            );
            // Still update the path to the cleaned version
            config.path = path.to_string();
            return Ok(());
        }

        // Validate the version part
        if let Err(e) = validate_go_install_version(ver) {
            error_tracker.record_invalid_value(format!("invalid version in path: {}", e));
            config.config_error = Some(e.to_string());
            config.path = path.to_string();
            return Ok(());
        }

        config.version = Some(ver.to_string());
        config.path = path.to_string();
    }

    // Validate and clean the import path format
    match validate_go_install_path(&config.path) {
        Ok(cleaned_path) => {
            config.path = cleaned_path;
        }
        Err(e) => {
            error_tracker.record_invalid_value(format!("invalid go install path: {}", e));
            config.config_error = Some(e.to_string());
            return Ok(());
        }
    }

    // Conditional default: exact = version.is_some(), only if not explicitly set
    let explicitly_set = match source {
        compote::ContextValue::Object(map, _) => map.contains_key("exact"),
        _ => false,
    };
    if !explicitly_set && config.version.is_some() {
        config.exact = true;
    }

    Ok(())
}

impl Default for UpConfigGoInstall {
    fn default() -> Self {
        UpConfigGoInstall {
            path: String::new(),
            version: None,
            exact: false,
            upgrade: false,
            prerelease: false,
            build: false,
            dirs: BTreeSet::new(),
            config_error: None,
            actual_version: OnceCell::new(),
            was_handled: OnceCell::new(),
        }
    }
}

impl UpConfigGoInstall {
    fn update_cache(
        &self,
        _options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &dyn ProgressHandler,
    ) {
        let version = match self.actual_version.get() {
            Some(version) => version,
            None => {
                progress_handler.error_with_message("version not set".to_string());
                return;
            }
        };

        progress_handler.progress("updating cache".to_string());

        if let Err(err) = GoInstallOperationCache::get().add_installed(&self.path, version) {
            progress_handler.progress(format!("failed to update github release cache: {err}"));
            return;
        }

        // Update environment
        environment.add_version(UpVersionParams {
            backend: "go-install",
            tool: &self.path,
            version,
            bin_path: "bin",
            dirs: self.dirs.clone(),
            ..UpVersionParams::default()
        });

        progress_handler.progress("updated cache".to_string());
    }

    fn short_path(&self) -> String {
        // Get the last, non-empty part of the path
        self.path
            .split('/')
            .filter(|part| !part.is_empty())
            .next_back()
            .unwrap_or("")
            .to_string()
    }

    fn desc(&self) -> String {
        if self.path.is_empty() {
            "go install:".to_string()
        } else if self.config_error.is_some() {
            format!("{}:", self.path)
        } else {
            format!(
                "{} ({}):",
                self.short_path(),
                match self.version {
                    None => "latest".to_string(),
                    Some(ref version) if version.is_empty() => "latest".to_string(),
                    Some(ref version) => version.clone(),
                }
            )
        }
    }

    pub fn up(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
        go_bin: &GoBin,
    ) -> Result<(), UpError> {
        progress_handler.init(self.desc().light_blue());

        if let Some(config_error) = &self.config_error {
            progress_handler.error_with_message(config_error.clone());
            return Err(UpError::Config(config_error.clone()));
        }

        if self.path.is_empty() {
            progress_handler.error_with_message("path is required".to_string());
            return Err(UpError::Config("path is required".to_string()));
        }

        if !global_config()
            .up_command
            .operations
            .is_go_install_source_allowed(&self.path)
        {
            let errmsg = format!("go-install source not allowed: {}", self.path);
            progress_handler.error_with_message(errmsg.clone());
            return Err(UpError::Config(errmsg));
        }

        let installed = self.resolve_and_install_version(go_bin, options, progress_handler)?;

        self.update_cache(options, environment, progress_handler);

        let version = match self.actual_version.get() {
            Some(version) => version.to_string(),
            None => "unknown".to_string(),
        };
        let msg = match installed {
            true => format!("{} installed", version.light_yellow()),
            false => format!("{version} already installed").light_black(),
        };
        progress_handler.success_with_message(msg);

        Ok(())
    }

    pub fn was_upped(&self) -> bool {
        matches!(
            self.was_handled.get(),
            Some(GoInstallHandled::Handled) | Some(GoInstallHandled::Noop)
        )
    }

    pub fn commit(&self, _options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        let version = match self.actual_version.get() {
            Some(version) => version,
            None => {
                return Err(UpError::Exec("version not set".to_string()));
            }
        };

        if let Err(err) =
            GoInstallOperationCache::get().add_required_by(env_version_id, &self.path, version)
        {
            return Err(UpError::Cache(format!(
                "failed to update go install cache: {err}"
            )));
        }

        Ok(())
    }

    pub fn down(&self, _progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        Ok(())
    }

    fn handling(&self) -> GoInstallHandled {
        match self.was_handled.get() {
            Some(handled) => handled.clone(),
            None => GoInstallHandled::Unhandled,
        }
    }

    fn upgrade_tool(&self, options: &UpOptions) -> bool {
        self.upgrade || options.upgrade || config(".").up_command.upgrade
    }

    fn exact(&self) -> bool {
        self.exact || {
            if let Some(version) = &self.version {
                is_go_pseudo_version(version)
            } else {
                false
            }
        }
    }

    fn resolve_and_install_version(
        &self,
        go_bin: &GoBin,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<bool, UpError> {
        if self.exact() {
            let version = self.version.clone().unwrap_or("latest".to_string());
            if version == "latest" {
                progress_handler.error_with_message("exact version cannot be 'latest'".to_string());
                return Err(UpError::Config(
                    "exact version cannot be 'latest'".to_string(),
                ));
            }

            let installed_versions = self.list_installed_versions(progress_handler)?;
            if installed_versions.contains(&version) {
                progress_handler.progress(format!(
                    "found matching installed version {}",
                    version.light_yellow(),
                ));

                return self.handle_installed(&version, Ok(false));
            }

            match self.install_version(go_bin, options, &version, progress_handler) {
                Ok(installed) => return self.handle_installed(&version, Ok(installed)),
                Err(err) => {
                    progress_handler.error_with_message(err.message());
                    return Err(err);
                }
            }
        }

        let mut version = "".to_string();
        let mut install_version = Err(UpError::Exec("did not even try".to_string()));
        let mut versions = None;

        // If the options do not include upgrade, then we can try using
        // an already-installed version if any matches the requirements
        if !self.upgrade_tool(options) {
            let resolve_str = match self.version.as_ref() {
                Some(version) if version != "latest" => version.to_string(),
                _ => {
                    let list_versions = self.list_versions(go_bin, options, progress_handler)?;
                    versions = Some(list_versions.clone());
                    let latest = self.latest_version(&list_versions)?;
                    progress_handler.progress(
                        format!("considering installed versions matching {latest}").light_black(),
                    );
                    latest
                }
            };

            let installed_versions = self.list_installed_versions(progress_handler)?;
            match self.resolve_version_from_str(&resolve_str, &installed_versions) {
                Ok(installed_version) => {
                    progress_handler.progress(format!(
                        "found matching installed version {}",
                        installed_version.light_yellow(),
                    ));

                    version = installed_version;
                    install_version = Ok(false);
                }
                Err(_err) => {
                    progress_handler.progress("no matching version installed".to_string());
                }
            }
        }

        if version.is_empty() {
            let versions = match versions {
                Some(versions) => versions,
                None => self.list_versions(go_bin, options, progress_handler)?,
            };
            version = match self.resolve_version(&versions.versions) {
                Ok(version) => version,
                Err(err) => {
                    // If the versions are not fresh of now, and we failed to
                    // resolve the version to install, we should try to refresh the
                    // versions list and try again
                    if options.read_cache && !versions.is_fresh() {
                        progress_handler.progress("no matching version found in cache".to_string());

                        let versions = self.list_versions(
                            go_bin,
                            &UpOptions {
                                read_cache: false,
                                ..options.clone()
                            },
                            progress_handler,
                        )?;

                        self.resolve_version(&versions.versions)
                            .inspect_err(|err| {
                                progress_handler.error_with_message(err.message());
                            })?
                    } else {
                        progress_handler.error_with_message(err.message());
                        return Err(err);
                    }
                }
            };

            // Try installing the version found
            install_version = self.install_version(go_bin, options, &version, progress_handler);
            if install_version.is_err() && !options.fail_on_upgrade {
                // If we get here and there is an issue downloading the version,
                // list all installed versions and check if one of those could
                // fit the requirement, in which case we can fallback to it
                let installed_versions = self.list_installed_versions(progress_handler)?;
                match self.resolve_version(&installed_versions) {
                    Ok(installed_version) => {
                        progress_handler.progress(format!(
                            "falling back to {}@{}",
                            self.path,
                            installed_version.light_yellow(),
                        ));

                        version = installed_version;
                        install_version = Ok(false);
                    }
                    Err(_err) => {}
                }
            }
        }

        self.handle_installed(&version, install_version)
    }

    fn handle_installed(
        &self,
        version: &str,
        installed: Result<bool, UpError>,
    ) -> Result<bool, UpError> {
        if let Ok(installed) = &installed {
            self.actual_version.set(version.to_string()).map_err(|_| {
                let errmsg = "failed to set actual version".to_string();
                UpError::Exec(errmsg)
            })?;

            if self
                .was_handled
                .set(if *installed {
                    GoInstallHandled::Handled
                } else {
                    GoInstallHandled::Noop
                })
                .is_err()
            {
                unreachable!("failed to set was_handled");
            }
        }

        installed
    }

    fn list_versions(
        &self,
        go_bin: &GoBin,
        options: &UpOptions,
        progress_handler: &UpProgressHandler,
    ) -> Result<GoInstallVersions, UpError> {
        let cache = GoInstallOperationCache::get();
        let cached_versions = if options.read_cache {
            if let Some(versions) = cache.get_versions(&self.path) {
                let versions = versions.clone();
                let config = global_config();
                let expire = config.cache.go_install.versions_expire;
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
        match self.list_versions_from_go(go_bin, progress_handler) {
            Ok(versions) => {
                if options.write_cache {
                    progress_handler.progress("updating cache with version list".to_string());
                    if let Err(err) = cache.add_versions(&self.path, &versions) {
                        progress_handler.progress(format!("failed to update cache: {err}"));
                    }
                }

                Ok(versions)
            }
            Err(err) => {
                if let Some(cached_versions) = cached_versions {
                    progress_handler.progress(format!(
                        "{}; {}",
                        format!("error refreshing version list: {err}").red(),
                        "using cached data".light_black()
                    ));
                    Ok(cached_versions)
                } else {
                    Err(err)
                }
            }
        }
    }

    fn list_versions_from_go(
        &self,
        go_bin: &GoBin,
        progress_handler: &UpProgressHandler,
    ) -> Result<GoInstallVersions, UpError> {
        // We need to:
        // - Resolve the path to use for the request, which is the path
        //   without the last particle, or before any cmd/ particle, and
        //   without any trailing slashes
        // - Call `go list -m -versions -json <path>`
        // - Parse the output into a list of versions, the only key that's
        //   interesting for us in the `Versions` field which is a list of
        //   strings

        let mut package_path = Some(Path::new(&self.path));
        while let Some(current_path) = package_path {
            if current_path.file_name().is_none() {
                break;
            }

            let mut go_list_cmd = go_bin.get_async_command();
            go_list_cmd.arg("list");
            go_list_cmd.arg("-m");
            go_list_cmd.arg("-versions");
            go_list_cmd.arg("-json");
            go_list_cmd.arg(current_path);
            go_list_cmd.stdout(std::process::Stdio::piped());
            go_list_cmd.stderr(std::process::Stdio::piped());

            match get_command_output(&mut go_list_cmd, RunConfig::new().with_askpass()) {
                Err(err) => {
                    let msg = format!("go list failed: {err}");
                    progress_handler.error_with_message(msg.clone());
                    return Err(UpError::Exec(msg));
                }
                Ok(output) if !output.status.success() => {}
                Ok(output) => {
                    let output = String::from_utf8(output.stdout).unwrap().trim().to_string();
                    if let Ok(versions) = serde_json::from_str::<GoInstallVersions>(&output) {
                        return Ok(versions);
                    }
                }
            }

            package_path = current_path.parent();
        }

        let errmsg = format!("unable to get versions for module {}", self.path);
        progress_handler.error_with_message(errmsg.clone());
        Err(UpError::Exec(errmsg))
    }

    fn latest_version(&self, versions: &GoInstallVersions) -> Result<String, UpError> {
        let latest = self.resolve_version_from_str("latest", &versions.versions)?;
        Ok(VersionParser::parse(&latest)
            .expect("failed to parse version string")
            .major()
            .to_string())
    }

    fn resolve_version(&self, versions: &[String]) -> Result<String, UpError> {
        let match_version = self.version.clone().unwrap_or_else(|| "latest".to_string());
        self.resolve_version_from_str(&match_version, versions)
    }

    fn resolve_version_from_str(
        &self,
        match_version: &str,
        versions: &[String],
    ) -> Result<String, UpError> {
        let mut matcher = VersionMatcher::new(match_version);
        matcher.prerelease(self.prerelease);
        matcher.build(self.build);
        matcher.prefix(true);

        let version = versions
            .iter()
            .filter_map(|version| VersionParser::parse(version))
            .sorted()
            .rev()
            .find(|version| matcher.matches(&version.to_string()))
            .ok_or_else(|| {
                UpError::Exec(format!(
                    "no matching version found for {}@{}",
                    self.path, match_version,
                ))
            })?;

        Ok(version.to_string())
    }

    fn list_installed_versions(
        &self,
        _progress_handler: &dyn ProgressHandler,
    ) -> Result<Vec<String>, UpError> {
        let version_path = go_install_bin_path().join(&self.path);

        if !version_path.exists() {
            return Ok(vec![]);
        }

        let installed_versions = std::fs::read_dir(&version_path)
            .map_err(|err| {
                let errmsg = format!("failed to read directory: {err}");
                UpError::Exec(errmsg)
            })?
            .filter_map(|entry| {
                entry.ok().and_then(|entry| {
                    if entry.file_type().ok()?.is_dir() {
                        entry.file_name().into_string().ok()
                    } else {
                        None
                    }
                })
            })
            .collect();

        Ok(installed_versions)
    }

    fn install_version(
        &self,
        go_bin: &GoBin,
        options: &UpOptions,
        version: &str,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<bool, UpError> {
        let install_path = self.version_path(version);

        if options.read_cache && install_path.exists() && install_path.is_dir() {
            progress_handler
                .progress(format!("installed {}@{} (cached)", self.path, version).light_black());

            return Ok(false);
        }

        // Make a temporary directory to download the release
        let tmp_dir = tempfile::Builder::new()
            .prefix(&tmpdir_cleanup_prefix("go-install"))
            .tempdir()
            .map_err(|err| {
                progress_handler.error_with_message(format!("failed to create temp dir: {err}"));
                UpError::Exec(format!("failed to create temp dir: {err}"))
            })?;
        let tmp_bin_path = tmp_dir.path().join("bin");

        let mut go_install_cmd = go_bin.get_async_command();
        go_install_cmd.arg("install");
        go_install_cmd.arg("-v");
        go_install_cmd.arg(format!("{}@{}", self.path, version));

        // Override GO environment variables to ensure that the
        // installation is done in the temporary directory
        go_install_cmd.env("GOPATH", tmp_dir.path());
        go_install_cmd.env("GOBIN", &tmp_bin_path);

        go_install_cmd.stdout(std::process::Stdio::piped());
        go_install_cmd.stderr(std::process::Stdio::piped());

        run_progress(
            &mut go_install_cmd,
            Some(progress_handler),
            RunConfig::default().with_askpass(),
        )?;

        if !tmp_bin_path.exists() {
            let msg = "failed to install (bin directory empty)".to_string();
            progress_handler.error_with_message(msg.clone());
            return Err(UpError::Exec(msg));
        }

        if !tmp_bin_path.exists() {
            let msg = "failed to install (bin directory was not created)".to_string();
            progress_handler.error_with_message(msg.clone());
            return Err(UpError::Exec(msg));
        }

        // Check that there is at least one binary in the bin directory
        let bin_files = std::fs::read_dir(&tmp_bin_path).map_err(|err| {
            let msg = format!("failed to read bin directory: {err}");
            progress_handler.error_with_message(msg.clone());
            UpError::Exec(msg)
        })?;
        let found_binary = bin_files
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .any(|entry| {
                entry
                    .metadata()
                    .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
            });
        if !found_binary {
            let msg = "failed to install (no binary found in bin directory)".to_string();
            progress_handler.error_with_message(msg.clone());
            return Err(UpError::Exec(msg));
        }

        // Move the installed version to the correct crate_name
        std::fs::create_dir_all(&install_path).map_err(|err| {
            let msg = format!("failed to create install directory: {err}");
            progress_handler.error_with_message(msg.clone());
            UpError::Exec(msg)
        })?;

        // Make sure the target directory does not exist
        let target_bin_dir = install_path.join("bin");
        force_remove_all(&target_bin_dir).map_err(|err| {
            let msg = format!("failed to remove target bin directory: {err}");
            progress_handler.error_with_message(msg.clone());
            UpError::Exec(msg)
        })?;

        // Move the tmp_bin_crate_name to the install_crate_name/<bin> directory
        safe_rename(&tmp_bin_path, target_bin_dir).map_err(|err| {
            let msg = format!("failed to move bin directory: {err}");
            progress_handler.error_with_message(msg.clone());
            UpError::Exec(msg)
        })?;

        Ok(true)
    }

    fn version_path(&self, version: &str) -> PathBuf {
        go_install_bin_path().join(&self.path).join(version)
    }
}

/// Validates a version string for go install
fn validate_go_install_version(version: &str) -> Result<(), GoInstallError> {
    if version.chars().any(|c| {
        c.is_whitespace()
            || c.is_control()
            || !c.is_ascii()
            || c == '@'
            || c == '<'
            || c == '>'
            || c == '['
            || c == ']'
            || c == '{'
            || c == '}'
            || c == ':'
            || c == ';'
            || c == ','
    }) {
        return Err(GoInstallError::InvalidImportPath(
            "version contains invalid characters".to_string(),
        ));
    }

    Ok(())
}

/// Cleans and validates a go install path
fn validate_go_install_path(path: &str) -> Result<String, GoInstallError> {
    if path.is_empty() {
        return Err(GoInstallError::InvalidImportPath(
            "empty import path".to_string(),
        ));
    }

    // Remove protocol if present
    let path = path.trim();
    let path = if let Some(idx) = path.find("://") {
        &path[idx + 3..]
    } else {
        path
    };

    // Split into segments and clean
    let segments: Vec<&str> = path
        .split('/')
        .filter(|s| !s.is_empty()) // Remove empty segments
        .collect();

    if segments.is_empty() {
        return Err(GoInstallError::InvalidImportPath(
            "empty path after cleaning".to_string(),
        ));
    }

    // Join segments back together
    Ok(segments.join("/"))
}

// This returns true if the provided version is in the format of a go pseudo-version
// e.g.:
//   - v0.0.0-20191109021931-daa7c04131f5
//   - vX.0.0-yyyymmddhhmmss-abcdefabcdef
//   - vX.Y.Z-pre.0.yyyymmddhhmmss-abcdefabcdef
//   - vX.Y.(Z+1)-0.yyyymmddhhmmss-abcdefabcdef
fn is_go_pseudo_version(version: &str) -> bool {
    // The version parser should be able to parse the version
    let parse_options = VersionParserOptions::new().complete_version(false);
    let parsed_version = match VersionParser::parse_with_options(version, &parse_options) {
        Some(parsed_version) => parsed_version,
        None => return false,
    };

    // The version should start with `v`
    match parsed_version.prefix() {
        Some("v") => {}
        _ => return false,
    }

    // There should be a pre-release bit, and it should be alphanumeric;
    // if there are multiple bits, we are only interested in the last one
    let pre_release = parsed_version.pre_release();
    let pre_release_last_bit = match pre_release.last() {
        Some(node_semver::Identifier::AlphaNumeric(chunk)) => chunk,
        _ => return false,
    };

    // That last bit should be two strings separated by a dash
    let pre_release_parts: Vec<&str> = pre_release_last_bit.split('-').collect();
    if pre_release_parts.len() != 2 {
        return false;
    }

    // The first part should be a date in the format of yyyymmddhhmmss
    let date = pre_release_parts[0];
    if date.len() != 14 || !date.chars().all(char::is_numeric) {
        return false;
    }

    // Validate the date can be parsed
    if PrimitiveDateTime::parse(
        date,
        format_description!("[year][month][day][hour][minute][second]"),
    )
    .is_err()
    {
        return false;
    }

    // The second part should be a commit hash
    let commit_hash = pre_release_parts[1];
    if commit_hash.len() != 12 || !commit_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }

    // If we got here, this is a pseudo-version
    true
}

#[derive(Debug, Clone)]
struct GoBin {
    bin: PathBuf,
    version: String,
}

impl GoBin {
    fn get(options: &UpOptions, progress_handler: &UpProgressHandler) -> Result<Self, UpError> {
        progress_handler.progress("install dependencies".to_string());
        let go_tool = UpConfigTool::Go(UpConfigGolang::new_any_version());

        // We create a fake environment since we do not want to add this
        // go version as part of it, but we want to be able to use `go`
        // to call `go list`, `go install`, etc.
        let mut fake_env = UpEnvironment::new();

        let subhandler = progress_handler.subhandler(&"go: ".light_black());
        go_tool.up(options, &mut fake_env, &subhandler)?;

        // Grab the tool from inside go_tool
        let go = match go_tool {
            UpConfigTool::Go(go) => go,
            _ => unreachable!("go_tool is not a Go tool"),
        };

        let installed_version = go.version()?;
        let install_path = PathBuf::from(mise_tool_path("go", &installed_version));
        let go_bin = install_path.join("bin").join("go");

        Ok(Self {
            bin: go_bin,
            version: installed_version,
        })
    }

    fn get_async_command(&self) -> TokioCommand {
        let tool_prefix = mise_tool_path("go", &self.version);

        let mut cmd = TokioCommand::new(&self.bin);
        cmd.env("GOROOT", &tool_prefix);
        cmd.env("GOVERSION", &self.version);
        cmd
    }
}

#[cfg(test)]
#[path = "go_install_test.rs"]
mod tests;
