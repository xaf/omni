use std::collections::HashMap;
use std::path::PathBuf;

use itertools::any;
use itertools::Itertools;
use serde::Serialize;

use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::config::global_config;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::UpConfigBundler;
use crate::internal::config::up::UpConfigCargoInstalls;
use crate::internal::config::up::UpConfigCustom;
use crate::internal::config::up::UpConfigGithubReleases;
use crate::internal::config::up::UpConfigGoInstalls;
use crate::internal::config::up::UpConfigGolang;
use crate::internal::config::up::UpConfigHomebrew;
use crate::internal::config::up::UpConfigMise;
use crate::internal::config::up::UpConfigNix;
use crate::internal::config::up::UpConfigNodejs;
use crate::internal::config::up::UpConfigPython;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::dynenv::update_dynamic_env_for_command_from_env;

// ============================================================================
// UpConfigBash: wrapper around UpConfigMise that sets the bash-specific tool_url
// ============================================================================

/// Wrapper around UpConfigMise that sets the bash-specific tool_url.
#[derive(Debug, Clone)]
pub struct UpConfigBash(pub UpConfigMise);

impl Default for UpConfigBash {
    fn default() -> Self {
        let mut mise = UpConfigMise::default();
        mise.tool_url = Some("https://github.com/xaf/asdf-bash".into());
        UpConfigBash(mise)
    }
}

impl<S: compote::CustomSource, L: compote::CustomLevel> compote::FromContextValue<S, L>
    for UpConfigBash
{
    fn from_context_value(
        value: &compote::ContextValue<S, L>,
        tracker: &mut compote::ErrorTracker,
    ) -> Result<Self, compote::Error> {
        let mut mise: UpConfigMise = compote::FromContextValue::from_context_value(value, tracker)?;
        mise.tool_url = Some("https://github.com/xaf/asdf-bash".into());
        mise.requested_tool = "bash".to_string();
        mise.process_from_tag();
        Ok(UpConfigBash(mise))
    }
}

impl Serialize for UpConfigBash {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        self.0.serialize(serializer)
    }
}

// ============================================================================
// UpConfigTool enum with compote derive
// ============================================================================

/// UpConfigTool represents a tool that can be upped or downed.
/// It can be a single tool or a combination of tools.
#[derive(Debug, Clone, compote::Config)]
#[compote(external_tag, skip_serialize)]
pub enum UpConfigTool {
    /// And represents a combination of tools that must all be upped.
    #[compote(rename = "and", variant = "and")]
    And(Vec<UpConfigTool>),

    /// Any represents a combination of tools where at least one must
    /// be upped. It will try to follow user preferences for ordering
    /// of which tool to handle. If none are available, it will try
    /// the others in the order they are defined. If the selected tool
    /// fails to up, it will try the next one until one is successful
    /// or all have been tried.
    #[compote(rename = "any", variant = "any")]
    Any(Vec<UpConfigTool>),

    // TODO: Apt(UpConfigApt),
    /// Bash represents the bash tool.
    #[compote(rename = "bash", variant = "bash")]
    Bash(UpConfigBash),

