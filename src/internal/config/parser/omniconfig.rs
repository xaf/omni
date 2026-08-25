use std::collections::HashMap;

use lazy_static::lazy_static;
use serde::Serialize;

use crate::internal::config::parser::AskPassConfig;
use crate::internal::config::parser::CacheConfig;
use crate::internal::config::parser::CdConfig;
use crate::internal::config::parser::CheckConfig;
use crate::internal::config::parser::CloneConfig;
use crate::internal::config::parser::CommandDefinition;
use crate::internal::config::parser::ConfigCommandsConfig;
use crate::internal::config::parser::EnvConfig;
use crate::internal::config::parser::GithubConfig;
use crate::internal::config::parser::MakefileCommandsConfig;
use crate::internal::config::parser::MatchSkipPromptIfConfig;
use crate::internal::config::parser::PathConfig;
use crate::internal::config::parser::PathRepoUpdatesConfig;
use crate::internal::config::parser::PromptsConfig;
use crate::internal::config::parser::ShellAliasesConfig;
use crate::internal::config::parser::SuggestCloneConfig;
use crate::internal::config::parser::SuggestConfig;
use crate::internal::config::parser::UpCommandConfig;
use crate::internal::config::up::UpConfig;
use crate::internal::config::OrgConfig;
use crate::internal::env::omni_git_env;
use crate::internal::env::user_home;

lazy_static! {
    #[derive(Debug, Serialize, Clone)]
    static ref DEFAULT_WORKTREE: String = {
        let home = user_home();
        let mut default_worktree_path = format!("{home}/git");
        if !std::path::Path::new(&default_worktree_path).is_dir() {
            // Check if GOPATH is set and GOPATH/src exists and is a directory
            let gopath = std::env::var("GOPATH").unwrap_or_else(|_| "".to_string());
            if !gopath.is_empty() {
                let gopath_src = format!("{gopath}/src");
                if std::path::Path::new(&gopath_src).is_dir() {
                    default_worktree_path = gopath_src;
                }
            }
        }
        default_worktree_path
    };
    #[derive(Debug, Serialize, Clone)]
    static ref DEFAULT_SANDBOX: String = {
        let home = user_home();
        format!("{home}/sandbox")
    };
}

// Default functions for compote derive
fn get_default_worktree() -> String {
    DEFAULT_WORKTREE.to_string()
}

fn get_default_sandbox() -> String {
    DEFAULT_SANDBOX.to_string()
}

fn get_default_repo_path_format() -> String {
    "%{host}/%{org}/%{repo}".to_string()
}

#[derive(Debug, Clone, compote::Config)]
pub struct OmniConfig {
    #[compote(default)]
    pub askpass: AskPassConfig,

    #[compote(default)]
    pub cache: CacheConfig,

    #[compote(default)]
    pub cd: CdConfig,

    #[compote(default, skip_if_empty)]
    pub check: CheckConfig,

    #[compote(default)]
    pub clone: CloneConfig,

    #[compote(default = "0.12")]
    pub command_match_min_score: f64,

    #[compote(default)]
    pub command_match_skip_prompt_if: MatchSkipPromptIfConfig,

    #[compote(default, skip_if_empty)]
    pub commands: HashMap<String, CommandDefinition>,

    #[compote(default)]
    pub config_commands: ConfigCommandsConfig,

    #[compote(default, skip_if_empty)]
    pub env: EnvConfig,

    #[compote(default, skip_if_empty)]
    pub github: GithubConfig,

    #[compote(default)]
    pub makefile_commands: MakefileCommandsConfig,

    #[compote(default, mutable_by = ["system", "user"], skip_if_empty)]
    pub org: Vec<OrgConfig>,

    #[compote(default)]
    pub path: PathConfig,

    #[compote(default)]
    pub path_repo_updates: PathRepoUpdatesConfig,

    #[compote(default, skip_if_empty)]
    pub prompts: PromptsConfig,

