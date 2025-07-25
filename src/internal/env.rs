use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::IsTerminal;
use std::io::Write;
use std::panic::catch_unwind;
use std::path::PathBuf;
use std::process::exit;
use std::sync::Mutex;
use std::time::Duration as StdDuration;
use std::time::Instant as StdInstant;

use blake3::Hasher;
use fs4::fs_std::FileExt;
use gethostname::gethostname;
use git2::Repository;
use git_url_parse::GitUrl;
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use petname::Generator;
use petname::Petnames;
use time::OffsetDateTime;

use crate::internal::config::global_config;
use crate::internal::config::parser::PathEntryConfig;
use crate::internal::config::up::utils::force_remove_dir_all;
use crate::internal::config::up::utils::SyncUpdateListener;
use crate::internal::config::OrgConfig;
use crate::internal::dynenv::DynamicEnvExportMode;
use crate::internal::errors::SyncUpdateError;
use crate::internal::git::id_from_git_url;
use crate::internal::git::safe_git_url_parse;
use crate::internal::user_interface::StringColor;
use crate::internal::utils::base62_encode;
use crate::omni_error;
use crate::omni_info;
use crate::omni_warning;

extern crate machine_uid;

lazy_static! {
    #[derive(Debug)]
    static ref GIT_ENV: Mutex<GitRepoEnvByPath> = Mutex::new(GitRepoEnvByPath::new());

    #[derive(Debug)]
    static ref WORKDIR_ENV: Mutex<WorkDirEnvByPath> = Mutex::new(WorkDirEnvByPath::new());

    #[derive(Debug)]
    static ref INTERACTIVE_SHELL: bool = {
        if let Ok(noninteractive) = std::env::var("OMNI_NONINTERACTIVE") {
            if !noninteractive.is_empty() {
                return false;
            }
        }

        std::io::stdout().is_terminal()
    };

    #[derive(Debug)]
    static ref CURRENT_SHELL: Shell = Shell::from_env();

    #[derive(Debug)]
    static ref CURRENT_EXE: PathBuf = {
        let current_exe = std::env::current_exe();
        if current_exe.is_err() {
            omni_error!("failed to get current executable path", "hook init");
            exit(1);
        }
        current_exe.unwrap()
    };

    #[derive(Debug)]
    static ref HOMEBREW_PREFIX: Option<String> = resolve_homebrew_prefix();

    #[derive(Debug)]
    static ref HOMEBREW_REPOSITORY: Option<String> = resolve_homebrew_repository();

    #[derive(Debug)]
    static ref TMPDIR_CLEANUP_PREFIX: String = {
        // Generate a uuid
        let uuid = uuid::Uuid::new_v4();

        // Keep only the first section
        let short_uuid = uuid.to_string()[..8].to_string();

        format!("omnitmp-{short_uuid}")
    };

    #[derive(Debug)]
    static ref NOW: OffsetDateTime = OffsetDateTime::now_utc();

    #[derive(Debug)]
    static ref RUNNING_AS_SUDO: bool = std::env::var_os("SUDO_USER").is_some()
        && match std::env::var("USER") {
            Ok(user) => user == "root",
            Err(_) => false,
        };
}

/// Get the time of the current execution of omni in UTC.
/// This is only computed once and the same time will keep
/// being returned for the same run of omni. If omni calls
/// itself in a subprocess, the subprocess will have a time
/// greater or equal to the one of the calling process.
pub fn now() -> OffsetDateTime {
    *NOW
}

/// Indicates if the current shell is a sudo shell.
pub fn running_as_sudo() -> bool {
    *RUNNING_AS_SUDO
}

/// Cleanup temporary directories created by omni in the system's
/// temporary directory. This only cleanup the directories created
/// with the specific prefix used for this instance of omni.
/// Any other temporary directory won't be force-cleaned-up by
/// this function.
pub fn tmpdir_cleanup() {
    let tmp_dir_prefix = (*TMPDIR_CLEANUP_PREFIX).to_string();
    let tmp_dir = std::env::temp_dir().join(tmp_dir_prefix);
    let tmp_dir_str = tmp_dir.to_string_lossy().to_string();
    let glob_pattern = format!("{tmp_dir_str}*");

    if let Ok(entries) = glob::glob(&glob_pattern) {
        for entry in entries.into_iter().flatten() {
            let _ = force_remove_dir_all(&entry);
        }
    }
}

pub fn tmpdir_cleanup_prefix(prefix: &str) -> String {
    format!("{}_{}.", *TMPDIR_CLEANUP_PREFIX, prefix)
}

