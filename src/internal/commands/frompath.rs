use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use once_cell::sync::OnceCell;
use serde::Deserialize;
use serde::Serialize;
use walkdir::WalkDir;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::help_parser::ParsedCommandHelp;
use crate::internal::commands::help_parser::PathCommandHelpParser;
use crate::internal::commands::path::omnipath;
use crate::internal::commands::HelpCommand;
use crate::internal::config;
use crate::internal::config::config_loader;
use crate::internal::config::global_config;
use crate::internal::config::utils::is_executable;
use crate::internal::config::CommandSyntax;
use crate::internal::config::ConfigExtendOptions;
use crate::internal::config::OmniConfig;
use crate::internal::config::SyntaxOptArg;
use crate::internal::git::package_path_from_handle;
use crate::internal::workdir;
use crate::internal::workdir::is_trusted;

#[derive(Debug, Clone)]
pub struct PathCommand {
    name: Vec<String>,
    source: String,
    aliases: BTreeMap<Vec<String>, String>,
    file_details: OnceCell<Option<PathCommandFileDetails>>,
}

impl PathCommand {
    pub fn all() -> Vec<Self> {
        Self::aggregate_commands_from_path(&omnipath())
    }

    pub fn local() -> Vec<Self> {
        // Check if we are in a work directory
        let workdir = workdir(".");
        let (wd_id, wd_root) = match (workdir.id(), workdir.root()) {
            (Some(id), Some(root)) => (id, root),
            _ => return vec![],
        };

        // Since we're prioritizing local, we want to make sure we consider
        // the local suggestions for the configuration; this means we will
        // handle suggested configuration even if not applied globally before
        // going over the omnipath.
        let cfg = config(".");
        let suggest_config_value = cfg.suggest_config.config();
        let local_config = if suggest_config_value.is_null() {
            cfg
        } else {
            let mut local_config = config_loader(".").raw_config.clone();
            local_config.extend(
                suggest_config_value.clone(),
                ConfigExtendOptions::new(),
                vec![],
            );
            OmniConfig::from_config_value(&local_config)
        };

        // Get the package and worktree paths for the current repo
        // TODO: make it work from a package path to include existing
        //       paths from the worktree too
        let worktree_path = Some(PathBuf::from(wd_root));
        let package_path = package_path_from_handle(&wd_id);
        let expected_path = PathBuf::from(wd_root);

        // Now we can extract the different values that would be applied to
        // the path that are actually matching the current work directory;
        // note we will consider both path that are matching the current work
        // directory but also convert any path that would match the package
        // path for the same work directory.
        let local_paths = local_config
            .path
            .prepend
            .iter()
            .chain(local_config.path.append.iter())
            .filter_map(|path_entry| {
                if !path_entry.is_valid() {
                    return None;
                }

                let pathbuf = PathBuf::from(&path_entry.full_path);

                if let Some(worktree_path) = &worktree_path {
                    if let Ok(suffix) = pathbuf.strip_prefix(worktree_path) {
                        return Some(expected_path.join(suffix).to_string_lossy().to_string());
                    }
                }

                if let Some(package_path) = &package_path {
                    if let Ok(suffix) = pathbuf.strip_prefix(package_path) {
                        return Some(expected_path.join(suffix).to_string_lossy().to_string());
                    }
                }

                None
            })
            .collect::<Vec<String>>();

        Self::aggregate_commands_from_path(&local_paths)
    }

