use std::path::Path;

use serde::Deserialize;
use serde::Serialize;
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
use compote::ConfigError as CompoteConfigError;
use compote::ContextValue as CompoteConfigValue;
use compote::ErrorTracker as CompoteErrorTracker;
use compote::FromContextValue as CompoteFromConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpConfigBundler {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gemfile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

// Helper functions for compote conversion
fn compote_get_str_or_none(
    map: &indexmap::IndexMap<String, CompoteConfigValue>,
    key: &str,
    _tracker: &mut CompoteErrorTracker,
) -> Option<String> {
    map.get(key).and_then(|v| match v {
        CompoteConfigValue::String(s, _) => Some(s.clone()),
        _ => None,
    })
}

fn compote_get_str_or_default(
    map: &indexmap::IndexMap<String, CompoteConfigValue>,
    key: &str,
    default: &str,
    _tracker: &mut CompoteErrorTracker,
) -> String {
    compote_get_str_or_none(map, key, _tracker).unwrap_or_else(|| default.to_string())
}

impl CompoteFromConfigValue for UpConfigBundler {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteConfigError> {
        match value {
            CompoteConfigValue::Object(map, _) => {
                let gemfile = compote_get_str_or_none(map, "gemfile", tracker);
                let path = Some(compote_get_str_or_default(
                    map,
                    "path",
                    Self::DEFAULT_PATH,
                    tracker,
                ));

                Ok(Self { gemfile, path })
            }
            CompoteConfigValue::String(s, _) => Ok(Self {
                gemfile: Some(s.clone()),
                ..Self::default()
            }),
            _ => {
                tracker.record_type_mismatch("table or string", value.type_name());
                Ok(Self::default())
            }
        }
    }
}
