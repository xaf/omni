use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::path::PathBuf;
use std::process::exit;

use itertools::Itertools;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::builtin::TidyGitRepo;
use crate::internal::commands::utils::abs_path;
use crate::internal::commands::utils::file_auto_complete;
use crate::internal::commands::Command;
use crate::internal::config::global_config;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::CommandSyntax;
use crate::internal::config::ConfigExtendOptions;
use crate::internal::config::ConfigExtendStrategy;
use crate::internal::config::ConfigLoader;
use crate::internal::config::ConfigValue;
use crate::internal::config::OrgConfig;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::env::shell_integration_is_loaded;
use crate::internal::env::user_home;
use crate::internal::env::Shell;
use crate::internal::git::format_path_with_template;
use crate::internal::git::full_git_url_parse;
use crate::internal::git::Org;
use crate::internal::user_interface::StringColor;
use crate::omni_error;
use crate::omni_info;
use crate::omni_warning;

#[derive(Debug, Clone)]
pub struct ConfigBootstrapCommand {}

impl ConfigBootstrapCommand {
    pub fn new() -> Self {
        Self {}
    }
}

impl BuiltinCommand for ConfigBootstrapCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["config".to_string(), "bootstrap".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
        Some(
            concat!(
                "Bootstraps the configuration of omni\n",
                "\n",
                "This will walk you through setting up the initial configuration to ",
                "use omni, such as setting up the worktree, format to use when cloning ",
                "repositories, and setting up initial organizations.\n",
            )
            .to_string(),
        )
    }

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            parameters: vec![
                SyntaxOptArg {
                    names: vec!["--worktree".to_string()],
                    desc: Some("Bootstrap the main worktree location".to_string()),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--repo-path-format".to_string()],
                    desc: Some("Bootstrap the repository path format".to_string()),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--organizations".to_string()],
                    desc: Some("Bootstrap the organizations".to_string()),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--shell".to_string()],
                    desc: Some("Bootstrap the shell integration".to_string()),
                    arg_type: SyntaxOptArgType::Flag,
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
        let options = ConfigBootstrapOptions::from_parsed_args(
            command
                .exec_parse_args_typed(argv, self.name())
                .expect("should have args to parse"),
        );

        match config_bootstrap(Some(options)) {
            Ok(true) => {
                omni_info!("configuration updated");
            }
            Ok(false) => {}
            Err(err) => {
                omni_error!(format!("{}", err));
                exit(1);
            }
        }

        exit(0);
    }
}

#[derive(Debug, Clone)]
pub struct ConfigBootstrapOptions {
    default: bool,
    worktree: bool,
    repo_path_format: bool,
    organizations: bool,
    shell: bool,
}

impl Default for ConfigBootstrapOptions {
    fn default() -> Self {
        Self {
            default: true,
            worktree: true,
            repo_path_format: true,
            organizations: true,
            shell: true,
        }
    }
}