pub fn git_env<T: AsRef<str>>(path: T) -> GitRepoEnv {
    let path: &str = path.as_ref();
    let path = std::fs::canonicalize(path)
        .unwrap_or(path.to_owned().into())
        .to_str()
        .unwrap()
        .to_owned();
    let mut git_env = GIT_ENV.lock().unwrap();
    git_env.get(&path).clone()
}

pub fn git_env_flush_cache<T: AsRef<str>>(path: T) {
    let path: &str = path.as_ref();
    let path = std::fs::canonicalize(path)
        .unwrap_or(path.to_owned().into())
        .to_str()
        .unwrap()
        .to_owned();
    let mut git_env = GIT_ENV.lock().unwrap();
    git_env.remove(&path);
}

pub fn git_env_fresh<T: AsRef<str>>(path: T) -> GitRepoEnv {
    git_env_flush_cache(&path);
    git_env(&path)
}

pub fn workdir<T: AsRef<str>>(path: T) -> WorkDirEnv {
    let path: &str = path.as_ref();
    let path = std::fs::canonicalize(path)
        .unwrap_or(path.to_owned().into())
        .to_str()
        .unwrap()
        .to_owned();
    let mut workdir_env = WORKDIR_ENV.lock().unwrap();
    workdir_env.get(&path).clone()
}

pub fn workdir_flush_cache<T: AsRef<str>>(path: T) {
    let path: &str = path.as_ref();
    let path = std::fs::canonicalize(path)
        .unwrap_or(path.to_owned().into())
        .to_str()
        .unwrap()
        .to_owned();
    let mut workdir_env = WORKDIR_ENV.lock().unwrap();
    workdir_env.remove(&path);
    git_env_flush_cache(&path);
}

pub fn workdir_or_init<T: AsRef<str>>(path: T) -> Result<WorkDirEnv, String> {
    let path: &str = path.as_ref();
    let path = std::fs::canonicalize(path)
        .unwrap_or(path.to_owned().into())
        .to_str()
        .unwrap()
        .to_owned();
    let mut workdir_env = WORKDIR_ENV.lock().unwrap();

    let wd = workdir_env.get(&path).clone();
    if wd.in_workdir() && wd.has_id() {
        return Ok(wd);
    }

    let wd_root = if let Some(wd_root) = wd.root() {
        wd_root
    } else {
        &path
    };

    workdir_env.remove(&path);

    let local_config_dir = PathBuf::from(wd_root).join(".omni");
    if let Err(err) = std::fs::create_dir_all(local_config_dir.clone()) {
        return Err(format!(
            "failed to create directory '{}': {}",
            local_config_dir.display(),
            err
        ));
    }

    // Open the 'id' file in the local config directory in write/create mode
    // and write a uuid to it
    let id_file = local_config_dir.join("id");
    match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(id_file.clone())
    {
        Ok(mut file) => {
            let id = WorkDirEnv::generate_id();
            if let Err(err) = file.write_all(id.as_bytes()) {
                return Err(format!(
                    "failed to write to '{}': {}",
                    id_file.display(),
                    err,
                ));
            }

            omni_warning!(format!("generated workdir id {}", id.light_yellow()));
        }
        Err(err) => {
            return Err(format!("failed to open '{}': {}", id_file.display(), err,));
        }
    }

    let wd = workdir_env.get(&path).clone();

    if !wd.in_workdir() {
        return Err(format!("failed to create workdir for '{path}'"));
    }

    Ok(wd)
}

fn compute_user_home() -> String {
    std::env::var("HOME").expect("Failed to determine user's home directory")
}

fn compute_xdg_config_home() -> String {
    match std::env::var("XDG_CONFIG_HOME") {
        Ok(xdg_config_home) if !xdg_config_home.is_empty() && xdg_config_home.starts_with('/') => {
            xdg_config_home
        }
        _ => {
            format!("{}/.config", user_home())
        }
    }
}

fn compute_config_home() -> String {
    match std::env::var("OMNI_CONFIG_HOME") {
        Ok(config_home)
            if !config_home.is_empty()
                && (config_home.starts_with('/') || config_home.starts_with("~/")) =>
        {
            if let Some(path_in_home) = config_home.strip_prefix("~/") {
                format!("{}/{}", user_home(), path_in_home)
            } else {
                config_home
            }
        }
        _ => {
            format!("{}/omni", xdg_config_home())
        }
    }
}

fn compute_xdg_data_home() -> String {
    match std::env::var("XDG_DATA_HOME") {
        Ok(xdg_data_home) if !xdg_data_home.is_empty() && xdg_data_home.starts_with('/') => {
            xdg_data_home
        }
        _ => {
            format!("{}/.local/share", user_home())
        }
    }
}