    fn aggregate_commands_from_path(paths: &Vec<String>) -> Vec<Self> {
        let mut all_commands: Vec<PathCommand> = Vec::new();
        let mut known_sources: HashMap<String, usize> = HashMap::new();

        for path in paths {
            // Aggregate all the files first, since WalkDir does not sort the list
            let mut files_to_process = Vec::new();
            for entry in WalkDir::new(path).follow_links(true).into_iter().flatten() {
                let filetype = entry.file_type();
                let filepath = entry.path();

                if !filetype.is_file() || !is_executable(filepath) {
                    continue;
                }

                files_to_process.push(filepath.to_path_buf());
            }

            // Sort the files by path
            files_to_process.sort();

            // Process the files
            for filepath in files_to_process {
                let mut partitions = filepath
                    .strip_prefix(format!("{}/", path))
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .split('/')
                    .collect::<Vec<&str>>();

                let num_partitions = partitions.len();

                // For each partition that is not the last one, remove
                // the suffix `.d` if it exists
                for partition in &mut partitions[0..num_partitions - 1] {
                    if partition.ends_with(".d") {
                        *partition = &partition[0..partition.len() - 2];
                    }
                }

                // For the last partition, remove any file extension
                if let Some(filename) = partitions.last_mut() {
                    if let Some(dotpos) = filename.rfind('.') {
                        *filename = &filename[0..dotpos];
                    }
                }

                let new_command = PathCommand::new(
                    partitions.iter().map(|s| s.to_string()).collect(),
                    filepath.to_str().unwrap().to_string(),
                );

                // Check if the source is already known
                if let Some(idx) = known_sources.get_mut(&new_command.real_source()) {
                    // Add this command's name to the command's aliases
                    let cmd: &mut _ = &mut all_commands[*idx];
                    cmd.add_alias(new_command.name(), Some(new_command.source()));
                } else {
                    // Add the new command
                    all_commands.push(new_command.clone());
                    known_sources.insert(new_command.real_source(), all_commands.len() - 1);
                }
            }
        }

        all_commands
    }

    pub fn new(name: Vec<String>, source: String) -> Self {
        Self {
            name,
            source,
            aliases: BTreeMap::new(),
            file_details: OnceCell::new(),
        }
    }

    pub fn name(&self) -> Vec<String> {
        self.name.clone()
    }

    pub fn aliases(&self) -> Vec<Vec<String>> {
        self.aliases.keys().cloned().collect()
    }

    fn add_alias(&mut self, alias: Vec<String>, source: Option<String>) {
        if alias == self.name {
            return;
        }

        if self.aliases.keys().any(|a| a == &alias) {
            return;
        }

        self.aliases
            .insert(alias, source.unwrap_or(self.source.clone()));
    }

    pub fn source(&self) -> String {
        self.source.clone()
    }

    fn real_source(&self) -> String {
        if let Ok(canon) = std::fs::canonicalize(&self.source) {
            canon.to_str().unwrap().to_string()
        } else {
            self.source.clone()
        }
    }

    pub fn help(&self) -> Option<String> {
        let details = match self.file_details() {
            Some(details) => details,
            None => return None,
        };

        if details.help.is_some() {
            return details.help.clone();
        }

        if let Some(parsed_help) = &details.parsed_help(&self.source) {
            return parsed_help.desc.clone();
        }

        None
    }

    pub fn syntax(&self) -> Option<CommandSyntax> {
        let details = match self.file_details() {
            Some(details) => details,
            None => return None,
        };

        if details.syntax.is_some() {
            return details.syntax.clone();
        }

        if let Some(parsed_help) = &details.parsed_help(&self.source) {
            return parsed_help.syntax.clone();
        }

        None
    }

    pub fn category(&self) -> Option<Vec<String>> {
        self.file_details()
            .and_then(|details| details.category.clone())
    }

    pub fn exec(&self, argv: Vec<String>, called_as: Option<Vec<String>>) {
        // Get the source of the command as called
        let source = called_as.clone().map_or(self.source.clone(), |called_as| {
            self.aliases
                .get(&called_as)
                .cloned()
                .unwrap_or(self.source.clone())
        });

        // If the help_parser is set and the argv contains `--help` or `-h`
        // we will instead send the command to the help command
        if argv.iter().any(|arg| arg == "--help" || arg == "-h") {
            if let Some(details) = self.file_details() {
                if details.help_parser.is_some() {
                    HelpCommand::new().exec(called_as.unwrap_or(self.name.clone()));
                    unreachable!("Help command should have exited");
                }
            }
        }

        // Execute the command
        let mut command = ProcessCommand::new(source);
        command.args(argv);
        command.exec();

        panic!("Something went wrong");
    }