impl ConfigBootstrapOptions {
    fn from_parsed_args(args: BTreeMap<String, ParseArgsValue>) -> Self {
        let worktree = matches!(
            args.get("worktree"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let repo_path_format = matches!(
            args.get("repo_path_format"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let organizations = matches!(
            args.get("organizations"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let shell = matches!(
            args.get("shell"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );

        // If none of the options are specified, default to all
        if !worktree && !repo_path_format && !organizations && !shell {
            return Self::default();
        }

        Self {
            default: false,
            worktree,
            repo_path_format,
            organizations,
            shell,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConfigBootstrap {
    #[serde(skip_serializing_if = "String::is_empty")]
    worktree: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    repo_path_format: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    org: Vec<OrgConfig>,
}

pub fn config_bootstrap(options: Option<ConfigBootstrapOptions>) -> Result<bool, String> {
    let options = options.unwrap_or_default();

    if options.worktree || options.repo_path_format || options.organizations {
        let worktree = if options.worktree {
            let (worktree, continue_bootstrap) = question_worktree();
            if !continue_bootstrap {
                return Ok(false);
            }
            worktree
        } else {
            "".to_string()
        };

        let repo_path_format = if options.repo_path_format {
            let (repo_path_format, continue_bootstrap) =
                question_repo_path_format(worktree.clone());
            if !continue_bootstrap {
                return Ok(false);
            }
            repo_path_format
        } else {
            "".to_string()
        };

        let orgs = if options.organizations {
            let (orgs, continue_bootstrap) = question_org(&worktree);
            if !continue_bootstrap {
                return Ok(false);
            }
            orgs
        } else {
            vec![]
        };

        let config = ConfigBootstrap {
            worktree,
            repo_path_format,
            org: orgs,
        };

        if let Err(err) = ConfigLoader::edit_main_user_config_file(|config_value| {
            // Dump our config object as yaml
            let yaml = serde_yaml::to_string(&config);

            // Now get a ConfigValue object from the yaml
            let new_config_value = match yaml {
                Ok(yaml) => match ConfigValue::from_str(&yaml) {
                    Ok(config_value) => config_value,
                    Err(err) => {
                        omni_error!(format!("failed to parse configuration: {}", err));
                        return false;
                    }
                },
                Err(err) => {
                    omni_error!(format!("failed to serialize configuration: {}", err));
                    return false;
                }
            };

            // Apply it over the existing configuration
            config_value.extend(
                new_config_value,
                ConfigExtendOptions::new()
                    .with_strategy(ConfigExtendStrategy::Replace)
                    .with_transform(false),
                vec![],
            );

            // And return true to save the configuration
            true
        }) {
            return Err(format!("Failed to update user configuration: {err}"));
        }
    }

    if options.shell {
        if shell_integration_is_loaded() {
            if options.default {
                // If the shell integration is already setup, no need to do anything else
                return Ok(true);
            } else {
                omni_info!("shell integration detected in this shell");
                omni_info!(format!(
                    "still proceeding as requested through {}",
                    "--shell".light_cyan()
                ));
            }
        }

        // We reach here only if we're missing the shell integration
        let current_shell = Shell::current();
        match current_shell {
            Shell::Unknown(_) | Shell::Posix => {
                omni_warning!(format!(
                    "omni does not provide a shell integration for your shell ({})",
                    current_shell.to_str().light_cyan(),
                ));
                omni_warning!("you can still use omni, but dynamic environment and easy");
                omni_warning!("navigation will not be available");
                return Ok(true);
            }
            _ => {}
        }

        let (rc_file, continue_bootstrap) = question_rc_file(&current_shell);
        if !continue_bootstrap {
            return Ok(false);
        }

        match std::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(rc_file.clone())
        {
            Ok(mut file) => {
                let hook = current_shell.hook_init_command();

                // Check if the hook is already in the file
                let mut line_number = 0;
                let reader = BufReader::new(&file);
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            line_number += 1;
                            if line.trim() == hook {
                                omni_info!(format!(
                                    "omni hook already present in {}",
                                    rc_file.to_string_lossy().light_blue(),
                                ));
                                return Ok(true);
                            }
                        }
                        Err(err) => {
                            return Err(format!(
                                "Failed to read from {}: {}",
                                rc_file.to_string_lossy(),
                                err
                            ));
                        }
                    }
                }

                // Check if we need to add an extra new line
                let ends_with_newline = if line_number > 0 {
                    let mut buf = [0; 1];
                    file.seek(std::io::SeekFrom::End(-1)).unwrap();
                    file.read_exact(&mut buf).unwrap();
                    buf[0] == b'\n'
                } else {
                    false
                };

                // If we get here, we have to write the hook at the end of the file
                let mut content = String::new();
                if line_number > 0 {
                    content.push('\n');
                    if !ends_with_newline {
                        content.push('\n');
                    }
                }
                content.push_str("# omni shell integration\n");
                content.push_str(&hook);
                content.push('\n');

                if let Err(err) = file.write_all(content.as_bytes()) {
                    return Err(format!(
                        "Failed to write to {}: {}",
                        rc_file.to_string_lossy(),
                        err
                    ));
                }

                omni_info!(format!(
                    "omni hook added to {}; remember to reload your shell",
                    rc_file.to_string_lossy().light_blue(),
                ));
            }
            Err(err) => {
                return Err(format!(
                    "Failed to open {}: {}",
                    rc_file.to_string_lossy(),
                    err
                ));
            }
        }
    }

    Ok(true)
}

fn question_worktree() -> (String, bool) {
    let global_config = global_config();

    let default_worktree = PathBuf::from(global_config.worktree.clone());
    let default_worktree = if let Ok(suffix) = default_worktree.strip_prefix(user_home()) {
        PathBuf::from("~").join(suffix)
    } else {
        default_worktree
    }
    .to_string_lossy()
    .to_string();

    let question = requestty::Question::input("config_worktree")
        .ask_if_answered(true)
        .on_esc(requestty::OnEsc::Terminate)
        .message("What is the directory where you usually put your projects?")
        .auto_complete(|p, _| file_auto_complete(p))
        .default(default_worktree)
        .validate(|path, _| {
            if path.is_empty() {
                return Err("You need to provide a value for the worktree".to_string());
            }

            let path_obj = PathBuf::from(path);
            let canonicalized = abs_path(path_obj);
            if canonicalized.exists() && !canonicalized.is_dir() {
                return Err("The worktree must be a directory".to_string());
            }
            Ok(())
        })
        .build();

    let worktree = match requestty::prompt_one(question) {
        Ok(answer) => match answer {
            requestty::Answer::String(path) => {
                let path_obj = PathBuf::from(path.clone());
                let canonicalized = abs_path(path_obj);
                if !canonicalized.is_dir() {
                    omni_warning!(
                        format!(
                            "directory {} does not exist, but will be created upon cloning",
                            path.clone().light_cyan(),
                        ),
                        ""
                    );
                }
                path
            }
            _ => unreachable!(),
        },
        Err(err) => {
            println!("{}\x1B[0K", format!("[✘] {err:?}").red());
            return ("".to_string(), false);
        }
    };

    (worktree, true)
}

fn question_repo_path_format(worktree: String) -> (String, bool) {
    let global_config = global_config();
    let current_repo_path_format = global_config.repo_path_format.clone();

    let mut choices = vec![
        (
            "%{host}/%{org}/%{repo}",
            true,
            "github.com/xaf/omni".to_string(),
        ),
        ("%{org}/%{repo}", true, "xaf/omni".to_string()),
        ("%{repo}", true, "omni".to_string()),
    ];

    let mut default = 0;
    if !current_repo_path_format.is_empty() {
        let mut found = false;

        for (index, (format, _joinpath, _example)) in choices.iter_mut().enumerate() {
            if current_repo_path_format == *format {
                default = index;
                found = true;
                break;
            }
        }

        if !found {
            let git_url = full_git_url_parse("https://github.com/xaf/omni").unwrap();
            let example = format_path_with_template(&worktree, &git_url, &current_repo_path_format);
            let example_str = example.to_string_lossy().to_string();

            choices.insert(
                0,
                (
                    &current_repo_path_format,
                    false,
                    format!("e.g. {example_str}"),
                ),
            );
        }
    }

    let custom = choices.len();
    choices.push((
        "custom",
        false,
        "use the variables to write your own format".to_string(),
    ));

    let qchoices: Vec<_> = choices
        .iter()
        .map(|(format, joinpath, example)| {
            let example = if *joinpath {
                let path = PathBuf::from(&worktree).join(example);
                format!("e.g. {}", path.to_string_lossy())
            } else {
                example.to_string()
            };
            format!("{} {}", format, format!("({example})").light_black())
        })
        .collect();

    let question = requestty::Question::select("config_repo_path_format")
        .ask_if_answered(true)
        .on_esc(requestty::OnEsc::Terminate)
        .message("How do you structure your projects inside your worktree?")
        .choices(qchoices)
        .default(default)
        .transform(|selected, _, backend| {
            // Let's stop at the first parenthesis we encounter
            let selected = selected.text.split('(').next().unwrap_or(&selected.text);
            write!(backend, "{}", selected.cyan())
        })
        .build();

    let repo_path_format = match requestty::prompt_one(question) {
        Ok(answer) => match answer {
            requestty::Answer::ListItem(item) => match item.index {
                idx if idx == custom => {
                    let question = requestty::Question::input("config_repo_path_format_custom")
                        .ask_if_answered(true)
                        .on_esc(requestty::OnEsc::Terminate)
                        .message("Which custom format do you wish to use?")
                        .default("%{host}/%{org}/%{repo}")
                        .validate(|format, _| {
                            if format.is_empty() {
                                return Err("You need to provide a format".light_red());
                            }

                            // Check that at least %{repo} exists
                            if !format.contains("%{repo}") {
                                return Err("The format must contain %{repo}"
                                    .to_string()
                                    .light_red());
                            }

                            // Check if any %{..} variable that is not repo, org or host
                            // exists, as other variables are not supported
                            let regex = Regex::new(r"%\{([^}]+)\}").unwrap();
                            for capture in regex.captures_iter(format) {
                                let var = capture.get(1).unwrap().as_str();
                                if var != "repo" && var != "org" && var != "host" {
                                    return Err(format!(
                                        "The format contains an unknown variable: %{{{var}}}"
                                    )
                                    .to_string()
                                    .light_red());
                                }
                            }

                            Ok(())
                        })
                        .build();

                    match requestty::prompt_one(question) {
                        Ok(answer) => match answer {
                            requestty::Answer::String(format) => format,
                            _ => unreachable!(),
                        },
                        Err(err) => {
                            println!("{}\x1B[0K", format!("[✘] {err:?}").red());
                            return ("".to_string(), false);
                        }
                    }
                }
                _ => choices[item.index].0.to_string(),
            },
            _ => unreachable!(),
        },
        Err(err) => {
            println!("{}\x1B[0K", format!("[✘] {err:?}").red());
            return ("".to_string(), false);
        }
    };

    (repo_path_format, true)
}

fn question_org(worktree: &str) -> (Vec<OrgConfig>, bool) {
    // Now that we have a worktree, we can list the repositories in there
    // and identify the organizations that the user has, so we can offer
    // them to be setup as trusted (or not) organizations.

    let mut worktrees = HashSet::new();
    worktrees.insert(PathBuf::from(worktree));

    let repositories = TidyGitRepo::list_repositories(worktrees);

    let mut orgs_map = HashMap::new();
    let mut hosts = HashSet::new();
    for repository in repositories {
        let origin_url = repository.origin_url;
        if let Ok(git_url) = full_git_url_parse(&origin_url) {
            let mut org = git_url.clone();

            // First we get the entry that's considering the host and
            // the org, but not the repo
            if org.git_suffix {
                org.path = org
                    .path
                    .strip_suffix(".git")
                    .unwrap_or(org.path.as_ref())
                    .to_string();
            }
            org.git_suffix = false;

            org.path = org
                .path
                .strip_suffix(format!("/{}", org.name).as_str())
                .unwrap_or(org.path.as_ref())
                .to_string();
            org.name = "".to_string();

            let org_str = org.to_string();
            let org_count = orgs_map.entry(org_str).or_insert(0);
            *org_count += 1;

            // Then we get the entry that's considering the host only
            org.path = "".to_string();
            let host_str = org.to_string();
            hosts.insert(host_str.clone());
            let host_count = orgs_map.entry(host_str.clone()).or_insert(0);
            *host_count += 1;

            // And now we strip the user and protocol if any, and add another host entry
            org.user = None;
            org.scheme_prefix = false;
            let stripped_host_str = org.to_string();
            if stripped_host_str != host_str {
                hosts.insert(stripped_host_str.clone());
                let stripped_host_count = orgs_map.entry(stripped_host_str).or_insert(0);
                *stripped_host_count += 1;
            }
        }
    }

    // Sort the map by value
    let mut orgs: Vec<_> = orgs_map
        .clone()
        .into_iter()
        .map(|(handle, count)| (if hosts.contains(&handle) { 1 } else { 2 }, count, handle))
        .sorted()
        .rev()
        .map(|(_, count, handle)| (count, handle))
        .collect();

    // If there are any organizations, already in the configuration,
    // prepend them to the list of organizations above
    let global_config = global_config();
    let current_orgs = global_config.org.clone();
    let mut selected_orgs = HashSet::new();
    for org in current_orgs.iter().rev() {
        if let Ok(org) = Org::new(org.clone()) {
            let count = *orgs_map.get(&org.config.handle).unwrap_or(&0);
            orgs.retain(|x| x.1 != org.config.handle);
            orgs.insert(0, (count, org.config.handle.clone()));
            selected_orgs.insert(org.config.handle.clone());

            if org.owner.is_none() {
                hosts.insert(org.config.handle.clone());
            }
        }
    }

    // If there are no organizations, we can just return early
    if orgs.is_empty() {
        return (vec![], true);
    }

    // Prepare the choices
    let orgs_choices: Vec<_> = orgs
        .iter()
        .map(|(count, org)| {
            (
                format!(
                    "{} {}",
                    org,
                    format!(
                        "({} repositor{})",
                        count,
                        if *count == 1 { "y" } else { "ies" },
                    )
                    .light_black(),
                ),
                selected_orgs.contains(org),
            )
        })
        .collect();

    // Now prepare a multi-select to offer the organizations to be added for easy
    // cloning and navigation
    let question = requestty::Question::multi_select("config_org")
        .ask_if_answered(true)
        .on_esc(requestty::OnEsc::Terminate)
        .message("Which organizations should be added to your configuration?")
        .choices_with_default(orgs_choices)
        .transform(|selected, _, backend| {
            write!(
                backend,
                "{} organization{}",
                selected.len(),
                if selected.len() == 1 { "" } else { "s" }
            )
        })
        .should_loop(false)
        .page_size(7)
        .build();

    let selected_orgs: Vec<String> = match requestty::prompt_one(question) {
        Ok(answer) => match answer {
            requestty::Answer::ListItems(items) => items
                .iter()
                .map(|item| orgs[item.index].1.clone())
                .collect(),
            _ => unreachable!(),
        },
        Err(err) => {
            println!("{}\x1B[0K", format!("[✘] {err:?}").red());
            return (vec![], false);
        }
    };

    // If there are no selected organizations, we can just return early
    if selected_orgs.is_empty() {
        return (vec![], true);
    }

    // Now do a multi-select to know which organizations should be trusted
    let question = requestty::Question::multi_select("config_org_trusted")
        .ask_if_answered(true)
        .on_esc(requestty::OnEsc::Terminate)
        .message("Which organizations should be trusted?")
        .choices_with_default(
            selected_orgs
                .iter()
                .map(|org| {
                    (
                        format!(
                            "{}{}",
                            org,
                            if hosts.contains(org) {
                                // Unicode warning sign
                                " \u{26A0}\u{FE0F}  (broad trust)".light_black()
                            } else {
                                "".to_string()
                            }
                        ),
                        global_config
                            .org
                            .iter()
                            .any(|x| x.handle == *org && x.trusted),
                    )
                })
                .collect::<Vec<_>>(),
        )
        .transform(|selected, _, backend| {
            write!(
                backend,
                "{} organization{}",
                selected.len(),
                if selected.len() == 1 { "" } else { "s" }
            )
        })
        .should_loop(false)
        .page_size(7)
        .build();

    let trusted_orgs: Vec<String> = match requestty::prompt_one(question) {
        Ok(answer) => match answer {
            requestty::Answer::ListItems(items) => items
                .iter()
                .map(|item| selected_orgs[item.index].clone())
                .collect(),
            _ => unreachable!(),
        },
        Err(err) => {
            println!("{}\x1B[0K", format!("[✘] {err:?}").red());
            return (vec![], false);
        }
    };

    // Let the user order the organizations in the order they want, as the
    // order of the organizations is important when cloning repositories,
    // the first organization that has the repository will be used.
    let ordered_orgs = if selected_orgs.len() > 1 {
        let question = requestty::Question::order_select("select_how_to_clone")
            .ask_if_answered(true)
            .on_esc(requestty::OnEsc::Terminate)
            .message("In which order should the organizations be checked for repositories?")
            .choices(selected_orgs.clone())
            .transform(|_selected, _, backend| write!(backend, "\u{2714}\u{FE0F}"))
            .build();

        match requestty::prompt_one(question) {
            Ok(answer) => match answer {
                requestty::Answer::ListItems(items) => items
                    .iter()
                    .map(|item| selected_orgs[item.index].clone())
                    .collect(),
                _ => unreachable!(),
            },
            Err(err) => {
                println!("{}\x1B[0K", format!("[✘] {err:?}").red());
                return (vec![], false);
            }
        }
    } else {
        selected_orgs
    };

    let current_orgs_worktrees: HashMap<String, String> = current_orgs
        .iter()
        .filter(|org| org.worktree.is_some())
        .map(|org| (org.handle.clone(), org.worktree.clone().unwrap()))
        .collect();
    let orgs_config: Vec<OrgConfig> = ordered_orgs
        .iter()
        .map(|org| {
            let trusted = trusted_orgs.contains(org);
            let worktree = current_orgs_worktrees.get(org);
            OrgConfig {
                handle: org.clone(),
                trusted,
                worktree: worktree.cloned(),
                repo_path_format: None,
            }
        })
        .collect();

    (orgs_config, true)
}

fn question_rc_file(current_shell: &Shell) -> (PathBuf, bool) {
    let default_rc_file = current_shell.default_rc_file();
    let default_rc_file = if let Ok(suffix) = default_rc_file.strip_prefix(user_home()) {
        PathBuf::from("~").join(suffix)
    } else {
        default_rc_file
    }
    .to_string_lossy()
    .to_string();

    omni_info!("omni requires a shell integration to provide some of its features");

    let question = requestty::Question::input("integration_rc_file")
        .ask_if_answered(true)
        .on_esc(requestty::OnEsc::Terminate)
        .message(format!(
            "Where is the RC file of your shell ({}) to load the integration?",
            current_shell.to_str(),
        ))
        .auto_complete(|p, _| file_auto_complete(p))
        .default(default_rc_file)
        .validate(|path, _| {
            if path.is_empty() {
                return Err("You need to provide a value for the rc_file"
                    .to_string()
                    .light_red());
            }

            let path_obj = PathBuf::from(path);
            let canonicalized = abs_path(path_obj);

            if canonicalized.exists() {
                // Check if the path is a file
                if !canonicalized.is_file() {
                    return Err("The provided path must be a file".light_red());
                }

                // Check if the file is writeable
                match canonicalized.metadata() {
                    Ok(metadata) => {
                        if metadata.permissions().readonly() {
                            return Err("The file must be writeable".light_red());
                        }
                    }
                    Err(err) => return Err(err.light_red()),
                }

                return Ok(());
            }

            // Make sure the directory in which the file is exists, or
            // create it if it doesn't
            if let Some(parent) = canonicalized.parent() {
                if !parent.exists() {
                    if let Err(err) = std::fs::create_dir_all(parent) {
                        return Err(format!(
                            "Failed to create directory {}: {}",
                            parent.to_string_lossy(),
                            err
                        )
                        .light_red());
                    }
                }
            }

            // Create the file if it doesn't exist
            if !canonicalized.exists() {
                if let Err(err) = std::fs::File::create(&canonicalized) {
                    return Err(format!(
                        "Failed to create file {}: {}",
                        canonicalized.to_string_lossy(),
                        err
                    )
                    .light_red());
                }
            }

            Ok(())
        })
        .build();

    let rc_file = match requestty::prompt_one(question) {
        Ok(answer) => match answer {
            requestty::Answer::String(path) => {
                let path_obj = PathBuf::from(path.clone());

                // No need for extra validation, as we have done it above
                abs_path(path_obj)
            }
            _ => unreachable!(),
        },
        Err(err) => {
            println!("{}\x1B[0K", format!("[✘] {err:?}").red());
            return (PathBuf::new(), false);
        }
    };

    (rc_file, true)
}