    /// Bundler represents the bundler tool.
    #[compote(
        rename = "bundler",
        alias = "bundle",
        variant = "bundler",
        variant = "bundle"
    )]
    Bundler(UpConfigBundler),

    /// CargoInstall represents a tool that can be installed from
    /// a call to `cargo install`.
    #[compote(
        rename = "cargo-install",
        alias = "cargo_install",
        alias = "cargoinstall",
        alias = "cargo-install-crates",
        alias = "cargo-install-crate",
        alias = "cargo-crates",
        alias = "cargo-crate",
        variant = "cargo-install",
        variant = "cargo_install",
        variant = "cargoinstall",
        variant = "cargo-install-crates",
        variant = "cargo-install-crate",
        variant = "cargo-crates",
        variant = "cargo-crate"
    )]
    CargoInstall(UpConfigCargoInstalls),

    /// Custom represents a custom tool, where the user can define
    /// a custom command to run to up/down the tool.
    #[compote(rename = "custom", variant = "custom")]
    Custom(UpConfigCustom),

    // TODO: Dnf(UpConfigDnf),
    /// GithubRelease represents a tool that can be installed from
    /// a github release.
    #[compote(
        rename = "github-release",
        alias = "github_release",
        alias = "githubrelease",
        alias = "ghrelease",
        alias = "github-releases",
        alias = "github_releases",
        alias = "githubreleases",
        alias = "ghreleases",
        alias = "github",
        alias = "gh-release",
        alias = "gh-releases",
        variant = "github-release",
        variant = "github_release",
        variant = "githubrelease",
        variant = "ghrelease",
        variant = "github-releases",
        variant = "github_releases",
        variant = "githubreleases",
        variant = "ghreleases",
        variant = "github",
        variant = "gh-release",
        variant = "gh-releases"
    )]
    GithubRelease(UpConfigGithubReleases),

    /// Go represents the golang tool.
    #[compote(rename = "go", alias = "golang", variant = "go", variant = "golang")]
    Go(UpConfigGolang),

    /// GoInstall represents a tool that can be installed from
    /// a call to `go install`.
    #[compote(
        rename = "go-install",
        alias = "go_install",
        alias = "goinstall",
        alias = "go-install-tools",
        alias = "go-install-tool",
        alias = "go-tools",
        alias = "go-tool",
        variant = "go-install",
        variant = "go_install",
        variant = "goinstall",
        variant = "go-install-tools",
        variant = "go-install-tool",
        variant = "go-tools",
        variant = "go-tool"
    )]
    GoInstall(UpConfigGoInstalls),

    /// Homebrew represents the homebrew tool.
    #[compote(
        rename = "homebrew",
        alias = "brew",
        variant = "homebrew",
        variant = "brew"
    )]
    Homebrew(UpConfigHomebrew),

    // TODO: Java(UpConfigMise), // JAVA_HOME
    // TODO: Kotlin(UpConfigMise), // KOTLIN_HOME
    /// Mise represents any generic mise tool that is not specifically
    /// defined in the other types for special handling.
    #[compote(fallback, from_tag = "requested_tool")]
    Mise(UpConfigMise),

    /// Nix represents the nix tool, which can be used to install
    /// packages from the nix package manager.
    #[compote(rename = "nix", variant = "nix")]
    Nix(UpConfigNix),

    /// Nodejs represents the nodejs tool.
    #[compote(
        rename = "nodejs",
        alias = "node",
        variant = "nodejs",
        variant = "node"
    )]
    Nodejs(UpConfigNodejs),

    /// Or represents a combination of tools where at least one must
    /// be upped. It will up the first tool that is available, and
    /// only try the others if the first one fails.
    #[compote(rename = "or", variant = "or")]
    Or(Vec<UpConfigTool>),

    // TODO: Pacman(UpConfigPacman),
    /// Python represents the python tool.
    #[compote(rename = "python", variant = "python")]
    Python(UpConfigPython),
}

// Generic function to create a hashmap with a single key/value pair.
fn create_hashmap<T>(key: &str, value: T) -> HashMap<String, T> {
    let mut new_obj = HashMap::new();
    new_obj.insert(key.to_string(), value);
    new_obj
}

impl Serialize for UpConfigTool {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ::serde::ser::Serializer,
    {
        // When serializing, we want to create a yaml object with the key being the UpConfigTool
        // type and the value being the UpConfigTool struct.
        match self {
            UpConfigTool::And(configs) => create_hashmap("and", configs).serialize(serializer),
            UpConfigTool::Any(configs) => create_hashmap("any", configs).serialize(serializer),
            UpConfigTool::Bash(config) => create_hashmap("bash", &config.0).serialize(serializer),
            UpConfigTool::Bundler(config) => {
                create_hashmap("bundler", config).serialize(serializer)
            }
            UpConfigTool::CargoInstall(config) => {
                create_hashmap("cargo-install", config).serialize(serializer)
            }
            UpConfigTool::Custom(config) => create_hashmap("custom", config).serialize(serializer),
            UpConfigTool::GithubRelease(config) => {
                create_hashmap("github-release", config).serialize(serializer)
            }
            UpConfigTool::Go(config) => create_hashmap("go", config).serialize(serializer),
            UpConfigTool::GoInstall(config) => {
                create_hashmap("go-install", config).serialize(serializer)
            }
            UpConfigTool::Homebrew(config) => {
                create_hashmap("homebrew", config).serialize(serializer)
            }
            UpConfigTool::Mise(config) => {
                create_hashmap(&config.name(), config).serialize(serializer)
            }
            UpConfigTool::Nix(config) => create_hashmap("nix", config).serialize(serializer),
            UpConfigTool::Nodejs(config) => create_hashmap("nodejs", config).serialize(serializer),
            UpConfigTool::Or(configs) => create_hashmap("or", configs).serialize(serializer),
            UpConfigTool::Python(config) => create_hashmap("python", config).serialize(serializer),
        }
    }
}