    pub fn autocompletion(&self) -> bool {
        self.file_details()
            .map(|details| details.autocompletion)
            .unwrap_or(false)
    }

    pub fn autocomplete(&self, comp_cword: usize, argv: Vec<String>) -> Result<(), ()> {
        let mut command = ProcessCommand::new(self.source.clone());
        command.arg("--complete");
        command.args(argv);
        command.env("COMP_CWORD", comp_cword.to_string());

        match command.output() {
            Ok(output) => {
                println!("{}", String::from_utf8_lossy(&output.stdout));
                Ok(())
            }
            Err(_) => Err(()),
        }
    }

    fn file_details(&self) -> Option<&PathCommandFileDetails> {
        self.file_details
            .get_or_init(|| PathCommandFileDetails::from_file(&self.source))
            .as_ref()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathCommandFileDetails {
    #[serde(default, deserialize_with = "deserialize_category")]
    category: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_help")]
    help: Option<String>,
    #[serde(default, deserialize_with = "deserialize_autocompletion")]
    autocompletion: bool,
    #[serde(default, deserialize_with = "deserialize_syntax")]
    syntax: Option<CommandSyntax>,
    #[serde(default)]
    help_parser: Option<PathCommandHelpParser>,
    #[serde(skip)]
    parsed_help: OnceCell<Option<ParsedCommandHelp>>,
}

fn deserialize_category<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    match value {
        serde_yaml::Value::String(s) => Ok(Some(
            s.split(',')
                .map(|s| s.trim().to_string())
                .collect::<Vec<String>>(),
        )),
        serde_yaml::Value::Sequence(s) => Ok(Some(
            s.iter()
                .map(|s| s.as_str().unwrap().trim().to_string())
                .collect::<Vec<String>>(),
        )),
        _ => Ok(None),
    }
}

fn deserialize_help<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    match value {
        serde_yaml::Value::String(s) => Ok(Some(s)),
        _ => Ok(None),
    }
}

fn deserialize_autocompletion<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    match value {
        serde_yaml::Value::Bool(b) => Ok(b),
        _ => Ok(false),
    }
}

fn deserialize_syntax<'de, D>(deserializer: D) -> Result<Option<CommandSyntax>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    if let Ok(value) = CommandSyntax::deserialize(deserializer) {
        return Ok(Some(value));
    }
    Ok(None)
}

impl PathCommandFileDetails {
    pub fn from_file(path: &str) -> Option<Self> {
        if let Some(details) = Self::from_metadata_file(path) {
            return Some(details);
        }

        if let Some(details) = Self::from_source_file(path) {
            return Some(details);
        }

        None
    }

    pub fn from_metadata_file(path: &str) -> Option<Self> {
        // The metadata file for `file.ext` can be either
        // `file.ext.metadata.yaml` or `file.metadata.yaml`
        let mut metadata_files = vec![format!("{}.metadata.yaml", path)];
        if let Some(dotpos) = path.rfind('.') {
            metadata_files.push(format!("{}.metadata.yaml", &path[0..dotpos]));
        }

        for metadata_file in metadata_files {
            let path = Path::new(&metadata_file);

            // Check if the metadata file exists
            if !path.exists() {
                continue;
            }

            if let Ok(file) = File::open(path) {
                if let Ok(mut md) = serde_yaml::from_reader::<_, Self>(file) {
                    // If the help is not empty, split it into lines
                    if let Some(help) = &mut md.help {
                        *help = handle_color_codes(help.clone());
                    }

                    return Some(md);
                }
            }
        }

        None
    }