fn compute_data_home() -> String {
    match std::env::var("OMNI_DATA_HOME") {
        Ok(data_home)
            if !data_home.is_empty()
                && (data_home.starts_with('/') || data_home.starts_with("~/")) =>
        {
            if let Some(path_in_home) = data_home.strip_prefix("~/") {
                format!("{}/{}", user_home(), path_in_home)
            } else {
                data_home
            }
        }
        _ => {
            format!("{}/omni", xdg_data_home())
        }
    }
}

fn compute_shims_dir() -> PathBuf {
    PathBuf::from(data_home()).join("shims")
}

fn compute_xdg_state_home() -> String {
    match std::env::var("XDG_STATE_HOME") {
        Ok(xdg_state_home) if !xdg_state_home.is_empty() && xdg_state_home.starts_with('/') => {
            xdg_state_home
        }
        _ => {
            format!("{}/.local/state", user_home())
        }
    }
}

fn compute_state_home() -> String {
    match std::env::var("OMNI_STATE_HOME") {
        Ok(state_home)
            if !state_home.is_empty()
                && (state_home.starts_with('/') || state_home.starts_with("~/")) =>
        {
            if let Some(path_in_home) = state_home.strip_prefix("~/") {
                format!("{}/{}", user_home(), path_in_home)
            } else {
                state_home
            }
        }
        _ => {
            format!("{}/omni", xdg_state_home())
        }
    }
}

fn compute_xdg_cache_home() -> String {
    match std::env::var("XDG_CACHE_HOME") {
        Ok(xdg_cache_home) if !xdg_cache_home.is_empty() && xdg_cache_home.starts_with('/') => {
            xdg_cache_home
        }
        _ => {
            format!("{}/.cache", user_home())
        }
    }
}

fn compute_cache_home() -> String {
    match std::env::var("OMNI_CACHE_HOME") {
        Ok(cache_home)
            if !(cache_home.is_empty()
                || (!cache_home.starts_with('/') && !cache_home.starts_with("~/"))) =>
        {
            if let Some(path_in_home) = cache_home.strip_prefix("~/") {
                format!("{}/{}", user_home(), path_in_home)
            } else {
                cache_home
            }
        }
        _ => {
            format!("{}/omni", xdg_cache_home())
        }
    }
}

fn compute_omnipath_env() -> Vec<String> {
    let mut omnipath = Vec::new();
    let mut omnipath_seen = HashSet::new();
    if let Ok(omnipath_str) = std::env::var("OMNIPATH") {
        for path in omnipath_str.split(':') {
            if !path.is_empty() && omnipath_seen.insert(path.to_string()) {
                omnipath.push(path.to_string());
            }
        }
    }
    omnipath
}

fn compute_omni_git_env() -> Option<String> {
    if let Ok(omni_git) = std::env::var("OMNI_GIT") {
        if !omni_git.is_empty() && omni_git.starts_with('/') {
            return Some(omni_git);
        }
    }
    None
}

fn compute_omni_org_env() -> Vec<OrgConfig> {
    let mut omni_org = Vec::new();
    if let Ok(omni_org_str) = std::env::var("OMNI_ORG") {
        for path in omni_org_str.split(',') {
            if !path.is_empty() {
                omni_org.push(OrgConfig::from_str(path));
            }
        }
    }
    omni_org
}

fn compute_omni_cmd_file() -> Option<String> {
    let mut omni_cmd_file = None;
    if let Ok(omni_cmd_file_str) = std::env::var("OMNI_CMD_FILE") {
        if !omni_cmd_file_str.is_empty() {
            omni_cmd_file = Some(omni_cmd_file_str);
        }
    }
    omni_cmd_file
}

fn compute_omni_tmpdir() -> String {
    match std::env::var("OMNI_TMPDIR") {
        Ok(omni_tmpdir) if !omni_tmpdir.is_empty() && omni_tmpdir.starts_with('/') => omni_tmpdir,
        _ => {
            let tmpdir = match std::env::var("TMPDIR") {
                Ok(tmpdir) if !tmpdir.is_empty() && tmpdir.starts_with('/') => tmpdir,
                _ => "/tmp".to_string(),
            };
            let user = whoami::username();

            format!("{tmpdir}/omni.{user}")
        }
    }
}