    #[compote(default_fn = "get_default_repo_path_format", skip_if_empty)]
    pub repo_path_format: String,

    #[compote(default, skip_if_empty)]
    pub shell_aliases: ShellAliasesConfig,

    #[compote(default, skip_if_empty)]
    pub suggest_clone: SuggestCloneConfig,

    #[compote(default, skip_if_empty)]
    pub suggest_config: SuggestConfig,

    #[compote(skip_if_empty)]
    pub up: Option<UpConfig>,

    #[compote(default, nested)]
    pub up_command: UpCommandConfig,

    #[compote(default_fn = "get_default_sandbox", skip_if_empty)]
    pub sandbox: String,

    #[compote(default_fn = "get_default_worktree", skip_if_empty)]
    pub worktree: String,
}

impl OmniConfig {
    const DEFAULT_COMMAND_MATCH_MIN_SCORE: f64 = 0.12;
    const DEFAULT_REPO_PATH_FORMAT: &'static str = "%{host}/%{org}/%{repo}";

    pub fn sandbox(&self) -> String {
        self.sandbox.clone()
    }

    pub fn worktree(&self) -> String {
        if let Some(omni_git) = omni_git_env() {
            return omni_git;
        }

        self.worktree.clone()
    }

    pub fn repo_path_format_host(&self) -> bool {
        self.repo_path_format.contains("%{host}")
    }

    pub fn repo_path_format_org(&self) -> bool {
        self.repo_path_format.contains("%{org}")
    }

    pub fn repo_path_format_repo(&self) -> bool {
        self.repo_path_format.contains("%{repo}")
    }

    /// Returns a hash of the configuration used for setting up a repository,
    /// so that we can inform the user if they should call `omni up` again.
    ///
    /// This includes the following configuration parameters:
    /// - up
    /// - suggest_config
    /// - suggest_clone
    /// - env
    pub fn up_hash(&self) -> String {
        let mut config_hasher = blake3::Hasher::new();

        if let Some(up) = &self.up {
            if let Ok(up_str) = compote::to_yaml(&up) {
                config_hasher.update(up_str.as_bytes());
            }
        }

        if let Ok(suggest_config_str) = compote::to_yaml(&self.suggest_config) {
            config_hasher.update(suggest_config_str.as_bytes());
        }

        if let Ok(suggest_clone_str) = compote::to_yaml(&self.suggest_clone) {
            config_hasher.update(suggest_clone_str.as_bytes());
        }

        if let Ok(env_str) = compote::to_yaml(&self.env) {
            config_hasher.update(env_str.as_bytes());
        }

        config_hasher.finalize().to_hex()[..16].to_string()
    }
}

impl Default for OmniConfig {
    fn default() -> Self {
        Self {
            askpass: AskPassConfig::default(),
            cache: CacheConfig::default(),
            cd: CdConfig::default(),
            check: CheckConfig::default(),
            clone: CloneConfig::default(),
            command_match_min_score: Self::DEFAULT_COMMAND_MATCH_MIN_SCORE,
            command_match_skip_prompt_if: MatchSkipPromptIfConfig::default(),
            commands: HashMap::new(),
            config_commands: ConfigCommandsConfig::default(),
            env: EnvConfig::default(),
            github: GithubConfig::default(),
            makefile_commands: MakefileCommandsConfig::default(),
            org: Vec::new(),
            path: PathConfig::default(),
            path_repo_updates: PathRepoUpdatesConfig::default(),
            prompts: PromptsConfig::default(),
            repo_path_format: Self::DEFAULT_REPO_PATH_FORMAT.to_string(),
            shell_aliases: ShellAliasesConfig::default(),
            suggest_clone: SuggestCloneConfig::default(),
            suggest_config: SuggestConfig::default(),
            up: None,
            up_command: UpCommandConfig::default(),
            sandbox: get_default_sandbox(),
            worktree: get_default_worktree(),
        }
    }
}
