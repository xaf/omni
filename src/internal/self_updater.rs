use std::io;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::exit;
use std::process::Command as ProcessCommand;

use lazy_static::lazy_static;
use semver::Prerelease;
use semver::Version;
use serde::Deserialize;
use tokio::process::Command as TokioCommand;

use crate::internal::config::config;
use crate::internal::config::up::github_release::UpConfigGithubRelease;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::UpOptions;
use crate::internal::config::utils::is_executable;
use crate::internal::env::current_exe;
use crate::internal::env::homebrew_prefix;
use crate::internal::env::homebrew_repository;
use crate::internal::user_interface::colors::StringColor;
use crate::internal::ConfigLoader;
use crate::internal::ConfigValue;
use crate::omni_error;
use crate::omni_info;

lazy_static! {

    static ref CURRENT_VERSION: Version = {
        let mut version = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
        if !version.pre.is_empty() {
            // Check if it starts with `rc` or `beta` or `alpha`, in which case
            // we wanna keep them, otherwise we consider we're at the version,
            // as otherwise semver would consider `1.0.0-5-xxxx` < `1.0.0`
            if !(version.pre.starts_with("rc")
                || version.pre.starts_with("beta")
                || version.pre.starts_with("alpha"))
            {
                // Clear prerelease
                version.pre = Prerelease::EMPTY;
            }
        }
        version
    };

    static ref INSTALLED_WITH_BREW: bool = BREW_INSTALL_DETAILS.0;

    static ref UPDATABLE_WITH_BREW: bool = BREW_INSTALL_DETAILS.1;

    static ref BREW_INSTALL_DETAILS: (bool, bool) = {
        let current_exe = current_exe();

        // Exit early if not possible to find the brew prefix
        let homebrew_prefix = match homebrew_prefix() {
            Some(prefix) => PathBuf::from(prefix),
            None => return (false, false),
        };

        // Exit early if not installed with brew
        if !current_exe.starts_with(&homebrew_prefix) {
            return (false, false);
        }

        // Try and resolve the real path, considering homebrew
        // uses symlinks for its executables in bin/
        let mut real_current_exe = current_exe.clone();
        if let Ok(real_path) = std::fs::canonicalize(&current_exe) {
            real_current_exe = real_path;
        }

        // Remove the <prefix>/Cellar piece, the next piece
        // is the formula name, which is what we want to extract;
        // if we can't find that piece, let's return false
        let parts = match real_current_exe
            .strip_prefix(&homebrew_prefix)
            .ok()
            .and_then(|p| p.to_str())
            .map(|s| s.split('/').collect::<Vec<&str>>())
        {
            Some(parts) => parts,
            None => return (false, false),
        };

        // The first part should be Cellar, otherwise we can't
        // find the formula name
        if parts.is_empty() || parts[0] != "Cellar" {
            return (false, false);
        }

        // The next part is the part we want
        let formula_name = if parts.len() > 1 {
            parts[1].to_string()
        } else {
            return (false, false);
        };

        // To be updatable, we need to not have a version
        // in the formula name, i.e. `omni@1.0.0` is not
        // updatable
        let is_updatable = !formula_name.contains('@');

        (true, is_updatable)
    };
}

pub fn self_update(explicit: bool) {
    if !explicit {
        // Check if OMNI_SKIP_SELF_UPDATE is set
        if let Some(skip_self_update) = std::env::var_os("OMNI_SKIP_SELF_UPDATE") {
            if !skip_self_update.to_str().unwrap().is_empty() {
                return;
            }
        }

        let config = config(".");
        if config.path_repo_updates.self_update.do_not_check() {
            return;
        }
    }

    if *INSTALLED_WITH_BREW && !(*UPDATABLE_WITH_BREW) {
        // If installed with brew, but not updatable, we can
        // just return, as there are no cases where we would
        // want to update
        if explicit {
            omni_info!("omni is installed using a versioned formula");
            omni_info!(format!(
                "please use {} to install a more recent version",
                "brew".light_yellow()
            ));
            exit(1);
        }
        return;
    }

    if let Some(omni_release) = OmniRelease::latest() {
        omni_release.check_and_update(explicit);
    }
}

#[derive(Debug, Deserialize)]
struct OmniRelease {
    version: String,
}

impl OmniRelease {
    fn latest() -> Option<Self> {
        let json_url =
            "https://raw.githubusercontent.com/xaf/homebrew-omni/main/Formula/resources/omni.json";

        let response = reqwest::blocking::get(json_url);
        if let Err(_err) = response {
            return None;
        }
        let mut response = response.unwrap();

        let mut content = String::new();
        response
            .read_to_string(&mut content)
            .expect("Failed to read response");

        let json: Result<OmniRelease, _> = serde_json::from_str(content.as_str());
        if let Err(err) = json {
            dbg!("Failed to parse latest release: {:?}", err);
            return None;
        }
        let json = json.unwrap();

        Some(json)
    }