cfg_if::cfg_if! {
    if #[cfg(test)] {
        pub fn user_home() -> String {
            compute_user_home()
        }

        pub fn xdg_config_home() -> String {
            compute_xdg_config_home()
        }

        pub fn config_home() -> String {
            compute_config_home()
        }

        pub fn xdg_data_home() -> String {
            compute_xdg_data_home()
        }

        pub fn data_home() -> String {
            compute_data_home()
        }

        pub fn shims_dir() -> PathBuf {
            compute_shims_dir()
        }

        pub fn xdg_state_home() -> String {
            compute_xdg_state_home()
        }

        pub fn state_home() -> String {
            compute_state_home()
        }

        pub fn xdg_cache_home() -> String {
            compute_xdg_cache_home()
        }

        pub fn cache_home() -> String {
            compute_cache_home()
        }

        pub fn omnipath_env() -> Vec<String> {
            compute_omnipath_env()
        }

        pub fn omni_git_env() -> Option<String> {
            compute_omni_git_env()
        }

        pub fn omni_org_env() -> Vec<OrgConfig> {
            compute_omni_org_env()
        }

        pub fn omni_cmd_file() -> Option<String> {
            compute_omni_cmd_file()
        }

        pub fn omni_tmpdir() -> String {
            compute_omni_tmpdir()
        }
    } else {
        lazy_static! {
            #[derive(Debug)]
            static ref HOME: String = compute_user_home();

            #[derive(Debug)]
            static ref XDG_CONFIG_HOME: String = compute_xdg_config_home();

            #[derive(Debug)]
            static ref CONFIG_HOME: String = compute_config_home();

            #[derive(Debug)]
            static ref XDG_DATA_HOME: String = compute_xdg_data_home();

            #[derive(Debug)]
            static ref DATA_HOME: String = compute_data_home();

            #[derive(Debug)]
            static ref SHIMS_DIR: PathBuf = compute_shims_dir();

            #[derive(Debug)]
            static ref XDG_STATE_HOME: String = compute_xdg_state_home();

            #[derive(Debug)]
            static ref STATE_HOME: String = compute_state_home();

            #[derive(Debug)]
            static ref XDG_CACHE_HOME: String = compute_xdg_cache_home();

            #[derive(Debug)]
            static ref CACHE_HOME: String = compute_cache_home();

            #[derive(Debug)]
            static ref OMNIPATH: Vec<String> = compute_omnipath_env();

            #[derive(Debug)]
            static ref OMNI_GIT: Option<String> = compute_omni_git_env();

            #[derive(Debug)]
            static ref OMNI_ORG: Vec<OrgConfig> = compute_omni_org_env();

            #[derive(Debug)]
            static ref OMNI_CMD_FILE: Option<String> = compute_omni_cmd_file();

            #[derive(Debug)]
            static ref OMNI_TMPDIR: String = compute_omni_tmpdir();
        }

        pub fn user_home() -> String {
            (*HOME).to_string()
        }

        pub fn xdg_config_home() -> String {
            (*XDG_CONFIG_HOME).to_string()
        }

        pub fn config_home() -> String {
            (*CONFIG_HOME).to_string()
        }

        pub fn xdg_data_home() -> String {
            (*XDG_DATA_HOME).to_string()
        }

        pub fn data_home() -> String {
            (*DATA_HOME).to_string()
        }

        pub fn shims_dir() -> PathBuf {
            (*SHIMS_DIR).clone()
        }

        pub fn xdg_state_home() -> String {
            (*XDG_STATE_HOME).to_string()
        }

        pub fn state_home() -> String {
            (*STATE_HOME).to_string()
        }

        pub fn xdg_cache_home() -> String {
            (*XDG_CACHE_HOME).to_string()
        }

        pub fn cache_home() -> String {
            (*CACHE_HOME).to_string()
        }

        pub fn omnipath_env() -> Vec<String> {
            (*OMNIPATH).clone()
        }

        pub fn omni_git_env() -> Option<String> {
            (*OMNI_GIT).clone()
        }

        pub fn omni_org_env() -> Vec<OrgConfig> {
            (*OMNI_ORG).clone()
        }

        pub fn omni_cmd_file() -> Option<String> {
            (*OMNI_CMD_FILE).clone()
        }

        pub fn omni_tmpdir() -> String {
            (*OMNI_TMPDIR).to_string()
        }
    }
}

pub fn shell_integration_is_loaded() -> bool {
    omni_cmd_file().is_some()
}

pub fn shell_is_interactive() -> bool {
    *INTERACTIVE_SHELL
}

pub fn current_exe() -> PathBuf {
    (*CURRENT_EXE).clone()
}

pub fn current_dir() -> PathBuf {
    std::env::current_dir().expect("failed to get current dir")
}

