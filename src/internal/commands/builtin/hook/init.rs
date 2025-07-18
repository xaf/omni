use std::collections::BTreeMap;
use std::process::exit;

use serde::Serialize;
use shell_escape::escape;
use tera::Context;
use tera::Tera;

use crate::internal::commands::base::BuiltinCommand;
use crate::internal::commands::Command;
use crate::internal::config::global_config;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::CommandSyntax;
use crate::internal::config::SyntaxOptArg;
use crate::internal::config::SyntaxOptArgNumValues;
use crate::internal::config::SyntaxOptArgType;
use crate::internal::env::current_exe;
use crate::internal::env::data_home;
use crate::internal::env::shims_dir;
use crate::internal::env::Shell;
use crate::internal::user_interface::StringColor;
use crate::omni_error;

#[derive(Debug, Clone)]
struct HookInitCommandArgs {
    shell: String,
    aliases: Vec<String>,
    command_aliases: Vec<InitHookAlias>,
    shims: bool,
    keep_shims: bool,
    print_shims_path: bool,
}

impl From<BTreeMap<String, ParseArgsValue>> for HookInitCommandArgs {
    fn from(args: BTreeMap<String, ParseArgsValue>) -> Self {
        let shell = match args.get("shell") {
            Some(ParseArgsValue::SingleString(Some(shell))) => shell.to_string(),
            _ => Shell::from_env().to_string(),
        };

        // Load aliases from the configuration first
        let config = global_config();
        let mut aliases: Vec<String> = vec![];
        let mut command_aliases: Vec<InitHookAlias> = vec![];
        for alias in config.shell_aliases.aliases.iter() {
            match alias.target.as_ref() {
                Some(target) => {
                    command_aliases.push(InitHookAlias::new(alias.alias.clone(), target.clone()));
                }
                None => aliases.push(alias.alias.clone()),
            }
        }

        // Add the aliases from the command line
        if let Some(ParseArgsValue::ManyString(cli_aliases)) = args.get("alias") {
            let cli_aliases: Vec<_> = cli_aliases
                .iter()
                .flat_map(|alias| alias.clone())
                .filter(|alias| !alias.is_empty())
                .collect();
            aliases.extend(cli_aliases);
        }

        // Add the command aliases from the command line
        if let Some(ParseArgsValue::GroupedString(cli_command_aliases)) = args.get("command_alias")
        {
            let cli_command_aliases = cli_command_aliases.iter().filter_map(|grouped| {
                if grouped.len() == 2 {
                    let source = grouped.first()?.clone()?.trim().to_string();
                    let target = grouped.get(1)?.clone()?.trim().to_string();
                    if source.is_empty() || target.is_empty() {
                        None
                    } else {
                        Some(InitHookAlias::new(source, target))
                    }
                } else {
                    None
                }
            });
            command_aliases.extend(cli_command_aliases);
        }

        let shims = matches!(
            args.get("shims"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let keep_shims = matches!(
            args.get("keep_shims_in_path"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );
        let print_shims_path = matches!(
            args.get("print_shims_path"),
            Some(ParseArgsValue::SingleBoolean(Some(true)))
        );

        Self {
            shell,
            aliases,
            command_aliases,
            shims,
            keep_shims,
            print_shims_path,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct InitHookAlias {
    alias: String,
    command: String,
    command_size: usize,
    full_command: String,
}

impl InitHookAlias {
    fn new(alias: String, command: String) -> Self {
        // Use shell split for command
        let command_vec = shell_words::split(&command)
            .unwrap_or_else(|err| {
                omni_error!(
                    format!("failed to parse alias command '{}': {}", command, err),
                    "hook init"
                );
                exit(1);
            })
            .into_iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        let full_command = format!("omni {command}");

        Self {
            alias,
            command: shell_words::quote(&command).to_string(),
            command_size: command_vec.len(),
            full_command: shell_words::quote(&full_command).to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HookInitCommand {}

impl HookInitCommand {
    pub fn new() -> Self {
        Self {}
    }
}

impl BuiltinCommand for HookInitCommand {
    fn new_boxed() -> Box<dyn BuiltinCommand> {
        Box::new(Self::new())
    }

    fn clone_boxed(&self) -> Box<dyn BuiltinCommand> {
        Box::new(self.clone())
    }

    fn name(&self) -> Vec<String> {
        vec!["hook".to_string(), "init".to_string()]
    }

    fn aliases(&self) -> Vec<Vec<String>> {
        vec![]
    }

    fn help(&self) -> Option<String> {
        Some(
            concat!(
            "Hook used to initialize the shell\n",
            "\n",
            "The \x1B[1m\x1B[4minit\x1B[0m hook will provide you with the command to run to ",
            "initialize omni in your shell. You can specify which shell you wish to load it ",
            "for by specifying either one of \x1B[1mzsh\x1B[0m, \x1B[1mbash\x1B[0m, or ",
            "\x1B[1mfish\x1B[0m as optional parameter. If no argument is specified, the login ",
            "shell, as provided by the \x1B[3mSHELL\x1B[0m environment variable, will be used. ",
            "You can load omni in your shell by using \x1B[1meval \"$(omni hook init YOURSHELL)",
            "\"\x1B[0m for bash or zsh, or \x1B[1momni hook init fish | source\x1B[0m for fish.\n",
            "\n",
            "The \x1B[1minit\x1B[0m hook supports the \x1B[1m--alias <alias>\x1B[0m ",
            "option, which adds an alias to the omni command with autocompletion support. It ",
            "also supports the \x1B[1m--command-alias <alias> <subcommand>\x1B[0m option, which ",
            "adds an alias to the specified omni subcommand with autocompletion support.",
        )
            .to_string(),
        )
    }

    fn syntax(&self) -> Option<CommandSyntax> {
        Some(CommandSyntax {
            parameters: vec![
                SyntaxOptArg {
                    names: vec!["--alias".to_string()],
                    desc: Some(
                        "Create an alias for the omni command with autocompletion support."
                            .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--command-alias".to_string()],
                    desc: Some(
                        concat!(
                            "Create an alias for the specified omni subcommand with autocompletion ",
                            "support. The second argument can be any omni subcommand, including ",
                            "custom subcommands.",
                        )
                        .to_string(),
                    ),
                    placeholders: vec!["ALIAS".to_string(), "SUBCOMMAND".to_string()],
                    num_values: Some(SyntaxOptArgNumValues::Exactly(2)),
                    arg_type: SyntaxOptArgType::Array(Box::new(SyntaxOptArgType::String)),
                    group_occurrences: true,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--shims".to_string()],
                    desc: Some(
                        "Only load the shims without setting up the dynamic environment."
                            .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--keep-shims-in-path".to_string()],
                    desc: Some(concat!(
                        "Prevent the dynamic environment from removing the shims directory from the PATH. ",
                        "This can be useful if you are used to launch your IDE from the terminal and do ",
                        "not have other means to load the shims in its environment."
                    ).to_string()),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["--print-shims-path".to_string()],
                    desc: Some(concat!(
                        "Print the path to the shims directory and exit. This should not be ",
                        "used to eval in a shell environment."
                    ).to_string()),
                    arg_type: SyntaxOptArgType::Flag,
                    ..Default::default()
                },
                SyntaxOptArg {
                    names: vec!["shell".to_string()],
                    desc: Some(
                        "Which shell to initialize omni for."
                            .to_string(),
                    ),
                    arg_type: SyntaxOptArgType::Enum(vec![
                        "bash".to_string(),
                        "zsh".to_string(),
                        "fish".to_string(),
                    ]),
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
        let args = HookInitCommandArgs::from(
            command
                .exec_parse_args_typed(argv, self.name())
                .expect("should have args to parse"),
        );

        if args.print_shims_path {
            println!("{}", shims_dir().display());
            exit(0);
        }

        match args.shell.as_str() {
            "bash" => dump_integration(
                args,
                include_bytes!("../../../../../templates/shell_integration.bash.tmpl"),
            ),
            "zsh" => dump_integration(
                args,
                include_bytes!("../../../../../templates/shell_integration.zsh.tmpl"),
            ),
            "fish" => dump_integration(
                args,
                include_bytes!("../../../../../templates/shell_integration.fish.tmpl"),
            ),
            _ => {
                omni_error!(
                    format!(
                        "invalid shell '{}', omni only supports bash, zsh and fish",
                        args.shell
                    ),
                    "hook init"
                );
                exit(1);
            }
        }
        exit(0);
    }
}

fn dump_integration(args: HookInitCommandArgs, integration: &[u8]) {
    let integration = String::from_utf8_lossy(integration).to_string();

    let mut context = Context::new();
    context.insert("OMNI_BIN", &escape(current_exe().to_string_lossy()));
    context.insert("OMNI_DATA_HOME", &escape(data_home().into()));
    context.insert("OMNI_SHIMS", &escape(shims_dir().to_string_lossy()));
    context.insert("OMNI_ALIASES", &args.aliases);
    context.insert("OMNI_COMMAND_ALIASES", &args.command_aliases);
    context.insert("SHIMS_ONLY", &args.shims);
    context.insert("KEEP_SHIMS", &args.keep_shims);

    let result = Tera::one_off(&integration, &context, false)
        .expect("failed to render integration template");

    println!("{result}");
}