    fn is_newer(&self) -> bool {
        match Version::parse(self.version.as_str()) {
            Ok(version) => version > *CURRENT_VERSION,
            Err(_err) => {
                omni_error!(format!("Failed to parse release version: {}", self.version));
                false
            }
        }
    }

    fn is_binary_version(&self) -> Result<bool, String> {
        // Get the current version from the binary at the path
        // of the current exe -- if it has been updated, it should
        // return the new version
        match ProcessCommand::new(current_exe()).arg("--version").output() {
            Ok(output) => {
                let output = String::from_utf8_lossy(&output.stdout);
                let output = output.trim();
                let version = output.split_whitespace().last().unwrap_or_default();

                let expected_version = Version::parse(self.version.as_str())
                    .expect("failed to parse expected version");

                match Version::parse(version) {
                    Ok(version) => Ok(version == expected_version),
                    Err(err) => Err(format!(
                        "failed to parse binary version '{version}': {err:?}"
                    )),
                }
            }
            Err(err) => Err(format!("failed to get binary version: {err:?}")),
        }
    }

    /// Check if we have write permissions for the current exe and for the directory
    /// of the current exe, since this is required for the self-update to work
    fn check_write_permissions(&self) -> bool {
        let current_exe = current_exe();
        if !current_exe.exists() {
            return false;
        }

        // Check first the exe itself
        match std::fs::metadata(&current_exe) {
            Ok(metadata) => {
                if metadata.permissions().readonly() {
                    return false;
                }
            }
            Err(_) => return false,
        }

        // Check the directory of the exe
        let parent = match current_exe.parent() {
            Some(parent) => parent,
            None => return false,
        };

        match std::fs::metadata(parent) {
            Ok(metadata) => {
                if metadata.permissions().readonly() {
                    return false;
                }
            }
            Err(_) => return false,
        }

        true
    }