/// Get the homebrew prefix, if available.
#[inline]
fn resolve_homebrew_prefix() -> Option<String> {
    // Get the homebrew prefix, either through the HOMEBREW_PREFIX env var
    // if available, or by calling `brew --prefix`; if both fail, we're not
    // installed with homebrew since... probably no homebrew.
    if let Ok(prefix) = std::env::var("HOMEBREW_PREFIX") {
        if !prefix.is_empty() {
            return Some(prefix);
        }
    }
    let output = std::process::Command::new("brew").arg("--prefix").output();
    if let Ok(output) = output {
        if output.status.success() {
            if let Ok(prefix) = String::from_utf8(output.stdout) {
                let prefix = prefix.trim();
                if !prefix.is_empty() {
                    return Some(prefix.to_string());
                }
            }
        }
    }
    None
}

/// Returns the homebrew prefix, if available.
pub fn homebrew_prefix() -> Option<String> {
    (*HOMEBREW_PREFIX).clone()
}

/// Get the homebrew repository, if available.
#[inline]
fn resolve_homebrew_repository() -> Option<String> {
    // Get the homebrew repository, either through the HOMEBREW_REPOSITORY env var
    // if available, or by calling `brew --repository`; if both fail, we're not
    // installed with homebrew since... probably no homebrew.
    if let Ok(repository) = std::env::var("HOMEBREW_REPOSITORY") {
        if !repository.is_empty() {
            return Some(repository);
        }
    }
    let output = std::process::Command::new("brew")
        .arg("--repository")
        .output();
    if let Ok(output) = output {
        if output.status.success() {
            if let Ok(repository) = String::from_utf8(output.stdout) {
                let repository = repository.trim();
                if !repository.is_empty() {
                    return Some(repository.to_string());
                }
            }
        }
    }
    None
}

/// Returns the homebrew repository, if available.
pub fn homebrew_repository() -> Option<String> {
    (*HOMEBREW_REPOSITORY).clone()
}

#[derive(Debug, Clone)]
pub struct GitRepoEnvByPath {
    env_by_path: HashMap<String, GitRepoEnv>,
}

impl GitRepoEnvByPath {
    fn new() -> Self {
        Self {
            env_by_path: HashMap::new(),
        }
    }

    pub fn get(&mut self, path: &str) -> &GitRepoEnv {
        if !self.env_by_path.contains_key(path) {
            self.env_by_path
                .insert(path.to_string(), GitRepoEnv::new(path));
        }
        self.env_by_path.get(path).unwrap()
    }

    pub fn remove(&mut self, path: &str) {
        self.env_by_path.remove(path);
    }
}

#[derive(Debug, Clone)]
pub struct GitRepoEnv {
    in_repo: bool,
    root: Option<String>,
    origin: Option<String>,
    commit: Option<String>,
    id: OnceCell<Option<String>>,
}

impl GitRepoEnv {
    fn new(path: &str) -> Self {
        let mut git_repo_env = Self {
            in_repo: false,
            root: None,
            origin: None,
            commit: None,
            id: OnceCell::new(),
        };

        let repository = match Repository::discover(path) {
            Ok(repository) => repository,
            Err(_) => return git_repo_env,
        };

        git_repo_env.in_repo = true;

        if let Ok(head) = repository.head() {
            if let Ok(head_commit) = head.peel_to_commit() {
                git_repo_env.commit = Some(head_commit.id().to_string());
            }
        }

        if let Some(workdir) = repository.workdir() {
            if let Some(root_dir) = workdir.to_str() {
                git_repo_env.root =
                    Some(root_dir.strip_suffix('/').unwrap_or(root_dir).to_string());
            }
        }

        if let Ok(remote) = repository.find_remote("origin") {
            if let Some(url) = remote.url() {
                git_repo_env.origin = Some(url.to_string());
                return git_repo_env;
            }
        }

        // loop over main, master, current
        for mut branch_name in &["main", "master", "__current"] {
            let mut string_branch_name = branch_name.to_string();
            if string_branch_name == "__current" {
                if let Ok(head) = repository.head() {
                    if let Some(shorthand) = head.shorthand() {
                        if shorthand != "HEAD" {
                            string_branch_name = shorthand.to_string();
                        }
                    }
                }
                if string_branch_name == "__current" {
                    continue;
                }
            }
            let str_branch_name = string_branch_name.as_str();
            branch_name = &str_branch_name;

            if let Ok(branch) = repository.find_branch(branch_name, git2::BranchType::Local) {
                if let Ok(upstream) = branch.upstream() {
                    if let Ok(Some(upstream_name)) = upstream.name() {
                        let upstream_name = upstream_name.split('/').next().unwrap();
                        if let Ok(remote) = repository.find_remote(upstream_name) {
                            if let Some(url) = remote.url() {
                                git_repo_env.origin = Some(url.to_string());
                                return git_repo_env;
                            }
                        }
                    }
                }
            }
        }

        if let Ok(remotes) = repository.remotes() {
            for remote in remotes.iter() {
                if let Ok(remote) = repository.find_remote(remote.unwrap()) {
                    if let Some(url) = remote.url() {
                        git_repo_env.origin = Some(url.to_string());
                        return git_repo_env;
                    }
                }
            }
        }

        git_repo_env
    }