impl UpConfigTool {
    pub fn up(
        &self,
        options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        match self {
            UpConfigTool::And(_) | UpConfigTool::Any(_) | UpConfigTool::Or(_) => {}
            _ => {
                // Update the dynamic environment so that if anything has changed
                // the command can consider it right away
                update_dynamic_env_for_command_from_env(".", environment);
            }
        }

        match self {
            UpConfigTool::And(configs) => {
                for config in configs {
                    config.up(options, environment, progress_handler)?;
                }
                Ok(())
            }
            UpConfigTool::Any(configs) => {
                let mut result = Ok(());
                for config in ordered_configs(configs) {
                    if config.is_available() {
                        result = config.up(options, environment, progress_handler);
                        if result.is_ok() {
                            break;
                        }
                    }
                }
                result
            }
            UpConfigTool::Bash(config) => config.0.up(options, environment, progress_handler),
            UpConfigTool::Bundler(config) => config.up(options, environment, progress_handler),
            UpConfigTool::CargoInstall(config) => config.up(options, environment, progress_handler),
            UpConfigTool::Custom(config) => config.up(options, environment, progress_handler),
            UpConfigTool::GithubRelease(config) => {
                config.up(options, environment, progress_handler)
            }
            UpConfigTool::Go(config) => config.up(options, environment, progress_handler),
            UpConfigTool::GoInstall(config) => config.up(options, environment, progress_handler),
            UpConfigTool::Homebrew(config) => config.up(options, environment, progress_handler),
            UpConfigTool::Mise(config) => config.up(options, environment, progress_handler),
            UpConfigTool::Nix(config) => config.up(options, environment, progress_handler),
            UpConfigTool::Nodejs(config) => config.up(options, environment, progress_handler),
            UpConfigTool::Or(configs) => {
                // We stop at the first successful up, we only return
                // an error if all the configs failed.
                let mut result = Ok(());
                for config in configs {
                    if config.is_available() {
                        result = config.up(options, environment, progress_handler);
                        if result.is_ok() {
                            break;
                        }
                    }
                }
                result
            }
            UpConfigTool::Python(config) => config.up(options, environment, progress_handler),
        }
    }

    pub fn commit(&self, options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        match self {
            UpConfigTool::And(configs) | UpConfigTool::Any(configs) | UpConfigTool::Or(configs) => {
                for config in configs {
                    config.commit(options, env_version_id)?;
                }
            }
            UpConfigTool::Bash(config) => {
                if config.0.was_upped() {
                    config.0.commit(options, env_version_id)?;
                }
            }
            UpConfigTool::Bundler(_config) => {}
            UpConfigTool::CargoInstall(config) => {
                if config.was_upped() {
                    config.commit(options, env_version_id)?;
                }
            }
            UpConfigTool::Custom(_config) => {}
            UpConfigTool::GithubRelease(config) => {
                if config.was_upped() {
                    config.commit(options, env_version_id)?;
                }
            }
            UpConfigTool::Go(config) => {
                if config.was_upped() {
                    config.commit(options, env_version_id)?;
                }
            }
            UpConfigTool::GoInstall(config) => {
                if config.was_upped() {
                    config.commit(options, env_version_id)?;
                }
            }
            UpConfigTool::Homebrew(config) => {
                if config.was_upped() {
                    config.commit(options, env_version_id)?;
                }
            }
            UpConfigTool::Mise(config) => {
                if config.was_upped() {
                    config.commit(options, env_version_id)?;
                }
            }
            UpConfigTool::Nix(_config) => {}
            UpConfigTool::Nodejs(config) => {
                if config.backend.was_upped() {
                    config.backend.commit(options, env_version_id)?;
                }
            }
            UpConfigTool::Python(config) => {
                if config.backend.was_upped() {
                    config.backend.commit(options, env_version_id)?;
                }
            }
        }

        Ok(())
    }

