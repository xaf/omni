use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::exit;

use itertools::Itertools;

use compote::ConfigWarning;
use compote::Error as CompoteError;
use compote::ErrorTracker;
use compote::Format;
use compote::FromContextValue;
use compote::Level;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::frompath::PathCommand;
use crate::internal::commands::Command;
use crate::internal::config::compote_loader::OmniConfigLoader;
use crate::internal::config::config;
use crate::internal::config::parser::path_pattern_from_str;
use crate::internal::config::parser::ConfigError;
use crate::internal::config::parser::ConfigErrorHandler;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::utils::check_allowed;
use crate::internal::config::CommandSyntax;
use crate::internal::config::ConfigLoader;
use crate::internal::config::OmniConfig;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::env::omnipath_env;
use crate::internal::git::is_path_gitignored;
use crate::internal::git::package_root_path;
use crate::internal::user_interface::StringColor;
use crate::internal::workdir;
use crate::omni_error;

#[derive(Debug, Clone)]
struct ConfigCheckCommandArgs {
    search_paths: HashSet<String>,
    config_files: HashSet<String>,
    include_packages: bool,
    global_scope: bool,
    local_scope: bool,
    default_scope: bool,
    ignore_errors: HashSet<String>,
    select_errors: HashSet<String>,
    patterns: Vec<String>,
    output: ConfigCheckCommandOutput,
}