    pub fn in_repo(&self) -> bool {
        self.in_repo
    }

    pub fn has_origin(&self) -> bool {
        self.in_repo && self.origin.is_some()
    }

    pub fn root(&self) -> Option<&str> {
        match &self.root {
            Some(root) => Some(root.as_str()),
            None => None,
        }
    }

    pub fn origin(&self) -> Option<&str> {
        match &self.origin {
            Some(origin) => Some(origin.as_str()),
            None => None,
        }
    }

    pub fn commit(&self) -> Option<&str> {
        match &self.commit {
            Some(commit) => Some(commit.as_str()),
            None => None,
        }
    }

    pub fn url(&self) -> Option<GitUrl> {
        if let Some(origin) = &self.origin {
            if let Ok(url) = safe_git_url_parse(origin) {
                return Some(url);
            }
        }
        None
    }

    pub fn id(&self) -> Option<String> {
        self.id
            .get_or_init(|| {
                if let Some(origin) = &self.origin {
                    if let Ok(url) = safe_git_url_parse(origin) {
                        return id_from_git_url(&url);
                    }
                }
                None
            })
            .clone()
    }
}

#[derive(Debug, Clone)]
pub struct WorkDirEnvByPath {
    env_by_path: HashMap<String, WorkDirEnv>,
}

impl WorkDirEnvByPath {
    fn new() -> Self {
        Self {
            env_by_path: HashMap::new(),
        }
    }

    pub fn get(&mut self, path: &str) -> &WorkDirEnv {
        if !self.env_by_path.contains_key(path) {
            self.env_by_path
                .insert(path.to_string(), WorkDirEnv::new(path));
        }
        self.env_by_path.get(path).unwrap()
    }

    pub fn remove(&mut self, path: &str) {
        self.env_by_path.remove(path);
    }
}

#[derive(Debug, Clone, Default)]
pub struct WorkDirEnv {
    in_workdir: bool,
    in_git: bool,
    root: Option<String>,
    id: OnceCell<Option<String>>,
    data_path: OnceCell<PathBuf>,
}

impl WorkDirEnv {
    fn new(path: &str) -> Self {
        let mut workdir_env = Self::default();

        let git = git_env(path);
        if git.in_repo() {
            workdir_env.in_workdir = true;
            workdir_env.in_git = true;
            workdir_env.root = git.root().map(|s| s.to_string());
        } else {
            // Start from `path` and go up until finding a `.omni/id` file
            let mut path = PathBuf::from(path);
            loop {
                if let Some(id) = Self::read_id_file(path.to_str().unwrap()) {
                    workdir_env.in_workdir = true;
                    workdir_env.root = Some(path.to_str().unwrap().to_string());
                    if workdir_env.id.set(Some(id)).is_err() {
                        unreachable!();
                    }
                    break;
                }
                if !path.pop() {
                    break;
                }
            }
        }

        workdir_env
    }

    pub fn in_workdir(&self) -> bool {
        self.in_workdir
    }

    pub fn in_git(&self) -> bool {
        self.in_git
    }

    pub fn root(&self) -> Option<&str> {
        match &self.root {
            Some(root) => Some(root.as_str()),
            None => None,
        }
    }

    pub fn data_path(&self) -> Option<&PathBuf> {
        if let Some(id) = &self.id() {
            let data_path = self.data_path.get_or_init(|| {
                // Generate a hash from the id
                let mut hasher = Hasher::new();
                hasher.update(id.as_bytes());
                let hash_bytes = hasher.finalize();
                let hash_b62 = base62_encode(hash_bytes.as_bytes())[..20].to_string();

                let mut data_path = PathBuf::from(data_home());
                data_path.push("wd");
                data_path.push(hash_b62);
                data_path
            });
            return Some(data_path);
        }
        None
    }

    pub fn reldir(&self, path: &str) -> Option<String> {
        if let Some(root) = &self.root {
            if let Ok(path) = std::fs::canonicalize(path) {
                if let Ok(path) = path.strip_prefix(root) {
                    let mut path = path.to_str().unwrap().to_string();
                    while path.starts_with('/') {
                        path = path[1..].to_string();
                    }
                    while path.ends_with('/') {
                        path = path[..path.len() - 1].to_string();
                    }
                    return Some(path);
                }
            }
        }
        None
    }

