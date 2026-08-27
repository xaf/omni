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

fn empty_operation_name(tag: &str) -> Option<&'static str> {
    match tag {
        "cargo-install"
        | "cargo_install"
        | "cargoinstall"
        | "cargo-install-crates"
        | "cargo-install-crate"
        | "cargo-crates"
        | "cargo-crate" => Some("cargo-install"),
        "github-release" | "github_release" | "githubrelease" | "ghrelease" | "github-releases"
        | "github_releases" | "githubreleases" | "ghreleases" | "github" | "gh-release"
        | "gh-releases" => Some("github-release"),
        "go-install" | "go_install" | "goinstall" | "go-install-tools" | "go-install-tool"
        | "go-tools" | "go-tool" => Some("go-install"),
        _ => None,
    }
}

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
// Feuilletage FromContextValue implementation for UpConfig
// ============================================================================
//
// This implementation delegates to UpConfigTool's derived FromContextValue
// for most cases, with special handling for int/float array elements
// (e.g., YAML `3.2` parsed as a float) which are treated as mise tool names.
// ============================================================================

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel> feuilletage::FromContextValue<S, L>
    for UpConfig
{
    fn from_context_value(
        value: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        let mut steps = Vec::new();
        let mut up_errors = Vec::new();

        // The value must be an array of tool configurations
        let config_array = match value {
            feuilletage::ContextValue::Array(arr, _) => arr,
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

            let empty_operation = match step_value {
                feuilletage::ContextValue::String(tag, _) => empty_operation_name(tag),
                feuilletage::ContextValue::Object(values, _) if values.len() == 1 => values
                    .iter()
                    .next()
                    .filter(|(_, value)| value.is_null())
                    .and_then(|(tag, _)| empty_operation_name(tag)),
                _ => None,
            };

            if let Some(operation) = empty_operation {
                tracker.record(feuilletage::Error::Custom {
                    code: "C002".to_string(),
                    path: format!("{}.{}", tracker.current_path(), operation),
                    message: "operation details are empty".to_string(),
                });
                up_errors.push(UpError::Config(format!(
                    "{operation} operation details are empty"
                )));
                tracker.pop();
                continue;
            }

            match step_value {
                // A bare string names a tool without supplying configuration.
                // Parse it like the equivalent `{ tool: {} }` external tag so
                // the tool name is not mistaken for its version.
                feuilletage::ContextValue::String(tag, context) => {
                    let empty_config =
                        feuilletage::ContextValue::object(Default::default(), context.clone());
                    let tagged_value = feuilletage::ContextValue::object(
                        [(tag.clone(), empty_config)].into_iter().collect(),
                        context.clone(),
                    );

                    match <UpConfigTool as feuilletage::FromContextValue<S, L>>::from_context_value(
                        &tagged_value,
                        tracker,
                    ) {
                        Ok(mut up_config) => {
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
                // Int/float values in the array are treated as mise tool names
                // (e.g., YAML `3.2` parsed as a float key).
                // These won't match any scalar variant in UpConfigTool's derived
                // FromContextValue, so we handle them directly as Mise fallback.
                feuilletage::ContextValue::Int(i, _) => {
                    let mut mise = UpConfigMise::default();
                    mise.requested_tool = i.to_string();
                    mise.version = "latest".to_string();
                    mise.process_from_tag();
                    steps.push(UpConfigTool::Mise(mise));
                }
                feuilletage::ContextValue::Float(f, _) => {
                    let mut mise = UpConfigMise::default();
                    mise.requested_tool = f.to_string();
                    mise.version = "latest".to_string();
                    mise.process_from_tag();
                    steps.push(UpConfigTool::Mise(mise));
                }
                // All other types (string, object) delegate to UpConfigTool's
                // derived FromContextValue which handles external_tag dispatch
                _ => {
                    match <UpConfigTool as feuilletage::FromContextValue<S, L>>::from_context_value(
                        step_value, tracker,
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

#[cfg(test)]
mod tests {
    use feuilletage::FromContextValue;

    use super::*;

    fn parse_up(yaml: &str) -> (UpConfig, feuilletage::ErrorTracker) {
        let context = feuilletage::Context::new(feuilletage::Source::Programmatic, feuilletage::Level::User);
        let mut config = feuilletage::Config::default();
        config.load_yaml(yaml, context);
        let mut tracker = feuilletage::ErrorTracker::new();
        let up = UpConfig::from_context_value(config.root(), &mut tracker).unwrap();
        (up, tracker)
    }

    #[test]
    fn parses_nonempty_operation_aliases() {
        let (up, tracker) = parse_up(
            "- cargo-crate:\n    crate: ripgrep\n- gh-release:\n    repository: BurntSushi/ripgrep\n- go-tool:\n    path: golang.org/x/tools/gopls\n",
        );

        assert_eq!(up.steps.len(), 3);
        assert!(matches!(up.steps[0], UpConfigTool::CargoInstall(_)));
        assert!(matches!(up.steps[1], UpConfigTool::GithubRelease(_)));
        assert!(matches!(up.steps[2], UpConfigTool::GoInstall(_)));
        assert!(tracker.errors().is_empty(), "{:#?}", tracker.errors());
    }

    #[test]
    fn reports_empty_operations_without_dropping_runtime_validation() {
        let (up, tracker) = parse_up("- cargo-crate\n- gh-release:\n- go-tools\n");

        assert!(up.steps.is_empty());
        assert_eq!(
            up.errors,
            vec![
                UpError::Config("cargo-install operation details are empty".to_string()),
                UpError::Config("github-release operation details are empty".to_string()),
                UpError::Config("go-install operation details are empty".to_string()),
            ]
        );
        assert_eq!(tracker.errors().len(), 3);
        assert!(tracker.errors().iter().all(|error| error.code() == "C002"));
    }

    #[test]
    fn continues_parsing_after_an_empty_operation() {
        let (up, tracker) = parse_up("- go-install:\n- terraform\n");

        assert_eq!(up.steps.len(), 1);
        assert!(matches!(up.steps[0], UpConfigTool::Mise(_)));
        assert_eq!(
            up.errors,
            vec![UpError::Config(
                "go-install operation details are empty".to_string()
            )]
        );
        assert_eq!(tracker.errors().len(), 1);
        assert_eq!(tracker.errors()[0].code(), "C002");
    }

    #[test]
    fn reports_empty_operations_for_every_accepted_alias() {
        let aliases = [
            ("cargo-install", "cargo-install"),
            ("cargo_install", "cargo-install"),
            ("cargoinstall", "cargo-install"),
            ("cargo-install-crates", "cargo-install"),
            ("cargo-install-crate", "cargo-install"),
            ("cargo-crates", "cargo-install"),
            ("cargo-crate", "cargo-install"),
            ("github-release", "github-release"),
            ("github_release", "github-release"),
            ("githubrelease", "github-release"),
            ("ghrelease", "github-release"),
            ("github-releases", "github-release"),
            ("github_releases", "github-release"),
            ("githubreleases", "github-release"),
            ("ghreleases", "github-release"),
            ("github", "github-release"),
            ("gh-release", "github-release"),
            ("gh-releases", "github-release"),
            ("go-install", "go-install"),
            ("go_install", "go-install"),
            ("goinstall", "go-install"),
            ("go-install-tools", "go-install"),
            ("go-install-tool", "go-install"),
            ("go-tools", "go-install"),
            ("go-tool", "go-install"),
        ];

        for (alias, operation) in aliases {
            let (up, tracker) = parse_up(&format!("- {alias}\n- {alias}:\n"));

            assert!(up.steps.is_empty(), "alias {alias}");
            assert_eq!(
                up.errors,
                vec![
                    UpError::Config(format!("{operation} operation details are empty")),
                    UpError::Config(format!("{operation} operation details are empty")),
                ],
                "alias {alias}"
            );
            assert_eq!(tracker.errors().len(), 2, "alias {alias}");
            assert!(
                tracker.errors().iter().all(|error| error.code() == "C002"),
                "alias {alias}"
            );
        }
    }

    #[test]
    fn bare_scalars_name_tools_without_becoming_versions() {
        let (up, tracker) = parse_up("- python\n- node\n- terraform\n- 3.2\n");

        assert_eq!(up.steps.len(), 4);
        match &up.steps[0] {
            UpConfigTool::Python(config) => {
                assert_eq!(config.backend.requested_tool, "python");
                assert_eq!(config.backend.version, "latest");
                assert!(config.backend.retained_config_value().is_some());
            }
            other => panic!("expected python, got {other:?}"),
        }
        match &up.steps[1] {
            UpConfigTool::Nodejs(config) => {
                assert_eq!(config.backend.requested_tool, "node");
                assert_eq!(config.backend.version, "latest");
                assert!(config.backend.retained_config_value().is_some());
            }
            other => panic!("expected node, got {other:?}"),
        }
        match &up.steps[2] {
            UpConfigTool::Mise(config) => {
                assert_eq!(config.requested_tool, "terraform");
                assert_eq!(config.version, "latest");
            }
            other => panic!("expected mise, got {other:?}"),
        }
        match &up.steps[3] {
            UpConfigTool::Mise(config) => {
                assert_eq!(config.requested_tool, "3.2");
                assert_eq!(config.version, "latest");
            }
            other => panic!("expected mise, got {other:?}"),
        }
        assert!(tracker.errors().is_empty(), "{:#?}", tracker.errors());
    }
}