    fn check_and_update(&self, explicit: bool) {
        let config = config(".");

        let desc = format!("{} update:", "omni".light_cyan()).light_blue();
        let progress_handler = UpProgressHandler::new(None);
        progress_handler.init(desc);

        progress_handler.progress("Checking for updates".to_string());

        if !self.is_newer() {
            progress_handler.success_with_message("already up-to-date".light_black());
            return;
        }

        let can_update = self.check_write_permissions() || *INSTALLED_WITH_BREW;
        let disabled_self_update = config.path_repo_updates.self_update.is_false();
        if disabled_self_update || !can_update {
            let msg = format!(
                "{} version {} is available{}",
                "omni:".light_cyan(),
                self.version.light_blue(),
                if disabled_self_update {
                    "".to_string()
                } else {
                    format!("; use {} to update", "sudo omni --update".light_yellow())
                }
            );
            progress_handler.success_with_message(msg);
            return;
        }

        if config.path_repo_updates.self_update.is_ask() {
            progress_handler.hide();

            let question = requestty::Question::expand("do_you_want_to_update")
                .ask_if_answered(true)
                .on_esc(requestty::OnEsc::Terminate)
                .message(format!(
                    "{} version {} is available; {}",
                    "omni:".light_cyan(),
                    self.version.light_blue(),
                    "do you want to install it?".yellow(),
                ))
                .choices(vec![
                    ('a', "Yes, always (update without asking in the future)"),
                    ('y', "Yes, this time (and ask me everytime)"),
                    ('n', "No"),
                    ('x', "No, never (skip without asking in the future)"),
                ])
                .default('y')
                .build();

            if !match requestty::prompt_one(question) {
                Ok(answer) => match answer {
                    requestty::Answer::ExpandItem(expanditem) => match expanditem.key {
                        'a' => self.edit_config_file_self_update(true),
                        'y' => true,
                        'n' => false,
                        'x' => self.edit_config_file_self_update(false),
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                },
                Err(err) => {
                    println!("{}", format!("[✘] {err:?}").red());
                    return;
                }
            } {
                return;
            }

            progress_handler.show();
        }

        let updated = if *INSTALLED_WITH_BREW {
            self.brew_upgrade(&progress_handler)
        } else {
            self.download(&progress_handler)
        };

        let updated = match updated {
            Ok(updated) => updated,
            Err(err) => {
                progress_handler.error_with_message(format!("failed to update: {err}"));
                return;
            }
        };

        match self.is_binary_version() {
            Ok(true) => {}
            Ok(false) => {
                progress_handler
                    .error_with_message("failed to update: binary version mismatch".to_string());
                return;
            }
            Err(err) => {
                progress_handler.error_with_message(err);
                return;
            }
        }

        if updated {
            progress_handler
                .success_with_message(format!("updated to version {}", self.version).light_green());

            // If this was an explicit self-update request, just exit cleanly
            // No need to re-execute - the update is complete
            if explicit {
                return;
            }

            // For background updates (during omni up, etc.), replace current process
            // with the new binary to ensure subsequent operations use the updated version
            let err = ProcessCommand::new(std::env::current_exe().unwrap())
                .args(std::env::args().skip(1))
                // We want to force the update, since by replacing the current
                // process, we're going to skip the rest of the updates otherwise
                .env("OMNI_FORCE_UPDATE", "1")
                // We want to skip the self-update, since we're already doing it
                // here, and we don't want to do it again when the new binary starts
                .env("OMNI_SKIP_SELF_UPDATE", "1")
                .exec();

            panic!("Failed to replace current process with the new binary: {err:?}");
        } else {
            progress_handler.success_with_message("already up-to-date".light_black());
        }
    }

    fn edit_config_file_self_update(&self, self_update: bool) -> bool {
        if let Err(err) = ConfigLoader::edit_main_user_config_file(|config_value| {
            let insert_value = if self_update { "true" } else { "false" };

            if let Some(config_path) = config_value.get_as_table_mut("path_repo_updates") {
                config_path.insert(
                    "self_update".to_string(),
                    ConfigValue::from_str(insert_value).expect("failed to create config value"),
                );
            } else if let Some(config_value_table) = config_value.as_table_mut() {
                config_value_table.insert(
                    "path_repo_updates".to_string(),
                    ConfigValue::from_str(format!("self_update: {insert_value}").as_str())
                        .expect("failed to create config value"),
                );
            } else {
                *config_value = ConfigValue::from_str(
                    format!("path_repo_updates:\n  self_update: {insert_value}").as_str(),
                )
                .expect("failed to create config value");
            }

            true
        }) {
            omni_error!(format!("failed to update configuration file: {:?}", err,));
        }

        self_update
    }

    fn brew_upgrade(&self, progress_handler: &UpProgressHandler) -> io::Result<bool> {
        progress_handler.progress("updating with homebrew".to_string());

        // We need to make sure first that the tap is up-to-date;
        // since we don't want to update the whole of homebrew,
        // which could take a while, we can use `git pull` in the
        // tap directory to update it
        let mut git_pull = TokioCommand::new("git");
        git_pull.arg("pull");
        git_pull.current_dir(
            Path::new(
                &homebrew_repository()
                    .ok_or_else(|| io::Error::other("failed to get homebrew repository"))?,
            )
            .join("Library")
            .join("Taps")
            .join("xaf")
            .join("homebrew-omni"),
        );
        git_pull.stdout(std::process::Stdio::piped());
        git_pull.stderr(std::process::Stdio::piped());

        let run = run_progress(&mut git_pull, Some(progress_handler), RunConfig::default());
        if let Err(err) = run {
            return Err(io::Error::other(err.to_string()));
        }

        let mut brew_upgrade = TokioCommand::new("brew");
        brew_upgrade.arg("upgrade");
        brew_upgrade.arg("xaf/omni/omni");
        brew_upgrade.env("HOMEBREW_NO_AUTO_UPDATE", "1");
        brew_upgrade.env("HOMEBREW_NO_INSTALL_UPGRADE", "1");
        brew_upgrade.stdout(std::process::Stdio::piped());
        brew_upgrade.stderr(std::process::Stdio::piped());

        let run = run_progress(
            &mut brew_upgrade,
            Some(progress_handler),
            RunConfig::default(),
        );
        if let Err(err) = run {
            return Err(io::Error::other(err.to_string()));
        }

        Ok(true)
    }

    fn download(&self, progress_handler: &UpProgressHandler) -> io::Result<bool> {
        // Create a GithubRelease configured for omni repository at the specified version
        let mut github_release = UpConfigGithubRelease::default();
        github_release.repository = "xaf/omni".to_string();
        github_release.version = Some(self.version.clone());

        // Fetch releases from GitHub
        progress_handler.progress("fetching releases from GitHub".to_string());
        let options = UpOptions::default();
        let releases = github_release
            .list_releases(&options, progress_handler)
            .map_err(|err| io::Error::other(format!("failed to list releases: {err}")))?;

        // Resolve the specific version release
        let release = github_release
            .resolve_release(&releases)
            .map_err(|err| io::Error::other(format!("failed to resolve release: {err}")))?;

        // Download, verify, and extract
        let tmp_dir = github_release
            .download_and_extract_to_temp(&release, progress_handler)
            .map_err(|err| io::Error::other(format!("failed to download release: {err}")))?;

        // Find the omni binary in the extracted content
        let search_pattern = format!("{}/*/omni", tmp_dir.path().display());
        let new_binary = glob::glob(&search_pattern)
            .map_err(|err| io::Error::other(format!("invalid glob pattern: {err}")))?
            .filter_map(|entry| entry.ok())
            .find(|path| path.is_file() && is_executable(path))
            .ok_or_else(|| io::Error::other("omni binary not found in release"))?;

        // Replace current binary with new binary
        progress_handler.progress("updating in-place".to_string());
        self_replace::self_replace(new_binary)?;

        progress_handler.progress("done".to_string());
        Ok(true)
    }
}
