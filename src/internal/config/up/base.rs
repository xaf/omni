use itertools::Itertools;
use serde::Serialize;

use crate::internal::cache::up_environments::UpEnvironment;
use crate::internal::cache::utils::Empty;
use crate::internal::cache::UpEnvironmentsCache;
use crate::internal::config::up::utils::cleanup_path;
use crate::internal::config::up::utils::reshim;
use crate::internal::config::up::utils::ProgressHandler;
use crate::internal::config::up::utils::UpProgressHandler;
use crate::internal::config::up::UpConfigCargoInstalls;
use crate::internal::config::up::UpConfigGithubReleases;
use crate::internal::config::up::UpConfigGoInstalls;
use crate::internal::config::up::UpConfigHomebrew;
use crate::internal::config::up::UpConfigMise;
use crate::internal::config::up::UpConfigTool;
use crate::internal::config::up::UpError;
use crate::internal::config::up::UpOptions;
use crate::internal::dynenv::update_dynamic_env_for_command;
use crate::internal::user_interface::colors::StringColor;
use crate::internal::workdir;
use crate::omni_warning;

#[derive(Debug, Clone)]
pub struct UpConfig {
    pub steps: Vec<UpConfigTool>,
    pub errors: Vec<UpError>,
}

impl Empty for UpConfig {
    fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

impl Serialize for UpConfig {
    // Serialization of UpConfig is serialization of the steps
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.steps.serialize(serializer)
    }
}

impl UpConfig {
    pub fn errors(&self) -> Vec<UpError> {
        self.errors.clone()
    }