    pub fn trust_id(&self) -> Option<String> {
        self.id
            .get_or_init(|| {
                self.root.as_ref()?;

                if let Some(id) = Self::read_id_file(self.root.as_ref().unwrap()) {
                    return Some(id);
                }

                if let Some(id) = git_env(self.root.as_ref().unwrap()).id() {
                    return Some(id);
                }

                None
            })
            .clone()
    }

    pub fn id(&self) -> Option<String> {
        self.trust_id().map(|id| {
            if self.is_package() {
                format!("package#{id}")
            } else {
                id
            }
        })
    }

    pub fn typed_id(&self) -> Option<String> {
        self.trust_id().map(|id| {
            if self.is_package() {
                format!("package#{id}")
            } else {
                format!("worktree#{id}")
            }
        })
    }

    pub fn is_package(&self) -> bool {
        match &self.root {
            Some(root) => PathEntryConfig::from_path(root).is_package(),
            None => false,
        }
    }

    pub fn has_id(&self) -> bool {
        self.id().is_some()
    }

    fn read_id_file(path: &str) -> Option<String> {
        let id_file = PathBuf::from(path).join(".omni/id");
        if id_file.exists() {
            if let Ok(id) = std::fs::read_to_string(id_file) {
                // if the id is valid, then we can use it, otherwise ignore it
                let id = id.trim();
                if Self::verify_id(id) {
                    return Some(id.to_string());
                }
            }
        }
        None
    }

    fn generate_id() -> String {
        let petname_id = Petnames::default().generate_one(3, "-").expect("no names");
        format!("{}:{:016x}", petname_id, Self::machine_id_hash(&petname_id))
    }

    fn verify_id(id: &str) -> bool {
        // Split id over ':'
        let id_parts: Vec<&str> = id.split(':').collect();

        // Check if id has 2 parts
        if id_parts.len() != 2 {
            return false;
        }

        // Check if first part is words with lowercase letters separated by '-'
        if !id_parts[0]
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '-')
        {
            return false;
        }

        // Check if second part is 16 characters long
        if id_parts[1].len() != 16 {
            return false;
        }

        // Check if second part is a hexadecimal u64
        if let Ok(hash_u64) = u64::from_str_radix(id_parts[1], 16) {
            // Compare hash_u64 with machine_id_hash
            return hash_u64 == WorkDirEnv::machine_id_hash(id_parts[0]);
        }

        false
    }

    fn machine_id_hash(uuid: &str) -> u64 {
        // We try to get a machine id, if we can't, we fallback to the hostname
        // If we can't get the hostname, we fallback to an empty string
        let machine_id = match machine_uid::get() {
            Ok(machine_id) => machine_id,
            Err(_) => match catch_unwind(gethostname) {
                Ok(hostname) => hostname.to_string_lossy().to_string(),
                Err(_) => "".to_string(),
            },
        };

        let mut hasher = Hasher::new();
        hasher.update(machine_id.as_bytes());
        hasher.update(uuid.as_bytes());

        let hash_bytes = hasher.finalize();
        let hash_u64 = u64::from_le_bytes(hash_bytes.as_bytes()[..8].try_into().unwrap());

        hash_u64
    }

    pub fn lock_update(
        self,
        listener: &mut SyncUpdateListener,
    ) -> Result<Option<std::fs::File>, SyncUpdateError> {
        // Make sure the sync directory for up operations exists
        let main_sync_dir_path = PathBuf::from(omni_tmpdir());
        let sync_dir_path = main_sync_dir_path.join("up");
        if !sync_dir_path.exists() {
            std::fs::create_dir_all(&sync_dir_path)?;
        }

        // Try to get the id and root, or raise error
        let id = self.id().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "workdir id not found")
        })?;

        let root = self.root().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "workdir root not found")
        })?;

        // Set random bytes as the separator, that couldn't be part of the strings above
        let separator = [0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00];

        // Use a hashing function to generate a unique identifier for the workdir,
        // using the workdir id and the workdir path, so that the same workdir in
        // the same location cannot be updated twice at the same time, but the same
        // workdir in two different locations can (e.g. a package and a worktree,
        // or two clones of the same repo...)
        let mut hasher = blake3::Hasher::new();
        hasher.update(id.as_bytes());
        hasher.update(&separator);
        hasher.update(root.as_bytes());
        let workdir_unique_id = hasher.finalize().to_hex()[..64].to_string();

        // The sync path is that id, in the up sync directory
        let sync_path = sync_dir_path.join(workdir_unique_id);

        // Open the sync file in read/write/create mode, so it gets created if it does not exist
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(sync_path)?;

        // Try to acquire an exclusive lock on the file
        if let Ok(_file_lock) = file.try_lock_exclusive() {
            // Now that we have the lock, truncate the file contents
            file.set_len(0)?;
            file.flush()?;

            return Ok(Some(file));
        }

        // If we can't acquire the lock, it means that another process is updating the workdir
        // at the same time, so we will wait on the update to finish while streaming the
        // file contents, and return None to indicate that the update was already done
        match listener.follow(&file) {
            Ok(true) => Ok(None), // The update was done
            Ok(false) => {
                // We killed the running process, let's try and lock the file
                // until we timeout, and error out if we can't
                let timeout_duration =
                    StdDuration::from_secs(global_config().up_command.attach_lock_timeout);
                let start_time = StdInstant::now();

                loop {
                    match file.try_lock_exclusive() {
                        Ok(_file_lock) => {
                            // Now that we have the lock, truncate the file contents
                            file.set_len(0)?;
                            file.flush()?;

                            return Ok(Some(file));
                        }
                        Err(_) => {
                            if start_time.elapsed() > timeout_duration {
                                return Err(SyncUpdateError::Timeout);
                            }

                            std::thread::sleep(StdDuration::from_millis(100));
                        }
                    }
                }
            }
            Err(SyncUpdateError::MismatchedInit { actual, .. }) => {
                // If the init operation is mismatched, we need to wait for the lock to be released
                omni_info!(format!(
                    "waiting for running {} operation to finish",
                    actual.name().light_yellow()
                ));

                // Grab the lock in a blocking way
                file.lock_exclusive()?;

                // Now that we have the lock, truncate the file contents
                file.set_len(0)?;
                file.flush()?;

                // Return the file
                Ok(Some(file))
            }
            Err(err) => Err(err),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Posix,
    Unknown(String),
}