impl From<BTreeMap<String, ParseArgsValue>> for ConfigCheckCommandArgs {
    fn from(args: BTreeMap<String, ParseArgsValue>) -> Self {
        let search_paths = match args.get("search_path") {
            Some(ParseArgsValue::ManyString(search_paths)) => {
                search_paths.iter().flat_map(|v| v.clone()).collect()
            }
            _ => HashSet::new(),
        };

        let config_files = match args.get("config_file") {
            Some(ParseArgsValue::ManyString(config_files)) => {
                config_files.iter().flat_map(|v| v.clone()).collect()
            }
            _ => HashSet::new(),
        };

        let include_packages = matches!(
            args.get("include_packages"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );

        let global_scope = matches!(
            args.get("global"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let local_scope = matches!(
            args.get("local"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let default_scope = !global_scope && !local_scope;

        let ignore_errors = match args.get("ignore") {
            Some(ParseArgsValue::ManyString(ignore_errors)) => {
                ignore_errors.iter().flat_map(|v| v.clone()).collect()
            }
            _ => HashSet::new(),
        };

        let select_errors = match args.get("select") {
            Some(ParseArgsValue::ManyString(select_errors)) => {
                select_errors.iter().flat_map(|v| v.clone()).collect()
            }
            _ => HashSet::new(),
        };

        let patterns = match args.get("pattern") {
            Some(ParseArgsValue::ManyString(patterns)) => {
                patterns.iter().flat_map(|v| v.clone()).collect()
            }
            _ => Vec::new(),
        };

        let output = match args.get("output") {
            Some(ParseArgsValue::SingleString(Some(value))) => match value.as_str() {
                "json" => ConfigCheckCommandOutput::Json,
                "plain" => ConfigCheckCommandOutput::Plain,
                _ => unreachable!("unknown value for output"),
            },
            _ => ConfigCheckCommandOutput::Plain,
        };

        Self {
            search_paths,
            config_files,
            include_packages,
            global_scope,
            local_scope,
            default_scope,
            ignore_errors,
            select_errors,
            patterns,
            output,
        }
    }
}

impl ConfigCheckCommandArgs {
    fn use_files_from_cli(&self) -> bool {
        !self.config_files.is_empty() || !self.search_paths.is_empty()
    }
}

#[derive(Debug, Clone)]
enum ConfigCheckCommandOutput {
    Plain,
    Json,
}

#[derive(Debug, Clone)]
pub struct ConfigCheckCommand {}

impl ConfigCheckCommand {
    pub fn new() -> Self {
        Self {}
    }
}

impl BuiltinCommand for ConfigCheckCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["config".to_string(), "check".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Check the configuration files and commands in the omnipath for errors\n",
                "\n",
                "This allows to report any error or potential error in the ",
                "configuration, or in any metadata for commands in the omnipath.\n",
            )
            .to_string(),
        )
    }

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            parameters: vec![
                SyntaxOptArg {
                    names: vec!["-P".to_string(), "--search-path".to_string()],
                    desc: Some(
                        concat!(
                            "Path to check for commands.\n",
                            "\n",
                            "Can be used multiple times. If neither this nor ",
                            "\033[1m--config-file\033[0m are provided, the current ",
                            "omnipath is checked.\n",
                        )
                        .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["-C".to_string(), "--config-file".to_string()],
                    desc: Some(
                        concat!(
                            "Configuration file to check.\n",
                            "\n",
                            "Can be used multiple times. If neither this nor ",
                            "\033[1m--search-path\033[0m are provided, the current ",
                            "configuration is checked.\n",
                        )
                        .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["-p".to_string(), "--include-packages".to_string()],
                    desc: Some("Include package errors in the check.".to_string()),
                    arg_type: SyntaxOptArgType::Flag,
                    conflicts_with: vec![
                        "local".to_string(),
                        "search-path".to_string(),
                        "config-file".to_string(),
                    ],
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--global".to_string()],
                    desc: Some(
                        "Check the global configuration files and omnipath only.".to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    conflicts_with: vec![
                        "local".to_string(),
                        "search-path".to_string(),
                        "config-file".to_string(),
                    ],
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--local".to_string()],
                    desc: Some(
                        "Check the local configuration files and omnipath only.".to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    conflicts_with: vec![
                        "global".to_string(),
                        "search-path".to_string(),
                        "config-file".to_string(),
                    ],
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--ignore".to_string()],
                    desc: Some("Error codes to ignore".to_string()),
                    arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                    value_delimiter: Some(','),
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--select".to_string()],
                    desc: Some("Error codes to select".to_string()),
                    arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                    value_delimiter: Some(','),
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--pattern".to_string()],
                    desc: Some(
                        concat!(
                            "Pattern of files to include (or exclude, if starting ",
                            "by '!') in the check.\n",
                            "\n",
                            "Allows for glob patterns to be used. If not passed, ",
                            "all files are included.\n",
                        )
                        .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["-o".to_string(), "--output".to_string()],
                    desc: Some("Output format".to_string()),
                    arg_type: SyntaxOptArgType::Enum(vec!["json".to_string(), "plain".to_string()]),
                    default: Some("plain".to_string()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        })
    }

    fn category(&self) -> Option<Vec<String>> {
        Some(vec!["General".to_string()])
    }

    fn exec(&self, argv: Vec<String>) {
        let command = Command::Builtin(self.clone_boxed());
        let args = ConfigCheckCommandArgs::from(
            command
                .exec_parse_args_typed(argv, self.name())
                .expect("should have args to parse"),
        );

        let wd = workdir(".");
        let wd_root = wd.root();

        if args.local_scope && wd_root.is_none() {
            omni_error!("Not in a worktree");
            exit(1);
        }

        let error_handler = ConfigErrorHandler::new();
        self.aggregate_config_errors(&error_handler, &args);
        self.aggregate_path_errors(&error_handler, &args);
        self.filter_and_print_errors(&error_handler, &args);
    }
}

impl ConfigCheckCommand {
    fn aggregate_config_errors(
        &self,
        error_handler: &ConfigErrorHandler,
        args: &ConfigCheckCommandArgs,
    ) {
        // Get all the available configuration files
        let config_files: Vec<(String, Level)> = if args.use_files_from_cli() {
            args.config_files
                .iter()
                .filter(|file| {
                    if !PathBuf::from(file).exists() {
                        omni_error!(format!("configuration file not found: {}", file));
                        exit(1);
                    }
                    true
                })
                .map(|file| (file.clone(), Level::Local))
                .collect()
        } else {
            ConfigLoader::all_config_files()
                .into_iter()
                .filter(|(_file, level)| match level {
                    Level::System => args.global_scope || args.default_scope,
                    Level::User => args.global_scope || args.default_scope,
                    Level::Local => args.local_scope || args.default_scope,
                })
                .collect()
        };

        for (file, level) in config_files {
            let Some(file_config) = deserialize_config_file(error_handler, &file, level) else {
                continue;
            };

            // Load the check configuration for the location of the file,
            // since we do not want to do local configuration checks that
            // are not relevant to the file / work directory of the file
            let local_check_config = config(&file).check;

            // Go over all the commands defined in the configuration;
            // commands can have subcommands, and subcommands can have
            // subsubcommands, etc. We want all that in a single list
            // to simplify some logic here
            let mut commands_to_process: Vec<_> = file_config.commands.into_iter().collect();
            let mut all_commands = vec![];
            while let Some((name, command)) = commands_to_process.pop() {
                all_commands.push((name.clone(), command.clone()));
                if let Some(subcommands) = command.subcommands {
                    commands_to_process.extend(
                        subcommands
                            .into_iter()
                            .map(|(n, c)| (format!("{name} {n}"), c)),
                    );
                }
            }

            for (command_name, command) in all_commands {
                // Validate the tags for the command
                let tags = command.tags;
                for (tag, filter) in local_check_config.tags.iter() {
                    if let Some(value) = tags.get(tag) {
                        if !filter.matches(value) {
                            error_handler
                                .with_key(&command_name)
                                .with_file(file.clone())
                                .with_context("tag", tag.to_string())
                                .with_expected(filter.to_string())
                                .with_actual(value.to_string())
                                .error(ConfigErrorKind::UserDefinedConfigCommandInvalidTagValue);
                        }
                    } else {
                        error_handler
                            .with_key(&command_name)
                            .with_file(file.clone())
                            .with_context("tag", tag.to_string())
                            .error(ConfigErrorKind::UserDefinedConfigCommandMissingTag);
                    }
                }
            }
        }
    }

    fn aggregate_path_errors(
        &self,
        error_handler: &ConfigErrorHandler,
        args: &ConfigCheckCommandArgs,
    ) {
        // Now go over all the paths in the omnipath, so we can report:
        // - Files without `chmod +x`
        // - Files with missing metadata
        // - Errors in the metadata files (yaml)
        // - Errors in the metadata headers

        let search_paths = if args.use_files_from_cli() {
            args.search_paths.clone()
        } else {
            // Use the configuration files to get the paths
            let config_files: Vec<_> = ConfigLoader::all_config_files()
                .into_iter()
                .filter(|(_file, level)| match level {
                    Level::System => args.global_scope || args.default_scope,
                    Level::User => args.global_scope || args.default_scope,
                    Level::Local => args.local_scope || args.default_scope,
                })
                .collect();

            let mut loader = OmniConfigLoader::new_from_files(config_files);
            let compote_config = match loader.build() {
                Ok(config) => config,
                Err(_) => {
                    // Return early with empty search paths
                    return;
                }
            };
            let mut tracker = ErrorTracker::new();
            let config: OmniConfig =
                OmniConfig::from_context_value(compote_config.root(), &mut tracker)
                    .unwrap_or_default();

            // Prepare the path list
            let mut paths = vec![];
            let mut seen = HashSet::new();

            // Read the prepend paths
            for path in config.path.prepend {
                if seen.insert(path.to_string()) {
                    paths.push(path.to_string());
                }
            }

            // If global, read the environment paths
            if args.global_scope || args.default_scope {
                for path in omnipath_env() {
                    if !path.is_empty() && seen.insert(path.clone()) {
                        paths.push(path.clone());
                    }
                }
            }

            // Read the append paths
            for path in config.path.append {
                if seen.insert(path.to_string()) {
                    paths.push(path.to_string());
                }
            }

            // TODO: If local, try and apply the `suggest_config` so that
            // we can evaluate any path that would be suggested to be added

            // Return all those paths
            paths.into_iter().collect()
        };

        for entry in search_paths {
            let path = PathBuf::from(&entry);
            if !path.exists() {
                error_handler
                    .with_file(entry)
                    .error(ConfigErrorKind::OmniPathNotFound);

                continue;
            }

            let path_error_handler = error_handler.with_file(&entry);
            for command in PathCommand::aggregate_with_errors(
                std::slice::from_ref(&entry),
                &path_error_handler,
            )
            .into_iter()
            .filter_map(|command| match command {
                Command::FromPath(path_command) => Some(path_command),
                _ => None,
            }) {
                command.check_errors(&path_error_handler);

                // Load the check configuration for the location of the file
                let local_check_config = config(&entry).check;

                // Validate the tags for the command
                let tags = command.tags();
                for (tag, filter) in local_check_config.tags.iter() {
                    if let Some(value) = tags.get(tag) {
                        if !filter.matches(value) {
                            path_error_handler
                                .with_file(command.source())
                                .with_context("tag", tag.to_string())
                                .with_expected(filter.to_string())
                                .with_actual(value.to_string())
                                .error(ConfigErrorKind::UserDefinedPathCommandInvalidTagValue);
                        }
                    } else {
                        path_error_handler
                            .with_file(command.source())
                            .with_context("tag", tag.to_string())
                            .error(ConfigErrorKind::UserDefinedPathCommandMissingTag);
                    }
                }
            }
        }
    }

    fn filter_and_print_errors(
        &self,
        error_handler: &ConfigErrorHandler,
        args: &ConfigCheckCommandArgs,
    ) {
        let cliarg_patterns: Vec<String> = args
            .patterns
            .iter()
            .map(|value| path_pattern_from_str(value, None, true))
            .collect();

        // Filter and sort the errors
        let errors = error_handler
            .errors()
            .into_iter()
            .filter(|e| {
                args.include_packages || !PathBuf::from(e.file()).starts_with(package_root_path())
            })
            .filter(|e| {
                // Load the check configuration for the location of the file
                let local_check_config = config(e.file()).check;

                // Get the patterns for this file
                let patterns: Vec<String> = cliarg_patterns
                    .iter()
                    .chain(&local_check_config.patterns())
                    .cloned()
                    .collect();

                // Check if the file is allowed
                if !check_allowed(e.file(), &patterns) {
                    return false;
                }

                // Get the selected and ignored errors
                let select_errors = args
                    .select_errors
                    .iter()
                    .chain(local_check_config.select.iter())
                    .map(|e| e.to_string())
                    .collect();

                let ignore_errors = args
                    .ignore_errors
                    .iter()
                    .chain(local_check_config.ignore.iter())
                    .map(|e| e.to_string())
                    .collect();

                // Check if the error is selected
                if !check_selected(e, &select_errors, &ignore_errors) {
                    return false;
                }

                // Check if the file is gitignored
                if is_path_gitignored(e.file()).unwrap_or(false) {
                    return false;
                }

                true
            })
            .sorted()
            .collect::<Vec<_>>();

        // Print the errors
        match args.output {
            ConfigCheckCommandOutput::Plain => {
                for error in errors.iter() {
                    println!("{error}");
                }
            }
            ConfigCheckCommandOutput::Json => match serde_json::to_string_pretty(&errors) {
                Ok(json) => println!("{json}"),
                Err(e) => {
                    omni_error!(format!("Error while serializing the errors to JSON: {}", e));
                }
            },
        }

        // Exit with the appropriate code
        exit(if errors.is_empty() { 0 } else { 1 });
    }
}

fn deserialize_config_file(
    error_handler: &ConfigErrorHandler,
    file: &str,
    level: Level,
) -> Option<OmniConfig> {
    let mut constraint_loader =
        OmniConfigLoader::new_from_file_with_format(file, Format::Yaml, level);
    let _ = constraint_loader.deserialize::<OmniConfig>();
    aggregate_compote_mutability_diagnostics(error_handler, &constraint_loader, file);

    // Config check validates every supplied value, including values that the
    // source level cannot apply. Mutability violations are reported separately.
    let mut loader = OmniConfigLoader::new_from_file_with_format(file, Format::Yaml, Level::User);
    let result = loader.deserialize::<OmniConfig>();
    let missing_command_runs = missing_command_run_paths(file);

    aggregate_compote_errors_excluding_command_runs(
        error_handler,
        loader.errors().errors(),
        file,
        &missing_command_runs,
    );
    aggregate_compote_warnings(error_handler, loader.errors().warnings(), file);
    aggregate_missing_command_runs(error_handler, file, &missing_command_runs);

    match result {
        Ok(config) => Some(config),
        Err(error) => {
            aggregate_compote_errors_excluding_command_runs(
                error_handler,
                &[error],
                file,
                &missing_command_runs,
            );
            None
        }
    }
}

fn missing_command_run_paths(file: &str) -> HashSet<String> {
    let Ok(contents) = fs::read_to_string(file) else {
        return HashSet::new();
    };
    let Ok(config) = serde_yaml::from_str::<serde_yaml::Value>(&contents) else {
        return HashSet::new();
    };
    let Some(commands) = config
        .as_mapping()
        .and_then(|config| config.get("commands"))
        .and_then(serde_yaml::Value::as_mapping)
    else {
        return HashSet::new();
    };

    let mut paths = HashSet::new();
    collect_missing_command_run_paths(commands, "commands", &mut paths);
    paths
}

fn collect_missing_command_run_paths(
    commands: &serde_yaml::Mapping,
    prefix: &str,
    paths: &mut HashSet<String>,
) {
    for (name, command) in commands {
        let Some(name) = name.as_str() else {
            continue;
        };
        let command_path = format!("{prefix}.{name}");

        if command.is_null() {
            paths.insert(format!("{command_path}.run"));
            continue;
        }

        let Some(command) = command.as_mapping() else {
            continue;
        };
        if !command.contains_key("run") {
            paths.insert(format!("{command_path}.run"));
        }
        if let Some(subcommands) = command
            .get("subcommands")
            .and_then(serde_yaml::Value::as_mapping)
        {
            collect_missing_command_run_paths(
                subcommands,
                &format!("{command_path}.subcommands"),
                paths,
            );
        }
    }
}

fn aggregate_missing_command_runs(
    error_handler: &ConfigErrorHandler,
    file: &str,
    paths: &HashSet<String>,
) {
    for path in paths {
        error_handler
            .with_file(file)
            .diagnostic("C001", format!("key '{path}' is missing"), false);
    }
}

fn aggregate_compote_errors_excluding_command_runs(
    error_handler: &ConfigErrorHandler,
    errors: &[CompoteError],
    fallback_file: &str,
    missing_command_runs: &HashSet<String>,
) {
    let errors = errors
        .iter()
        .filter(|error| !command_run_error_is_replaced(error, missing_command_runs))
        .cloned()
        .collect::<Vec<_>>();
    aggregate_compote_errors(error_handler, &errors, fallback_file);
}

fn command_run_error_is_replaced(
    error: &CompoteError,
    missing_command_runs: &HashSet<String>,
) -> bool {
    match error {
        CompoteError::MissingField { path } => missing_command_runs.contains(path),
        CompoteError::InvalidValue { path, message } => {
            missing_command_runs.contains(path)
                && message.starts_with("required field 'run' was not provided")
        }
        CompoteError::TypeMismatch {
            path,
            expected,
            actual,
        } => {
            expected == "object"
                && actual == "null"
                && missing_command_runs.contains(&format!("{path}.run"))
        }
        _ => false,
    }
}

fn aggregate_compote_mutability_diagnostics(
    error_handler: &ConfigErrorHandler,
    loader: &OmniConfigLoader,
    file: &str,
) {
    aggregate_compote_warnings(error_handler, loader.errors().warnings(), file);

    for error in loader.errors().errors() {
        if let CompoteError::InvalidValue { path, message } = error {
            if message.contains("can only be set by levels") {
                error_handler.with_file(file).diagnostic(
                    "C110",
                    format!("unsupported value at config path '{path}': {message}"),
                    true,
                );
            }
        }
    }
}

fn aggregate_compote_errors(
    error_handler: &ConfigErrorHandler,
    errors: &[CompoteError],
    fallback_file: &str,
) {
    for error in errors {
        let (file, message) = compote_error_details(error, fallback_file);
        error_handler
            .with_file(file)
            .diagnostic(error.code(), message, false);
    }
}

fn aggregate_compote_warnings(
    error_handler: &ConfigErrorHandler,
    warnings: &[ConfigWarning],
    file: &str,
) {
    for warning in warnings {
        error_handler.with_file(file).diagnostic(
            "C110",
            format!(
                "unsupported value at config path '{}': {}",
                warning.path, warning.message
            ),
            true,
        );
    }
}

fn compote_error_details(error: &CompoteError, fallback_file: &str) -> (String, String) {
    match error {
        CompoteError::MissingField { path } => (
            fallback_file.to_string(),
            format!("missing required field at config path '{path}'"),
        ),
        CompoteError::TypeMismatch {
            path,
            expected,
            actual,
        } => (
            fallback_file.to_string(),
            format!("invalid value at config path '{path}': expected {expected}, got {actual}"),
        ),
        CompoteError::InvalidValue { path, message } => (
            fallback_file.to_string(),
            format!("invalid value at config path '{path}': {message}"),
        ),
        CompoteError::MergeConflict { path, message } => (
            fallback_file.to_string(),
            format!("merge conflict at config path '{path}': {message}"),
        ),
        CompoteError::ImmutableOverride { path, source } => (
            fallback_file.to_string(),
            format!("source '{source}' cannot override immutable config path '{path}'"),
        ),
        CompoteError::ParseError { source, message } => (source.clone(), message.clone()),
        CompoteError::FormatNotSupported { format, message } => (
            fallback_file.to_string(),
            format!("unsupported config format '{format}': {message}"),
        ),
        CompoteError::IoError { path, message } => (path.clone(), message.clone()),
        CompoteError::Custom { path, message, .. } => (
            fallback_file.to_string(),
            format!("error at config path '{path}': {message}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::Builder;

    use super::*;

    #[test]
    fn malformed_yaml_is_aggregated_with_its_source_file() {
        let file = Builder::new().suffix(".yaml").tempfile().unwrap();
        fs::write(file.path(), "commands:\n  broken: [\n").unwrap();
        let file_path = file.path().to_string_lossy().to_string();

        let error_handler = ConfigErrorHandler::new();
        deserialize_config_file(&error_handler, &file_path, Level::Local);

        let errors = error_handler.errors();
        let parse_errors: Vec<_> = errors
            .iter()
            .filter(|error| error.errorcode() == "C120")
            .collect();
        assert_eq!(parse_errors.len(), 1, "{errors:#?}");
        assert_eq!(parse_errors[0].file(), file_path);
        assert_eq!(parse_errors[0].lineno(), 0);
        assert!(!parse_errors[0].message().is_empty());

        let selected = HashSet::from(["C120".to_string()]);
        assert!(check_selected(parse_errors[0], &selected, &HashSet::new()));
        let json = serde_json::to_value(parse_errors[0]).unwrap();
        assert_eq!(json["file"], file_path);
        assert_eq!(json["lineno"], 0);
        assert_eq!(json["errorcode"], "C120");
    }

    #[test]
    fn non_fatal_deserialization_error_is_aggregated_with_its_config_path() {
        let file = Builder::new().suffix(".yaml").tempfile().unwrap();
        fs::write(file.path(), "command_match_min_score: invalid\n").unwrap();
        let file_path = file.path().to_string_lossy().to_string();

        let error_handler = ConfigErrorHandler::new();
        let config = deserialize_config_file(&error_handler, &file_path, Level::Local);
        assert!(config.is_some());

        let errors = error_handler.errors();
        assert!(errors.iter().all(|error| error.file() == file_path));
        assert!(errors.iter().all(|error| error.lineno() == 0));
        assert_eq!(
            errors
                .iter()
                .filter(|error| {
                    error.errorcode() == "C102"
                        && error.message().contains("command_match_min_score")
                })
                .count(),
            1,
            "{errors:#?}",
        );
    }

    #[test]
    fn mutable_by_warning_is_reported_as_c110_without_c102_for_that_field() {
        let file = Builder::new().suffix(".yaml").tempfile().unwrap();
        fs::write(file.path(), "org:\n  - handle: acme\n").unwrap();
        let file_path = file.path().to_string_lossy().to_string();

        let error_handler = ConfigErrorHandler::new();
        let config = deserialize_config_file(&error_handler, &file_path, Level::Local);
        assert!(config.is_some());

        let errors = error_handler.errors();
        let warning = errors
            .iter()
            .find(|error| error.errorcode() == "C110")
            .unwrap_or_else(|| panic!("missing C110 warning: {errors:#?}"));
        assert_eq!(warning.file(), file_path);
        assert_eq!(warning.lineno(), 0);
        assert!(warning.message().contains("org"));
        assert!(warning.default_ignored());
        assert!(!errors
            .iter()
            .any(|error| { error.errorcode() == "C102" && error.message().contains("org") }));

        assert!(!check_selected(warning, &HashSet::new(), &HashSet::new()));
        assert!(check_selected(
            warning,
            &HashSet::from(["C110".to_string()]),
            &HashSet::new()
        ));
    }

    #[test]
    fn rechecking_same_file_deduplicates_backend_diagnostics() {
        let file = Builder::new().suffix(".yaml").tempfile().unwrap();
        fs::write(file.path(), "command_match_min_score: invalid\n").unwrap();
        let file_path = file.path().to_string_lossy().to_string();
        let error_handler = ConfigErrorHandler::new();

        deserialize_config_file(&error_handler, &file_path, Level::Local);
        let first_errors = error_handler.errors();
        deserialize_config_file(&error_handler, &file_path, Level::Local);

        assert_eq!(error_handler.errors(), first_errors);
    }

    #[test]
    fn backend_dedup_preserves_same_diagnostic_from_different_files() {
        let error_handler = ConfigErrorHandler::new();

        error_handler
            .with_file("first.yaml")
            .diagnostic("C102", "same diagnostic", false);
        error_handler
            .with_file("second.yaml")
            .diagnostic("C102", "same diagnostic", false);

        assert_eq!(error_handler.errors().len(), 2);
    }

    #[test]
    fn backend_errors_are_not_ignored_by_default() {
        let error_handler = ConfigErrorHandler::new();
        error_handler
            .with_file("config.yaml")
            .diagnostic("C102", "backend error", false);

        let errors = error_handler.errors();
        assert_eq!(errors.len(), 1);
        assert!(!errors[0].default_ignored());
        assert!(check_selected(&errors[0], &HashSet::new(), &HashSet::new()));
    }

    #[test]
    fn command_run_validation_descends_into_invalid_parent_commands() {
        let file = Builder::new().suffix(".yaml").tempfile().unwrap();
        fs::write(
            file.path(),
            concat!(
                "commands:\n",
                "  empty:\n",
                "  parent:\n",
                "    subcommands:\n",
                "      child:\n",
                "        desc: missing run\n",
            ),
        )
        .unwrap();
        let file_path = file.path().to_string_lossy().to_string();

        let error_handler = ConfigErrorHandler::new();
        deserialize_config_file(&error_handler, &file_path, Level::Local);

        let errors = error_handler.errors();
        let missing = errors
            .iter()
            .filter(|error| error.errorcode() == "C001")
            .map(ConfigError::message)
            .collect::<HashSet<_>>();
        assert_eq!(
            missing,
            HashSet::from([
                "key 'commands.empty.run' is missing".to_string(),
                "key 'commands.parent.run' is missing".to_string(),
                "key 'commands.parent.subcommands.child.run' is missing".to_string(),
            ])
        );
        assert!(!errors.iter().any(|error| {
            error.errorcode() == "C101" && error.message().contains("commands.empty")
        }));
        assert!(!errors.iter().any(|error| {
            error.errorcode() == "C102"
                && error
                    .message()
                    .contains("required field 'run' was not provided")
        }));
    }
}

fn check_selected(
    error: &ConfigError,
    select_errors: &HashSet<String>,
    ignore_errors: &HashSet<String>,
) -> bool {
    // Filter according to the error code
    let errorcode = error.errorcode().to_uppercase();

    // Get the longest selected entry from which the error starts with
    let selected_level: i8 = select_errors
        .iter()
        .filter(|e| errorcode.starts_with(e.to_uppercase().as_str()))
        .map(|e| e.len() as i8)
        .max()
        .unwrap_or(if select_errors.is_empty() { 0 } else { -1 });

    // Skip this error if the selected_level < 0
    if selected_level < 0 || (error.default_ignored() && selected_level < 4) {
        return false;
    }

    // Get the longest ignored entry from which the error starts with
    let ignored_level: i8 = ignore_errors
        .iter()
        .filter(|e| errorcode.starts_with(e.to_uppercase().as_str()))
        .map(|e| e.len() as i8)
        .max()
        .unwrap_or(-1);

    // Skip this error if the ignored_level >= selected_level
    if ignored_level >= selected_level {
        return false;
    }

    true
}