    pub fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        match self {
            UpConfigTool::And(configs) | UpConfigTool::Any(configs) | UpConfigTool::Or(configs) => {
                for config in configs {
                    config.down(progress_handler)?;
                }
                Ok(())
            }
            UpConfigTool::Bash(config) => config.0.down(progress_handler),
            UpConfigTool::Bundler(config) => config.down(progress_handler),
            UpConfigTool::CargoInstall(config) => config.down(progress_handler),
            UpConfigTool::Custom(config) => config.down(progress_handler),
            UpConfigTool::GithubRelease(config) => config.down(progress_handler),
            UpConfigTool::Go(config) => config.down(progress_handler),
            UpConfigTool::GoInstall(config) => config.down(progress_handler),
            UpConfigTool::Homebrew(config) => config.down(progress_handler),
            UpConfigTool::Mise(config) => config.down(progress_handler),
            UpConfigTool::Nix(config) => config.down(progress_handler),
            UpConfigTool::Nodejs(config) => config.down(progress_handler),
            UpConfigTool::Python(config) => config.down(progress_handler),
        }
    }

    pub fn is_available(&self) -> bool {
        match self {
            UpConfigTool::And(configs) | UpConfigTool::Any(configs) | UpConfigTool::Or(configs) => {
                any(configs, |config| config.is_available())
            }
            UpConfigTool::Homebrew(config) => config.is_available(),
            UpConfigTool::Nix(config) => config.is_available(),
            _ => true,
        }
    }

    pub fn dir(&self) -> Option<String> {
        match self {
            UpConfigTool::Custom(config) => config.dir(),
            _ => None,
        }
    }

    pub fn was_upped(&self) -> bool {
        match self {
            UpConfigTool::And(configs) | UpConfigTool::Any(configs) | UpConfigTool::Or(configs) => {
                any(configs, |config| config.was_upped())
            }
            UpConfigTool::Bash(config) => config.0.was_upped(),
            // UpConfigTool::Bundler(config) => config.was_upped(),
            UpConfigTool::CargoInstall(config) => config.was_upped(),
            UpConfigTool::Custom(config) => config.was_upped(),
            // UpConfigTool::GithubRelease(config) => config.was_upped(),
            UpConfigTool::Go(config) => config.was_upped(),
            UpConfigTool::GoInstall(config) => config.was_upped(),
            // UpConfigTool::Homebrew(config) => config.was_upped(),
            UpConfigTool::Mise(config) => config.was_upped(),
            UpConfigTool::Nix(config) => config.was_upped(),
            UpConfigTool::Nodejs(config) => config.backend.was_upped(),
            UpConfigTool::Python(config) => config.backend.was_upped(),
            _ => false,
        }
    }

    pub fn data_paths(&self) -> Vec<PathBuf> {
        match self {
            UpConfigTool::And(configs) => configs
                .iter()
                .flat_map(|config| config.data_paths())
                .collect(),
            UpConfigTool::Any(configs) | UpConfigTool::Or(configs) => {
                match configs
                    .iter()
                    .find(|config| config.is_available() && config.was_upped())
                {
                    Some(config) => config.data_paths(),
                    None => vec![],
                }
            }
            UpConfigTool::Bash(config) => config.0.data_paths(),
            // UpConfigTool::Bundler(config) => config.data_paths(),
            // UpConfigTool::CargoInstall(config) => config.data_paths(),
            UpConfigTool::Custom(config) => config.data_paths(),
            // UpConfigTool::GithubRelease(config) => config.data_paths(),
            UpConfigTool::Go(config) => config.data_paths(),
            // UpConfigTool::GoInstall(config) => config.data_paths(),
            // UpConfigTool::Homebrew(config) => config.data_paths(),
            UpConfigTool::Mise(config) => config.data_paths(),
            UpConfigTool::Nix(config) => config.data_paths(),
            UpConfigTool::Nodejs(config) => config.backend.data_paths(),
            UpConfigTool::Python(config) => config.data_paths(),
            _ => vec![],
        }
    }

    pub fn to_name(&self) -> String {
        match self {
            UpConfigTool::And(_) => "and".into(),
            UpConfigTool::Any(_) => "any".into(),
            UpConfigTool::Or(_) => "or".into(),
            UpConfigTool::Bash(_) => "bash".into(),
            UpConfigTool::Bundler(_) => "bundler".into(),
            UpConfigTool::CargoInstall(_) => "cargo-install".into(),
            UpConfigTool::Custom(_) => "custom".into(),
            UpConfigTool::GithubRelease(_) => "github-release".into(),
            UpConfigTool::Go(_) => "go".into(),
            UpConfigTool::GoInstall(_) => "go-install".into(),
            UpConfigTool::Homebrew(_) => "homebrew".into(),
            UpConfigTool::Mise(config) => config.name(),
            UpConfigTool::Nix(_) => "nix".into(),
            UpConfigTool::Nodejs(_) => "nodejs".into(),
            UpConfigTool::Python(_) => "python".into(),
        }
    }

    pub fn sort_value(&self) -> i32 {
        match self {
            UpConfigTool::And(configs) | UpConfigTool::Any(configs) | UpConfigTool::Or(configs) => {
                // return the minimum sort value of all the configs
                // in the list
                configs
                    .iter()
                    .map(|config| config.sort_value())
                    .min()
                    .unwrap_or(i32::MAX)
            }
            _ => {
                let config = global_config();
                let preferred_tools = &config.up_command.preferred_tools;

                // check what is the position for self.to_name() in
                // preferred_tools and if it is not there, return the
                // max value possible; the lowest value means the
                // highest priority
                let position = preferred_tools
                    .iter()
                    .position(|x| x.to_lowercase() == self.to_name());

                match position {
                    Some(position) => position as i32,
                    None => i32::MAX,
                }
            }
        }
    }
}

fn ordered_configs(configs: &[UpConfigTool]) -> Vec<&UpConfigTool> {
    configs
        .iter()
        .sorted_by(|a, b| a.sort_value().cmp(&b.sort_value()))
        .collect()
}