impl std::fmt::Display for Shell {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl Shell {
    pub fn current() -> Self {
        CURRENT_SHELL.clone()
    }

    pub fn all() -> Vec<Self> {
        vec![Shell::Bash, Shell::Zsh, Shell::Fish, Shell::Posix]
    }

    pub fn from_env() -> Self {
        let shell = determine_shell();
        Self::from_str(&shell)
    }

    pub fn from_str(shell: &str) -> Self {
        match shell.to_lowercase().as_str() {
            "bash" => Shell::Bash,
            "zsh" => Shell::Zsh,
            "fish" => Shell::Fish,
            "posix" => Shell::Posix,
            _ => Shell::Unknown(shell.to_string()),
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            Shell::Bash => "bash",
            Shell::Zsh => "zsh",
            Shell::Fish => "fish",
            Shell::Posix => "posix",
            Shell::Unknown(shell) => shell,
        }
    }

    pub fn dynenv_export_mode(&self) -> Option<DynamicEnvExportMode> {
        match self {
            Shell::Bash | Shell::Zsh | Shell::Posix => Some(DynamicEnvExportMode::Posix),
            Shell::Fish => Some(DynamicEnvExportMode::Fish),
            Shell::Unknown(_) => None,
        }
    }

    pub fn is_fish(&self) -> bool {
        matches!(self, Shell::Fish)
    }

    pub fn default_rc_file(&self) -> PathBuf {
        match self {
            Shell::Bash => PathBuf::from(user_home()).join(".bashrc"),
            Shell::Zsh => PathBuf::from(user_home()).join(".zshrc"),
            Shell::Fish => PathBuf::from(xdg_config_home()).join("fish/omni.fish"),
            Shell::Posix => PathBuf::from("/dev/null"),
            Shell::Unknown(_) => PathBuf::from("/dev/null"),
        }
    }

    pub fn hook_init_command(&self) -> String {
        match self {
            Shell::Bash => "eval \"$(omni hook init bash)\"".to_string(),
            Shell::Zsh => "eval \"$(omni hook init zsh)\"".to_string(),
            Shell::Fish => "omni hook init fish | source".to_string(),
            Shell::Posix => String::new(),
            Shell::Unknown(_) => String::new(),
        }
    }
}

fn determine_shell() -> String {
    for var in &["OMNI_SHELL", "SHELL"] {
        if let Some(shell) = std::env::var_os(var) {
            let shell = shell.to_str().unwrap();
            if !shell.is_empty() {
                if shell.contains('/') {
                    if let Some(shell) = shell.split('/').next_back() {
                        return shell.to_string();
                    }
                }
                return shell.to_string();
            }
        }
    }

    "bash".to_string()
}