    pub fn from_source_file(path: &str) -> Option<Self> {
        let mut autocompletion = false;
        let mut category = None;
        let mut help_parser = None;
        let mut help_lines = Vec::new();
        let parsed_help = OnceCell::new();

        let mut parameters: Vec<SyntaxOptArg> = vec![];

        let mut reading_help = false;

        let file = File::open(path);
        if file.is_err() {
            return None;
        }
        let file = file.unwrap();

        let reader = BufReader::new(file);
        for line in reader.lines() {
            if line.is_err() {
                // If the file is not readable, skip trying to read the headers
                return None;
            }
            let line = line.unwrap();

            // Early exit condition to stop reading when we don't need to anymore
            if !line.starts_with('#') || (reading_help && !line.starts_with("# help:")) {
                break;
            }

            if line.starts_with("# category:") {
                let cat: Vec<String> = line
                    .strip_prefix("# category:")
                    .unwrap()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect();
                category = Some(cat);
            } else if line.starts_with("# autocompletion:") {
                let completion = line
                    .strip_prefix("# autocompletion:")
                    .unwrap()
                    .trim()
                    .to_lowercase();
                autocompletion = completion == "true";
            } else if line.starts_with("# help_parser:") {
                let help_parser_str = line
                    .strip_prefix("# help_parser:")
                    .unwrap()
                    .trim()
                    .to_lowercase();
                help_parser = PathCommandHelpParser::from_str(&help_parser_str);
            } else if line.starts_with("# help:") {
                reading_help = true;
                let help_line =
                    handle_color_codes(line.strip_prefix("# help:").unwrap().trim().to_string());
                help_lines.push(help_line);
            } else if line.starts_with("# arg:") || line.starts_with("# opt:") {
                let param_required = line.starts_with("# arg:");
                let param = line
                    .strip_prefix("# arg:")
                    .or_else(|| line.strip_prefix("# opt:"))
                    .unwrap()
                    .splitn(2, ':')
                    .map(|s| s.trim().to_string())
                    .collect::<Vec<String>>();
                if param.len() != 2 {
                    continue;
                }

                let param_name = param[0].clone();
                let param_desc = param[1].clone();

                if let Some(cur_param_desc) = parameters
                    .iter_mut()
                    .find(|p| p.name == param_name && p.required == param_required)
                {
                    cur_param_desc.desc = Some(format!(
                        "{}\n{}",
                        cur_param_desc.desc.clone().unwrap_or(String::new()),
                        param_desc
                    ));
                } else {
                    parameters.push(SyntaxOptArg::new(
                        param_name,
                        Some(param_desc),
                        param_required,
                    ))
                }
            }
        }

        let mut syntax = match parameters.len() {
            0 => None,
            _ => Some(CommandSyntax::new()),
        };

        if !parameters.is_empty() {
            for parameter in &mut parameters {
                if let Some(desc) = &parameter.desc {
                    parameter.desc = Some(handle_color_codes(desc.clone()));
                }
            }
            syntax.as_mut().unwrap().parameters = parameters;
        }

        if help_parser.is_some() {
            // Check if the help parser is enabled in the configuration
            let enable_parser = if global_config().path_commands.help_parser {
                // If the directory is not trusted, init the parsed help to None
                // so that we don't run an untrusted command to show the help
                match std::fs::canonicalize(path) {
                    Ok(path) => is_trusted(&path.to_string_lossy().to_string()),
                    Err(_) => false,
                }
            } else {
                false
            };

            if !enable_parser {
                if parsed_help.set(None).is_err() {
                    unreachable!("Parsed help should not be set");
                }
            }
        }

        // Return the file details
        Some(PathCommandFileDetails {
            category,
            help: Some(help_lines.join("\n")),
            autocompletion,
            syntax,
            help_parser,
            parsed_help,
        })
    }

    fn parsed_help(&self, source: &str) -> Option<&ParsedCommandHelp> {
        self.parsed_help
            .get_or_init(|| match &self.help_parser {
                Some(parser) => parser.call_and_parse(source),
                None => None,
            })
            .as_ref()
    }
}

fn handle_color_codes(string: String) -> String {
    string
        .replace("\\033[", "\x1B[")
        .replace("\\e[", "\x1B[")
        .replace("\\x1B[", "\x1B[")
}