    pub fn has_steps(&self) -> bool {
        !self.steps.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn clear_cache() {
        let workdir = workdir(".");
        if let Some(workdir_id) = workdir.id() {
            if let Err(err) = UpEnvironmentsCache::get().clear(&workdir_id) {
                omni_warning!(format!("failed to update cache: {}", err));
            }
        }
    }

    pub fn up(&self, options: &UpOptions, environment: &mut UpEnvironment) -> Result<(), UpError> {
        // Get current directory
        let current_dir = std::env::current_dir().expect("Failed to get current directory");

        // Filter the steps to only the available ones
        let steps = self
            .steps
            .iter()
            .filter(|step| step.is_available())
            .collect::<Vec<&UpConfigTool>>();

        // Go through the steps
        let num_steps = steps.len() + 2;
        for (idx, step) in steps.iter().enumerate() {
            // Make sure that we're in the right directory
            let step_dir = current_dir.join(step.dir().unwrap_or("".to_string()));
            if let Err(error) = std::env::set_current_dir(&step_dir) {
                return Err(UpError::Exec(format!(
                    "failed to change directory to {}: {}",
                    step_dir.display(),
                    error
                )));
            }

            let mut progress_handler = UpProgressHandler::new(Some((idx + 1, num_steps)));
            if let Some(sync_file) = &options.lock_file {
                progress_handler.set_sync_file(sync_file);
            }

            step.up(options, environment, &progress_handler)?
        }

        // Save and assign the environment
        self.assign_environment(environment, Some((num_steps - 1, num_steps)), options)?;

        // Cleanup anything that's not needed
        self.cleanup(Some((num_steps, num_steps)), options)?;

        Ok(())
    }

    fn assign_environment(
        &self,
        environment: &mut UpEnvironment,
        progress: Option<(usize, usize)>,
        options: &UpOptions,
    ) -> Result<(), UpError> {
        let mut progress_handler = UpProgressHandler::new(progress);
        if let Some(sync_file) = &options.lock_file {
            progress_handler.set_sync_file(sync_file);
        }
        progress_handler.init("apply environment:".light_blue());

        let workdir = workdir(".");
        let workdir_id = match workdir.id() {
            Some(workdir_id) => workdir_id,
            None => {
                let err = "failed to get workdir id".to_string();
                progress_handler.error_with_message(err.clone());
                return Err(UpError::Exec(err));
            }
        };

        // Assign the version id to the workdir now that we have successfully set it up
        progress_handler.progress("associating workdir to environment".to_string());
        let (new_env, newly_assigned, assigned_environment) = UpEnvironmentsCache::get()
            .assign_environment(&workdir_id, options.commit_sha.clone(), environment)
            .map_err(|err| {
                progress_handler.error_with_message(format!("failed to update cache: {err}"));
                UpError::Cache(err.to_string())
            })?;

        if assigned_environment.is_empty() {
            progress_handler.error_with_message("failed to assign environment".to_string());
            return Err(UpError::Cache("failed to assign environment".to_string()));
        }

        // Go over the up configuration again, but this time to set the dependencies
        // as required by the `assigned_environment`
        if new_env {
            progress_handler.progress("committing environment dependencies".to_string());
            if let Err(err) = self.commit(options, &assigned_environment) {
                progress_handler.error_with_message(format!(
                    "failed to commit environment dependencies: {err}"
                ));
                return Err(UpError::Cache(err.to_string()));
            }
        }

        if newly_assigned {
            progress_handler.success_with_message("done".light_green());
        } else {
            progress_handler.success_with_message("already up-to-date".light_black());
        }

        Ok(())
    }

    fn commit(&self, options: &UpOptions, env_version_id: &str) -> Result<(), UpError> {
        // Filter the steps to only the available ones
        let steps = self
            .steps
            .iter()
            .filter(|step| step.is_available())
            .collect::<Vec<&UpConfigTool>>();

        // Go through the steps
        let num_steps = steps.len() + 1;
        for (idx, step) in steps.iter().enumerate() {
            let mut progress_handler = UpProgressHandler::new(Some((idx + 1, num_steps)));
            if let Some(sync_file) = &options.lock_file {
                progress_handler.set_sync_file(sync_file);
            }

            step.commit(options, env_version_id)?
        }

        Ok(())
    }

    pub fn down(&self, options: &UpOptions) -> Result<(), UpError> {
        // Filter the steps to only the available ones
        let steps = self
            .steps
            .iter()
            .filter(|step| step.is_available())
            .collect::<Vec<&UpConfigTool>>();

        // Go through the steps, in reverse
        let num_steps = steps.len();
        for (idx, step) in steps.iter().rev().enumerate() {
            // Update the dynamic environment so that if anything has changed
            // the command can consider it right away
            update_dynamic_env_for_command(".");

            let mut progress_handler = UpProgressHandler::new(Some((idx + 1, num_steps)));
            if let Some(sync_file) = &options.lock_file {
                progress_handler.set_sync_file(sync_file);
            }

            step.down(&progress_handler)?
        }

        // Cleanup anything that's not needed
        self.cleanup(Some((num_steps, num_steps)), options)?;

        Ok(())
    }

    /// Cleanup anything that's not needed anymore; this will call the cleanup
    /// method of every existing tool, so that it can cleanup dependencies from
    /// steps that do not exist anymore on top of previous versions of recently
    /// upgraded tools.
    pub fn cleanup(
        &self,
        progress: Option<(usize, usize)>,
        options: &UpOptions,
    ) -> Result<(), UpError> {
        let mut progress_handler = UpProgressHandler::new(progress);
        if let Some(sync_file) = &options.lock_file {
            progress_handler.set_sync_file(sync_file);
        }
        progress_handler.init("resources cleanup:".light_blue());

        let mut cleanups = vec![];

        // Call cleanup on the different operation types
        if let Some(cleanup) = UpConfigMise::cleanup(&progress_handler)? {
            cleanups.push(cleanup);
        }
        if let Some(cleanup) = UpConfigHomebrew::cleanup(&progress_handler)? {
            cleanups.push(cleanup);
        }
        if let Some(cleanup) = UpConfigGithubReleases::cleanup(&progress_handler)? {
            cleanups.push(cleanup);
        }
        if let Some(cleanup) = UpConfigGoInstalls::cleanup(&progress_handler)? {
            cleanups.push(cleanup);
        }
        if let Some(cleanup) = UpConfigCargoInstalls::cleanup(&progress_handler)? {
            cleanups.push(cleanup);
        }

        // Then cleanup the data path
        if let Some(cleanup) = self.cleanup_data_path(&progress_handler)? {
            cleanups.push(cleanup);
        }

        // Then regenerate the shims
        if let Some(reshim) = reshim(&progress_handler)? {
            cleanups.push(reshim);
        }

        if cleanups.is_empty() {
            progress_handler.success_with_message("nothing to do".light_black());
        } else {
            progress_handler.success_with_message(cleanups.join(", "));
        }

        Ok(())
    }

    pub fn cleanup_data_path(
        &self,
        progress_handler: &dyn ProgressHandler,
    ) -> Result<Option<String>, UpError> {
        let wd = workdir(".");
        let wd_data_path = match wd.data_path() {
            Some(data_path) => data_path,
            None => return Ok(None),
        };

        // If the workdir data path does not exist, we're done
        if !wd_data_path.exists() {
            return Ok(None);
        }

        let expected_data_paths = self
            .steps
            .iter()
            .filter(|step| step.is_available() && step.was_upped())
            .flat_map(|step| step.data_paths())
            .filter(|data_path| data_path.starts_with(wd_data_path))
            .sorted()
            .dedup()
            .collect::<Vec<_>>();

        let (root_removed, num_removed, _) =
            cleanup_path(wd_data_path, expected_data_paths, progress_handler, true)?;

        if root_removed {
            return Ok(Some(format!(
                "removed workdir data path {}",
                wd_data_path.display().to_string().light_yellow()
            )));
        }

        if num_removed == 0 {
            return Ok(None);
        }

        Ok(Some(format!(
            "removed {} entr{} from the data path",
            num_removed.to_string().light_yellow(),
            if num_removed > 1 { "ies" } else { "y" }
        )))
    }
}

// ============================================================================
// Compote FromContextValue implementation for UpConfig
// ============================================================================
//
// This implementation delegates to UpConfigTool's derived FromContextValue
// for most cases, with special handling for int/float array elements
// (e.g., YAML `3.2` parsed as a float) which are treated as mise tool names.
// ============================================================================

impl<S: compote::CustomSource, L: compote::CustomLevel> compote::FromContextValue<S, L> for UpConfig {
    fn from_context_value(
        value: &compote::ContextValue<S, L>,
        tracker: &mut compote::ErrorTracker,
    ) -> Result<Self, compote::Error> {
        let mut steps = Vec::new();
        let mut up_errors = Vec::new();

        // The value must be an array of tool configurations
        let config_array = match value {
            compote::ContextValue::Array(arr, _) => arr,
            _ => {
                tracker.record_type_mismatch("array", value.type_name());
                return Ok(Self {
                    steps: Vec::new(),
                    errors: Vec::new(),
                });
            }
        };

        for (index, step_value) in config_array.iter().enumerate() {
            tracker.push_index(index);

            match step_value {
                // Int/float values in the array are treated as mise tool names
                // (e.g., YAML `3.2` parsed as a float key).
                // These won't match any scalar variant in UpConfigTool's derived
                // FromContextValue, so we handle them directly as Mise fallback.
                compote::ContextValue::Int(i, _) => {
                    let mut mise = UpConfigMise::default();
                    mise.requested_tool = i.to_string();
                    mise.process_from_tag();
                    steps.push(UpConfigTool::Mise(mise));
                }
                compote::ContextValue::Float(f, _) => {
                    let mut mise = UpConfigMise::default();
                    mise.requested_tool = f.to_string();
                    mise.process_from_tag();
                    steps.push(UpConfigTool::Mise(mise));
                }
                // All other types (string, object) delegate to UpConfigTool's
                // derived FromContextValue which handles external_tag dispatch
                _ => {
                    match <UpConfigTool as compote::FromContextValue<S, L>>::from_context_value(
                        step_value,
                        tracker,
                    ) {
                        Ok(mut up_config) => {
                            // Post-process Mise fallback variants to parse
                            // "backend:tool@version" from the injected tag
                            if let UpConfigTool::Mise(ref mut mise) = up_config {
                                mise.process_from_tag();
                            }
                            steps.push(up_config);
                        }
                        Err(_) => {
                            up_errors.push(UpError::Config(format!(
                                "invalid config for step {}",
                                index + 1
                            )));
                        }
                    }
                }
            }

            tracker.pop(); // pop index
        }

        Ok(Self {
            steps,
            errors: up_errors,
        })
    }
}
