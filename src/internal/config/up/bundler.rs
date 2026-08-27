use std::path::Path;

use tokio::process::Command as TokioCommand;

use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::commands::utils::abs_path;
use crate::internal::config::global_config;
use crate::internal::config::up::utils::run_progress;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::RunConfig;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::user_interface::StringColor;

fn default_bundler_path() -> Option<String> {
    Some(UpConfigBundler::DEFAULT_PATH.to_string())
}

/// Configuration for bundler operation.
///
/// Accepts either:
/// - A string: interpreted as the gemfile path
/// - An object with `gemfile` and `path` fields
#[derive(Debug, Clone, feuilletage::Config)]
#[feuilletage(scalar_as = "gemfile")]
pub struct UpConfigBundler {
    #[feuilletage(skip_if_empty)]
    pub gemfile: Option<String>,
    #[feuilletage(default_fn = "default_bundler_path", skip_if_empty)]
    pub path: Option<String>,
}

impl Default for UpConfigBundler {
    fn default() -> Self {
        UpConfigBundler {
            gemfile: None,
            path: Some(Self::DEFAULT_PATH.to_string()),
        }
    }
}

impl UpConfigBundler {
    const DEFAULT_PATH: &'static str = "vendor/bundle";

    fn gemfile_abs_path(&self) -> String {
        let gemfile = if let Some(gemfile) = &self.gemfile {
            gemfile.clone()
        } else {
            "Gemfile".to_string()
        };

        // make a path from the str
        let gemfile = Path::new(&gemfile);

        abs_path(gemfile).to_str().unwrap().to_string()
    }

    pub fn up(
        &self,
        _options: &UpOptions,
        environment: &mut UpEnvironment,
        progress_handler: &UpProgressHandler,
    ) -> Result<(), UpError> {
        progress_handler.init("bundler".light_blue());

        if !global_config()
            .up_command
            .operations
            .is_operation_allowed("bundler")
        {
            let errmsg = "bundler operation is not allowed".to_string();
            progress_handler.error_with_message(errmsg.clone());
            return Err(UpError::Config(errmsg));
        }

        progress_handler.progress("install Gemfile dependencies".to_string());

        if let Some(path) = &self.path {
            progress_handler.progress("setting bundle path".to_string());

            let mut bundle_config = TokioCommand::new("bundle");
            bundle_config.arg("config");
            bundle_config.arg("--local");
            bundle_config.arg("path");
            bundle_config.arg(path);
            bundle_config.stdout(std::process::Stdio::piped());
            bundle_config.stderr(std::process::Stdio::piped());

            run_progress(
                &mut bundle_config,
                Some(progress_handler),
                RunConfig::default(),
            )?;
        }

        progress_handler.progress("installing bundle".to_string());

        let mut bundle_install = TokioCommand::new("bundle");
        bundle_install.arg("install");
        if let Some(gemfile) = &self.gemfile {
            bundle_install.arg("--gemfile");
            bundle_install.arg(gemfile);
        }
        bundle_install.stdout(std::process::Stdio::piped());
        bundle_install.stderr(std::process::Stdio::piped());

        let result = run_progress(
            &mut bundle_install,
            Some(progress_handler),
            RunConfig::default(),
        );

        if let Err(err) = &result {
            progress_handler.error_with_message(format!("bundle install failed: {err}"));
            return result;
        }

        environment.add_env_var("BUNDLE_GEMFILE", &self.gemfile_abs_path());

        progress_handler.success();

        Ok(())
    }

    pub fn down(&self, progress_handler: &UpProgressHandler) -> Result<(), UpError> {
        progress_handler.init("bundler".light_blue());
        progress_handler.progress("removing Gemfile dependencies".to_string());

        // Check if path exists, and if so delete it
        if self.path.is_some() && Path::new(&self.path.clone().unwrap()).exists() {
            let path = self.path.clone().unwrap();
            let path = abs_path(path).to_str().unwrap().to_string();

            progress_handler.progress(format!("removing {path}"));

            if let Err(err) = std::fs::remove_dir_all(&path) {
                progress_handler.error_with_message(format!("failed to remove {path}: {err}"));
                return Err(UpError::Exec(format!("failed to remove {path}: {err}")));
            }

            // Cleanup the parents as long as they are empty directories
            let mut parent = Path::new(&path);
            while let Some(path) = parent.parent() {
                if let Err(_err) = std::fs::remove_dir(path) {
                    break;
                }
                parent = path;
            }

            progress_handler.success()
        } else {
            progress_handler.success_with_message("skipping (nothing to do)".light_black())
        }

        Ok(())
    }
}

// Manual FromContextValue implementation replaced by derive macro above
// The #[feuilletage(scalar_as = "gemfile")] attribute handles string-to-object wrapping
// The #[feuilletage(default = "vendor/bundle")] handles the default path value
