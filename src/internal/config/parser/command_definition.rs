use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::cache::utils as cache_utils;
use crate::internal::commands::utils::abs_path;
use crate::internal::commands::utils::str_to_bool;
use crate::internal::commands::HelpCommand;
use crate::internal::config::parser::ConfigErrorHandler;
use crate::internal::config::parser::ConfigErrorKind;
use crate::internal::config::parser::ParseArgsErrorKind;
use crate::internal::config::parser::ParseArgsValue;
use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigSource;
use crate::internal::config::ConfigValue;
use crate::internal::user_interface::colors::StringColor;
use crate::internal::ORG_LOADER;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommandDefinition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    pub run: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syntax: Option<CommandSyntax>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subcommands: Option<HashMap<String, CommandDefinition>>,
    #[serde(default, skip_serializing_if = "cache_utils::is_false")]
    pub argparser: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "cache_utils::is_false")]
    pub export: bool,
    #[serde(skip)]
    pub source: ConfigSource,
    #[serde(skip)]
    pub scope: ConfigScope,
}

impl CommandDefinition {
    pub(super) fn from_config_value(
        config_value: &ConfigValue,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let desc = config_value.get_as_str_or_none("desc", &error_handler.with_key("desc"));

        let run = config_value
            .get_as_str_or_none("run", &error_handler.with_key("run"))
            .unwrap_or_else(|| {
                error_handler
                    .with_key("run")
                    .error(ConfigErrorKind::MissingKey);
                "true".to_string()
            });

        let aliases = config_value.get_as_str_array("aliases", &error_handler.with_key("aliases"));

        let syntax = match config_value.get("syntax") {
            Some(value) => {
                CommandSyntax::from_config_value(&value, &error_handler.with_key("syntax"))
            }
            None => None,
        };

        let tags = match config_value.get("tags") {
            Some(value) => {
                let mut tags = BTreeMap::new();
                if let Some(table) = value.as_table() {
                    for (key, value) in table {
                        if let Some(value) = value.as_str_forced() {
                            tags.insert(key.to_string(), value.to_string());
                        } else {
                            error_handler
                                .with_key("tags")
                                .with_key(key)
                                .with_expected("string")
                                .with_actual(value)
                                .error(ConfigErrorKind::InvalidValueType);
                        }
                    }
                } else {
                    error_handler
                        .with_key("tags")
                        .with_expected("table")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                }
                tags
            }
            None => BTreeMap::new(),
        };

        let category =
            config_value.get_as_str_array("category", &error_handler.with_key("category"));
        let category = if category.is_empty() {
            None
        } else {
            Some(category)
        };

        let dir = config_value.get_as_str_or_none("dir", &error_handler.with_key("dir"));

        let subcommands = match config_value.get("subcommands") {
            Some(value) => {
                let mut subcommands = HashMap::new();
                let subcommands_error_handler = error_handler.with_key("subcommands");
                if let Some(table) = value.as_table() {
                    for (key, value) in table {
                        subcommands.insert(
                            key.to_string(),
                            CommandDefinition::from_config_value(
                                &value,
                                &subcommands_error_handler.with_key(key),
                            ),
                        );
                    }
                } else {
                    subcommands_error_handler
                        .with_expected("table")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                }
                Some(subcommands)
            }
            None => None,
        };

        let argparser = config_value.get_as_bool_or_default(
            "argparser",
            false, // Disable argparser by default
            &error_handler.with_key("argparser"),
        );

        let export = config_value.get_as_bool_or_default(
            "export",
            false, // Do not export by default
            &error_handler.with_key("export"),
        );

        Self {
            desc,
            run,
            aliases,
            syntax,
            category,
            dir,
            subcommands,
            argparser,
            tags,
            export,
            source: config_value.get_source().clone(),
            scope: config_value.current_scope().clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct CommandSyntax {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<SyntaxOptArg>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<SyntaxGroup>,
}

impl CommandSyntax {
    const RESERVED_NAMES: [&'static str; 2] = ["-h", "--help"];

    pub fn new() -> Self {
        Self::default()
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
        error_handler: &ConfigErrorHandler,
    ) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;
        let config_value = ConfigValue::from_value(ConfigSource::Null, ConfigScope::Null, value);
        if let Some(command_syntax) = CommandSyntax::from_config_value(&config_value, error_handler)
        {
            Ok(command_syntax)
        } else {
            Err(serde::de::Error::custom("invalid command syntax"))
        }
    }

    pub(super) fn from_config_value(
        config_value: &ConfigValue,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
        let mut usage = None;
        let mut parameters = vec![];
        let mut groups = vec![];

        if let Some(array) = config_value.as_array() {
            parameters.extend(array.iter().enumerate().filter_map(|(idx, value)| {
                SyntaxOptArg::from_config_value(value, None, &error_handler.with_index(idx))
            }));
        } else if let Some(table) = config_value.as_table() {
            let keys = [
                ("parameters", None),
                ("arguments", Some(true)),
                ("argument", Some(true)),
                ("options", Some(false)),
                ("option", Some(false)),
                ("optional", Some(false)),
            ];

            for (key, required) in keys {
                if let Some(value) = table.get(key) {
                    let param_error_handler = error_handler.with_key(key);
                    if let Some(value) = value.as_array() {
                        let arguments = value
                            .iter()
                            .enumerate()
                            .filter_map(|(idx, value)| {
                                SyntaxOptArg::from_config_value(
                                    value,
                                    required,
                                    &param_error_handler.with_index(idx),
                                )
                            })
                            .collect::<Vec<SyntaxOptArg>>();
                        parameters.extend(arguments);
                    } else if let Some(arg) =
                        SyntaxOptArg::from_config_value(value, required, &param_error_handler)
                    {
                        parameters.push(arg);
                    } else {
                        param_error_handler
                            .with_expected("array or table")
                            .with_actual(value)
                            .error(ConfigErrorKind::InvalidValueType);
                    }
                }
            }

            if let Some(value) = table.get("groups") {
                groups =
                    SyntaxGroup::from_config_value_multi(value, &error_handler.with_key("groups"));
            }

            if let Some(value) = table.get("usage") {
                if let Some(value) = value.as_str_forced() {
                    usage = Some(value.to_string());
                } else {
                    error_handler
                        .with_key("usage")
                        .with_expected("string")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                }
            }
        } else if let Some(value) = config_value.as_str_forced() {
            usage = Some(value.to_string());
        } else {
            error_handler
                .with_expected("array, table or string")
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);
        }

        if parameters.is_empty() && groups.is_empty() && usage.is_none() {
            return None;
        }

        Some(Self {
            usage,
            parameters,
            groups,
        })
    }

    /// The 'leftovers' parameter is used to capture all the remaining arguments
    /// It corresponds to using 'trailing_var_arg' in clap
    /// The following will lead to panic:
    /// - Using 'leftovers' more than once
    /// - Using 'leftovers' before the last positional argument
    /// - Using 'leftovers' with a non-positional argument
    fn check_parameters_leftovers(&self) -> Result<(), String> {
        // Grab all the leftovers params
        let leftovers_params = self.parameters.iter().filter(|param| param.leftovers);

        // Check if the count is greater than one
        if leftovers_params.clone().count() > 1 {
            return Err(format!(
                "only one argument can use {}; found {}",
                "leftovers".light_yellow(),
                leftovers_params
                    .map(|param| param.name().light_yellow())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        // Check if any is non-positional
        let nonpositional_leftovers = leftovers_params
            .clone()
            .filter(|param| !param.is_positional());
        if nonpositional_leftovers.clone().count() > 0 {
            return Err(format!(
                "only positional arguments can use {}; found {}",
                "leftovers".light_yellow(),
                nonpositional_leftovers
                    .map(|param| param.name().light_yellow())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        // Check if our leftovers argument is before the last positional argument
        let last_positional_idx = self
            .parameters
            .iter()
            .rposition(|param| param.is_positional());
        if let Some(lpidx) = last_positional_idx {
            for (idx, param) in self.parameters.iter().enumerate() {
                if param.leftovers && idx < lpidx {
                    return Err(format!(
                        "only the last positional argument can use {}",
                        "leftovers".light_yellow()
                    ));
                }
            }
        }

        Ok(())
    }

    /// The 'last' parameter is used to capture arguments after using '--' on the command line
    /// It corresponds to setting 'last' to true in clap
    /// The following will lead to panic:
    /// - Flags using 'last'
    /// - non-positional using 'last'
    fn check_parameters_last(&self) -> Result<(), String> {
        // Grab all the last params
        let params = self
            .parameters
            .iter()
            .filter(|param| param.last_arg_double_hyphen);

        // Check if any is a non-positional argument
        let nonpositional_last = params.clone().filter(|param| !param.is_positional());
        if nonpositional_last.clone().count() > 0 {
            return Err(format!(
                "only positional arguments can use {}; found {}",
                "last".light_yellow(),
                nonpositional_last
                    .map(|param| param.name().light_yellow())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        Ok(())
    }

    /// Since when setting a counter we do not expect any value, parameters using
    /// the `counter` type will panic if:
    /// - They are positional
    /// - They have a num_values
    fn check_parameters_counter(&self) -> Result<(), String> {
        // Grab all the counter params
        let params = self
            .parameters
            .iter()
            .filter(|param| matches!(param.arg_type(), SyntaxOptArgType::Counter));

        for param in params {
            if param.is_positional() {
                return Err(format!(
                    "{}: counter argument cannot be positional",
                    param.name().light_yellow()
                ));
            }

            if param.num_values.is_some() {
                return Err(format!(
                    "{}: counter argument cannot have a num_values (counters do not take any values)",
                    param.name().light_yellow()
                ));
            }
        }

        Ok(())
    }

    fn check_parameters_references_iter(
        &self,
        references: impl Iterator<Item = impl ToString>,
        available_references: &HashSet<String>,
        reference_type: &str,
        param_name: &str,
    ) -> Result<(), String> {
        for reference in references {
            let reference = reference.to_string();

            if !available_references.contains(&reference) {
                return Err(format!(
                    "parameter or group {} specified in {} for {} does not exist",
                    reference.light_yellow(),
                    reference_type.light_yellow(),
                    param_name.light_yellow(),
                ));
            }
        }

        Ok(())
    }

    fn check_parameters_references(&self) -> Result<(), String> {
        let available_references = self
            .parameters
            .iter()
            .map(|param| param.dest())
            .chain(self.groups.iter().map(|group| group.dest()))
            .collect::<HashSet<_>>();

        for param in &self.parameters {
            let dest = param.dest();

            self.check_parameters_references_iter(
                param.requires.iter().map(|param| sanitize_str(param)),
                &available_references,
                "requires",
                &dest,
            )?;
            self.check_parameters_references_iter(
                param.conflicts_with.iter().map(|param| sanitize_str(param)),
                &available_references,
                "conflicts_with",
                &dest,
            )?;
            self.check_parameters_references_iter(
                param
                    .required_without
                    .iter()
                    .map(|param| sanitize_str(param)),
                &available_references,
                "required_without",
                &dest,
            )?;
            self.check_parameters_references_iter(
                param
                    .required_without_all
                    .iter()
                    .map(|param| sanitize_str(param)),
                &available_references,
                "required_without_all",
                &dest,
            )?;
            self.check_parameters_references_iter(
                param
                    .required_if_eq
                    .keys()
                    .map(|k| sanitize_str(k))
                    .collect::<Vec<_>>()
                    .iter(),
                &available_references,
                "required_if_eq",
                &dest,
            )?;
            self.check_parameters_references_iter(
                param.required_if_eq_all.keys().map(|k| sanitize_str(k)),
                &available_references,
                "required_if_eq_all",
                &dest,
            )?;
        }

        for group in &self.groups {
            let dest = group.dest();

            self.check_parameters_references_iter(
                group.parameters.iter().map(|param| sanitize_str(param)),
                &available_references,
                "parameters",
                &dest,
            )?;

            self.check_parameters_references_iter(
                group.requires.iter().map(|param| sanitize_str(param)),
                &available_references,
                "requires",
                &dest,
            )?;

            self.check_parameters_references_iter(
                group.conflicts_with.iter().map(|param| sanitize_str(param)),
                &available_references,
                "conflicts_with",
                &dest,
            )?;
        }

        Ok(())
    }

    /// The identifiers in the parameters and groups should be unique
    /// across the parameters and groups, or else it will lead to panic
    fn check_parameters_unique_names(&self) -> Result<(), String> {
        let mut dests = HashSet::new();
        let mut names = HashSet::new();

        for param in &self.parameters {
            let dest = param.dest();
            if !dests.insert(dest.clone()) {
                return Err(format!(
                    "identifier {} is defined more than once",
                    dest.light_yellow()
                ));
            }

            for name in param.all_names() {
                // Check if name is -h or --help or any other reserved names
                if Self::RESERVED_NAMES.contains(&name.as_str()) {
                    return Err(format!(
                        "name {} is reserved and cannot be used",
                        name.light_yellow()
                    ));
                }

                if !names.insert(name.clone()) {
                    return Err(format!(
                        "name {} is defined more than once",
                        name.light_yellow()
                    ));
                }
            }
        }

        for group in &self.groups {
            let dest = group.dest();
            if !dests.insert(dest.clone()) {
                return Err(format!(
                    "identifier {} is defined more than once",
                    dest.light_yellow()
                ));
            }
        }

        Ok(())
    }

    /// Allow hyphen values requires that the argument can take a value.
    /// It will thus panic if:
    /// - Set when num_values is set to 0
    /// - Set on a counter
    /// - Set on a flag
    fn check_parameters_allow_hyphen_values(&self) -> Result<(), String> {
        // Grab all the counter params
        let params = self
            .parameters
            .iter()
            .filter(|param| param.allow_hyphen_values);

        for param in params {
            if let Some(SyntaxOptArgNumValues::Exactly(0)) = param.num_values {
                return Err(format!(
                    "{}: cannot use {} with 'num_values=0'",
                    param.name().light_yellow(),
                    "allow_hyphen_values".light_yellow(),
                ));
            }

            match param.arg_type {
                SyntaxOptArgType::Flag | SyntaxOptArgType::Counter => {
                    return Err(format!(
                        "{}: cannot use {} on a {}",
                        param.name().light_yellow(),
                        "allow_hyphen_values".light_yellow(),
                        param.arg_type.to_str(),
                    ))
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Positional parameters have some constraints that could lead the
    /// building of the argument parser to panic:
    /// - If a non-required positional argument appears before a required one
    /// - If a num_values > 1 positional argument appears before a non-required
    ///   one, the latter must have last=true or required=true
    /// - If using num_values=0 or any number of values lower than 1 for a required
    ///   positional argument
    fn check_parameters_positional(&self) -> Result<(), String> {
        let mut prev_positional_with_num_values: Option<String> = None;
        let mut prev_positional_without_required: Option<String> = None;

        for param in self.parameters.iter().filter(|param| param.is_positional()) {
            if !param.required {
                if !param.last_arg_double_hyphen {
                    if let Some(prev) = prev_positional_with_num_values {
                        return Err(format!(
                            "{}: positional need to be required or use '{}' if appearing after {} with num_values > 1",
                            param.name().light_yellow(),
                            "last=true".light_yellow(),
                            prev.light_yellow(),
                        ));
                    }
                }

                if prev_positional_without_required.is_none() {
                    prev_positional_without_required = Some(param.name().clone());
                }
            } else if let Some(prev) = prev_positional_without_required {
                return Err(format!(
                    "{}: required positional argument cannot appear after non-required one {}",
                    param.name().light_yellow(),
                    prev.light_yellow(),
                ));
            } else if let Some(
                SyntaxOptArgNumValues::Exactly(0)
                | SyntaxOptArgNumValues::AtMost(0)
                | SyntaxOptArgNumValues::Between(_, 0),
            ) = param.num_values
            {
                return Err(format!(
                    "{}: positional argument cannot have 'num_values=0'",
                    param.name().light_yellow(),
                ));
            }

            if param.num_values.is_some() && prev_positional_with_num_values.is_none() {
                prev_positional_with_num_values = Some(param.name().clone());
            }
        }

        Ok(())
    }

    /// The flag parameters have some constraints that could lead the
    /// building of the argument parser to panic:
    /// - If a flag has num_values set
    fn check_parameters_flag(&self) -> Result<(), String> {
        for param in self
            .parameters
            .iter()
            .filter(|param| param.arg_type == SyntaxOptArgType::Flag)
        {
            if param.num_values.is_some() {
                return Err(format!(
                    "{}: flag argument cannot have 'num_values' set",
                    param.name().light_yellow(),
                ));
            }
        }

        Ok(())
    }

    fn check_parameters(&self) -> Result<(), String> {
        self.check_parameters_unique_names()?;
        self.check_parameters_references()?;
        self.check_parameters_leftovers()?;
        self.check_parameters_last()?;
        self.check_parameters_counter()?;
        self.check_parameters_allow_hyphen_values()?;
        self.check_parameters_positional()?;
        self.check_parameters_flag()?;

        Ok(())
    }

    pub fn argparser(&self, called_as: Vec<String>) -> Result<clap::Command, String> {
        let mut parser = clap::Command::new(called_as.join(" "))
            .disable_help_subcommand(true)
            .disable_version_flag(true);

        self.check_parameters()?;

        for param in &self.parameters {
            parser = param.add_to_argparser(parser);
        }

        for group in &self.groups {
            parser = group.add_to_argparser(parser);
        }

        Ok(parser)
    }

    pub fn parse_args_typed(
        &self,
        argv: Vec<String>,
        called_as: Vec<String>,
    ) -> Result<BTreeMap<String, ParseArgsValue>, ParseArgsErrorKind> {
        let mut parse_argv = vec!["".to_string()];
        parse_argv.extend(argv);

        let parser = match self.argparser(called_as.clone()) {
            Ok(parser) => parser,
            Err(err) => {
                return Err(ParseArgsErrorKind::ParserBuildError(err));
            }
        };

        let matches = match parser.try_get_matches_from(&parse_argv) {
            Err(err) => match err.kind() {
                clap::error::ErrorKind::DisplayHelp => {
                    HelpCommand::new().exec_with_exit_code(called_as, 0);
                    unreachable!("help command should have exited");
                }
                clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    HelpCommand::new().exec_with_exit_code(called_as, 1);
                    unreachable!("help command should have exited");
                }
                clap::error::ErrorKind::DisplayVersion => {
                    unreachable!("version flag is disabled");
                }
                _ => {
                    return Err(ParseArgsErrorKind::ArgumentParsingError(err));
                }
            },
            Ok(matches) => matches,
        };

        let mut args = BTreeMap::new();

        for param in &self.parameters {
            param.add_to_args(&mut args, &matches, None)?;
        }

        for group in &self.groups {
            group.add_to_args(&mut args, &matches, &self.parameters)?;
        }

        Ok(args)
    }

    pub fn parse_args(
        &self,
        argv: Vec<String>,
        called_as: Vec<String>,
    ) -> Result<BTreeMap<String, String>, ParseArgsErrorKind> {
        let typed_args = self.parse_args_typed(argv, called_as)?;

        let mut args = BTreeMap::new();
        for (key, value) in typed_args {
            value.export_to_env(&key, &mut args);
        }

        let mut all_args = Vec::new();
        for param in &self.parameters {
            all_args.push(param.dest());
        }
        for group in &self.groups {
            all_args.push(group.dest());
        }
        args.insert("OMNI_ARG_LIST".to_string(), all_args.join(" "));

        Ok(args)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SyntaxOptArg {
    #[serde(alias = "name")]
    pub names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub required: bool,
    #[serde(alias = "placeholder", skip_serializing_if = "Vec::is_empty")]
    pub placeholders: Vec<String>,
    #[serde(rename = "type", skip_serializing_if = "SyntaxOptArgType::is_default")]
    pub arg_type: SyntaxOptArgType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_missing_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_values: Option<SyntaxOptArgNumValues>,
    #[serde(rename = "delimiter", skip_serializing_if = "Option::is_none")]
    pub value_delimiter: Option<char>,
    #[serde(rename = "last", skip_serializing_if = "cache_utils::is_false")]
    pub last_arg_double_hyphen: bool,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub leftovers: bool,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub allow_hyphen_values: bool,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub allow_negative_numbers: bool,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub group_occurrences: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts_with: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required_without: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required_without_all: Vec<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub required_if_eq: HashMap<String, String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub required_if_eq_all: HashMap<String, String>,
}

impl Default for SyntaxOptArg {
    fn default() -> Self {
        Self {
            names: vec![],
            dest: None,
            desc: None,
            required: false,
            placeholders: vec![],
            arg_type: SyntaxOptArgType::String,
            default: None,
            default_missing_value: None,
            num_values: None,
            value_delimiter: None,
            last_arg_double_hyphen: false,
            leftovers: false,
            allow_hyphen_values: false,
            allow_negative_numbers: false,
            group_occurrences: false,
            requires: vec![],
            conflicts_with: vec![],
            required_without: vec![],
            required_without_all: vec![],
            required_if_eq: HashMap::new(),
            required_if_eq_all: HashMap::new(),
        }
    }
}

impl SyntaxOptArg {
    pub(super) fn from_config_value(
        config_value: &ConfigValue,
        required: Option<bool>,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
        let mut names;
        let mut arg_type;
        let mut placeholders;
        let mut leftovers;

        let mut desc = None;
        let mut dest = None;
        let mut required = required;
        let mut default = None;
        let mut default_missing_value = None;
        let mut num_values = None;
        let mut value_delimiter = None;
        let mut last_arg_double_hyphen = false;
        let mut allow_hyphen_values = false;
        let mut allow_negative_numbers = false;
        let mut group_occurrences = false;
        let mut requires = vec![];
        let mut conflicts_with = vec![];
        let mut required_without = vec![];
        let mut required_without_all = vec![];
        let mut required_if_eq = HashMap::new();
        let mut required_if_eq_all = HashMap::new();

        if let Some(table) = config_value.as_table() {
            let value_for_details;

            if let Some(name_value) = table.get("name") {
                if let Some(name_value) = name_value.as_str() {
                    (names, arg_type, placeholders, leftovers) = parse_arg_name(&name_value);
                    value_for_details = Some(config_value.clone());
                } else {
                    error_handler
                        .with_key("name")
                        .with_expected("string")
                        .with_actual(name_value)
                        .error(ConfigErrorKind::InvalidValueType);
                    return None;
                }
            } else if table.len() == 1 {
                if let Some((key, value)) = table.into_iter().next() {
                    (names, arg_type, placeholders, leftovers) = parse_arg_name(&key);
                    value_for_details = Some(value);
                } else {
                    return None;
                }
            } else {
                error_handler
                    .with_key("name")
                    .error(ConfigErrorKind::MissingKey);
                return None;
            }

            if let Some(value_for_details) = value_for_details {
                if let Some(value_str) = value_for_details.as_str() {
                    desc = Some(value_str.to_string());
                } else if let Some(value_table) = value_for_details.as_table() {
                    desc = value_for_details
                        .get_as_str_or_none("desc", &error_handler.with_key("desc"));
                    dest = value_for_details
                        .get_as_str_or_none("dest", &error_handler.with_key("dest"));

                    if required.is_none() {
                        required = Some(value_for_details.get_as_bool_or_default(
                            "required",
                            false,
                            &error_handler.with_key("required"),
                        ));
                    }

                    // Try to load the placeholders from the placeholders key,
                    // if not found, try to load it from the placeholder key
                    for key in &["placeholders", "placeholder"] {
                        let ph =
                            value_for_details.get_as_str_array(key, &error_handler.with_key(key));
                        if !ph.is_empty() {
                            placeholders = ph;
                            break;
                        }
                    }

                    default = value_for_details
                        .get_as_str_or_none("default", &error_handler.with_key("default"));
                    default_missing_value = value_for_details.get_as_str_or_none(
                        "default_missing_value",
                        &error_handler.with_key("default_missing_value"),
                    );
                    num_values = SyntaxOptArgNumValues::from_config_value(
                        value_table.get("num_values"),
                        &error_handler.with_key("num_values"),
                    );
                    value_delimiter = value_for_details
                        .get_as_str_or_none("delimiter", &error_handler.with_key("delimiter"))
                        .and_then(|value| {
                            value.chars().next().or_else(|| {
                                error_handler
                                    .with_key("delimiter")
                                    .with_expected("non-empty string")
                                    .with_actual(value)
                                    .error(ConfigErrorKind::InvalidValueType);
                                None
                            })
                        });
                    last_arg_double_hyphen = value_for_details.get_as_bool_or_default(
                        "last",
                        false,
                        &error_handler.with_key("last"),
                    );
                    leftovers = value_for_details.get_as_bool_or_default(
                        "leftovers",
                        false,
                        &error_handler.with_key("leftovers"),
                    );
                    allow_hyphen_values = value_for_details.get_as_bool_or_default(
                        "allow_hyphen_values",
                        false,
                        &error_handler.with_key("allow_hyphen_values"),
                    );
                    allow_negative_numbers = value_for_details.get_as_bool_or_default(
                        "allow_negative_numbers",
                        false,
                        &error_handler.with_key("allow_negative_numbers"),
                    );
                    group_occurrences = value_for_details.get_as_bool_or_default(
                        "group_occurrences",
                        false,
                        &error_handler.with_key("group_occurrences"),
                    );

                    arg_type = SyntaxOptArgType::from_config_value(
                        value_table.get("type"),
                        value_table.get("values"),
                        value_delimiter,
                        &error_handler.with_key("type"),
                    )
                    .unwrap_or(SyntaxOptArgType::String);

                    requires = value_for_details
                        .get_as_str_array("requires", &error_handler.with_key("requires"));

                    conflicts_with = value_for_details.get_as_str_array(
                        "conflicts_with",
                        &error_handler.with_key("conflicts_with"),
                    );

                    required_without = value_for_details.get_as_str_array(
                        "required_without",
                        &error_handler.with_key("required_without"),
                    );

                    required_without_all = value_for_details.get_as_str_array(
                        "required_without_all",
                        &error_handler.with_key("required_without_all"),
                    );

                    if let Some(required_if_eq_value) = value_table.get("required_if_eq") {
                        if let Some(value) = required_if_eq_value.as_table() {
                            for (key, value) in value {
                                if let Some(value) = value.as_str_forced() {
                                    required_if_eq.insert(key.to_string(), value.to_string());
                                } else {
                                    error_handler
                                        .with_key("required_if_eq")
                                        .with_key(key)
                                        .with_expected("string")
                                        .with_actual(value)
                                        .error(ConfigErrorKind::InvalidValueType);
                                }
                            }
                        } else {
                            error_handler
                                .with_key("required_if_eq")
                                .with_expected("table")
                                .with_actual(required_if_eq_value)
                                .error(ConfigErrorKind::InvalidValueType);
                        }
                    }

                    if let Some(required_if_eq_all_value) = value_table.get("required_if_eq_all") {
                        if let Some(value) = required_if_eq_all_value.as_table() {
                            for (key, value) in value {
                                if let Some(value) = value.as_str_forced() {
                                    required_if_eq_all.insert(key.to_string(), value.to_string());
                                } else {
                                    error_handler
                                        .with_key("required_if_eq_all")
                                        .with_key(key)
                                        .with_expected("string")
                                        .with_actual(value)
                                        .error(ConfigErrorKind::InvalidValueType);
                                }
                            }
                        } else {
                            error_handler
                                .with_key("required_if_eq_all")
                                .with_expected("table")
                                .with_actual(required_if_eq_all_value)
                                .error(ConfigErrorKind::InvalidValueType);
                        }
                    }

                    let aliases = value_for_details
                        .get_as_str_array("aliases", &error_handler.with_key("aliases"));
                    names.extend(aliases);
                }
            }
        } else if let Some(value) = config_value.as_str() {
            (names, arg_type, placeholders, leftovers) = parse_arg_name(&value);
        } else {
            error_handler
                .with_expected("string or table")
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);
            return None;
        }

        Some(Self {
            names,
            dest,
            desc,
            required: required.unwrap_or(false),
            placeholders,
            arg_type,
            default,
            default_missing_value,
            num_values,
            value_delimiter,
            last_arg_double_hyphen,
            leftovers,
            allow_hyphen_values,
            allow_negative_numbers,
            group_occurrences,
            requires,
            conflicts_with,
            required_without,
            required_without_all,
            required_if_eq,
            required_if_eq_all,
        })
    }

    pub fn arg_type(&self) -> SyntaxOptArgType {
        let convert_to_array = self.leftovers || self.value_delimiter.is_some();

        if convert_to_array {
            match &self.arg_type {
                SyntaxOptArgType::String
                | SyntaxOptArgType::Integer
                | SyntaxOptArgType::Float
                | SyntaxOptArgType::Boolean
                | SyntaxOptArgType::Enum(_) => {
                    SyntaxOptArgType::Array(Box::new(self.arg_type.clone()))
                }
                _ => self.arg_type.clone(),
            }
        } else {
            self.arg_type.clone()
        }
    }

    pub fn dest(&self) -> String {
        let dest = match self.dest {
            Some(ref dest) => dest.clone(),
            None => self.name().clone(),
        };

        sanitize_str(&dest)
    }

    fn organized_names(
        &self,
    ) -> (
        String,
        Option<String>,
        Option<String>,
        Vec<String>,
        Vec<String>,
    ) {
        let long_names = self
            .names
            .iter()
            .filter(|name| name.starts_with("--"))
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        let (main_long, long_names) = long_names
            .split_first()
            .map(|(f, r)| (Some(f.clone()), r.to_vec()))
            .unwrap_or((None, vec![]));

        let short_names = self
            .names
            .iter()
            .filter(|name| name.starts_with('-') && !name.starts_with("--"))
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        let (main_short, short_names) = short_names
            .split_first()
            .map(|(f, r)| (Some(f.clone()), r.to_vec()))
            .unwrap_or((None, vec![]));

        let main = if let Some(main_long) = &main_long {
            main_long.clone()
        } else if let Some(main_short) = &main_short {
            main_short.clone()
        } else {
            self.names
                .first()
                .expect("name should have at least one value")
                .clone()
        };

        (main, main_long, main_short, long_names, short_names)
    }

    pub fn name(&self) -> String {
        let (main, _, _, _, _) = self.organized_names();
        main
    }

    pub fn all_names(&self) -> Vec<String> {
        self.names.clone()
    }

    pub fn is_positional(&self) -> bool {
        !self.name().starts_with('-')
    }

    pub fn is_last(&self) -> bool {
        self.last_arg_double_hyphen
    }

    pub fn is_repeatable(&self) -> bool {
        self.arg_type().is_array() || matches!(self.arg_type(), SyntaxOptArgType::Counter)
    }

    pub fn takes_value(&self) -> bool {
        if matches!(
            self.arg_type(),
            SyntaxOptArgType::Flag | SyntaxOptArgType::Counter
        ) {
            return false;
        }

        if let Some(SyntaxOptArgNumValues::Exactly(0)) = self.num_values {
            return false;
        }

        true
    }

    /// Returns the representation of that argument for the
    /// 'usage' string in the help message
    pub fn usage(&self) -> String {
        self.help_name(false, true)
    }

    /// Returns the representation of that argument for the help message
    /// This will include:
    /// - For a positional, only the placeholder "num_values" times
    /// - For an optional, the main long and main short, with the placeholder "num_values" times
    ///
    /// The "include_short" parameter influences if the short is shown or not for an optional.
    /// The "use_colors" parameter influences if the output should be colored or not.
    pub fn help_name(&self, include_short: bool, use_colors: bool) -> String {
        let mut help_name = String::new();

        if self.is_positional() {
            let placeholders = if self.placeholders.is_empty() {
                vec![sanitize_str(&self.name()).to_uppercase()]
            } else {
                self.placeholders.clone()
            };

            let placeholders = placeholders
                .iter()
                .map(|ph| {
                    if self.required {
                        format!("<{ph}>")
                    } else {
                        format!("[{ph}]")
                    }
                })
                .map(|ph| if use_colors { ph.light_cyan() } else { ph })
                .collect::<Vec<_>>();

            let (min_num, max_num) = match &self.num_values {
                Some(SyntaxOptArgNumValues::Exactly(n)) => (*n, Some(*n)),
                Some(SyntaxOptArgNumValues::AtLeast(min)) => (*min, None),
                Some(SyntaxOptArgNumValues::AtMost(max)) => (0, Some(*max)),
                Some(SyntaxOptArgNumValues::Any) => (0, None),
                Some(SyntaxOptArgNumValues::Between(min, max)) => (*min, Some(*max)),
                None => (1, Some(1)),
            };

            // Get the max between min and 1
            let min_placeholders = std::cmp::max(min_num, 1);
            let repr = placeholders
                .iter()
                .cycle()
                .take(min_placeholders)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");

            // If the max is None or greater than min, or if the arg type is an array
            // we need to add "..." to the end
            let repr =
                if self.arg_type().is_array() || max_num.is_none() || max_num.unwrap() > min_num {
                    format!("{repr}...")
                } else {
                    repr
                };

            help_name.push_str(&repr);
        } else {
            // Split the short and long names, and only keep the first of each (return Option<_>)
            let all_names = self.all_names();
            let (short_name, long_name): (Vec<_>, Vec<_>) =
                all_names.iter().partition(|name| !name.starts_with("--"));
            let short_name = short_name.first();
            let long_name = long_name.first();

            if include_short || long_name.is_none() {
                if let Some(short_name) = short_name {
                    let short_name = if use_colors {
                        short_name.bold().light_cyan()
                    } else {
                        short_name.to_string()
                    };
                    help_name.push_str(&short_name);

                    if long_name.is_some() {
                        help_name.push_str(", ");
                    }
                }
            }
            if let Some(long_name) = long_name {
                let long_name = if use_colors {
                    long_name.bold().light_cyan()
                } else {
                    long_name.to_string()
                };
                help_name.push_str(&long_name);
            }

            if self.takes_value() {
                let placeholders = if self.placeholders.is_empty() {
                    vec![sanitize_str(&self.name()).to_uppercase()]
                } else {
                    self.placeholders.clone()
                };

                let (min_num, max_num) = match &self.num_values {
                    Some(SyntaxOptArgNumValues::Exactly(n)) => (*n, Some(*n)),
                    Some(SyntaxOptArgNumValues::AtLeast(min)) => (*min, None),
                    Some(SyntaxOptArgNumValues::AtMost(max)) => (0, Some(*max)),
                    Some(SyntaxOptArgNumValues::Any) => (0, None),
                    Some(SyntaxOptArgNumValues::Between(min, max)) => (*min, Some(*max)),
                    None => (1, Some(1)),
                };

                let repr = match (min_num, max_num) {
                    (0, Some(0)) => "".to_string(),
                    (1, Some(1)) => {
                        let repr = format!(
                            "<{}>",
                            placeholders
                                .first()
                                .expect("there should be at least one placeholder")
                        );
                        if use_colors {
                            repr.light_cyan()
                        } else {
                            repr
                        }
                    }
                    (min, Some(max)) if min == max => {
                        // Placeholders can be N elements, e.g. A, B, C
                        // We want to go over placeholders for M values, e.g. A B C A B C if M > N,
                        // or A B C if M == N, or A B if M < N
                        placeholders
                            .iter()
                            .cycle()
                            .take(min)
                            .map(|repr| format!("<{repr}>"))
                            .map(|repr| if use_colors { repr.light_cyan() } else { repr })
                            .collect::<Vec<_>>()
                            .join(" ")
                    }
                    (0, Some(1)) => {
                        let repr = format!(
                            "[{}]",
                            placeholders
                                .first()
                                .expect("there should be at least one placeholder")
                        );
                        if use_colors {
                            repr.light_cyan()
                        } else {
                            repr
                        }
                    }
                    (0, _) => {
                        let repr = format!(
                            "[{}]",
                            placeholders
                                .first()
                                .expect("there should be at least one placeholder")
                        );
                        let repr = if use_colors { repr.light_cyan() } else { repr };
                        format!("{repr}...")
                    }
                    (min, _) => {
                        let repr = placeholders
                            .iter()
                            .cycle()
                            .take(min)
                            .map(|repr| format!("<{repr}>"))
                            .map(|repr| if use_colors { repr.light_cyan() } else { repr })
                            .collect::<Vec<_>>()
                            .join(" ");
                        format!("{repr}...")
                    }
                };

                if !repr.is_empty() {
                    help_name.push(' ');
                    help_name.push_str(&repr);
                }
            } else if matches!(self.arg_type, SyntaxOptArgType::Counter) {
                help_name.push_str("...");
            }
        }

        help_name
    }

    /// Returns the description of that argument for the help message
    pub fn help_desc(&self) -> String {
        let mut help_desc = String::new();

        // Add the description if any
        if let Some(desc) = &self.desc {
            help_desc.push_str(desc);
        }

        // Add the default value if any
        if !matches!(self.arg_type, SyntaxOptArgType::Flag) {
            if let Some(default) = &self.default {
                if !default.is_empty() {
                    if !help_desc.is_empty() {
                        help_desc.push(' ');
                    }
                    help_desc
                        .push_str(&format!("[{}: {}]", "default".italic(), default).light_black());
                }
            }

            if let Some(default_missing_value) = &self.default_missing_value {
                if !default_missing_value.is_empty() {
                    if !help_desc.is_empty() {
                        help_desc.push(' ');
                    }
                    help_desc.push_str(
                        &format!(
                            "[{}: {}]",
                            "default missing value".italic(),
                            default_missing_value
                        )
                        .light_black(),
                    );
                }
            }
        }

        // Add the possible values if any
        if let Some(possible_values) = self.arg_type().possible_values() {
            if !help_desc.is_empty() {
                help_desc.push(' ');
            }
            help_desc.push_str(
                &format!(
                    "[{}: {}]",
                    "possible values".italic(),
                    possible_values.join(", ")
                )
                .light_black(),
            );
        }

        // Add the aliases if any
        let (_, _, _, long_aliases, short_aliases) = self.organized_names();

        if !long_aliases.is_empty() {
            if !help_desc.is_empty() {
                help_desc.push(' ');
            }

            help_desc.push_str(
                &format!("[{}: {}]", "aliases".italic(), long_aliases.join(", ")).light_black(),
            );
        }

        if !short_aliases.is_empty() {
            if !help_desc.is_empty() {
                help_desc.push(' ');
            }

            help_desc.push_str(
                &format!(
                    "[{}: {}]",
                    "short aliases".italic(),
                    short_aliases.join(", ")
                )
                .light_black(),
            );
        }

        help_desc
    }

    pub fn add_to_argparser(&self, parser: clap::Command) -> clap::Command {
        let mut arg = clap::Arg::new(self.dest());

        // Add the help for the argument
        if let Some(desc) = &self.desc {
            arg = arg.help(desc);
        }

        // Add all the names for that argument
        if !self.is_positional() {
            let (_, main_long, main_short, long_names, short_names) = self.organized_names();

            if let Some(main_long) = &main_long {
                if sanitize_str(main_long).is_empty() {
                    // TODO: raise error ?
                    return parser;
                }

                let long = main_long.trim_start_matches("-").to_string();
                arg = arg.long(long);
            }

            if let Some(main_short) = &main_short {
                if sanitize_str(main_short).is_empty() {
                    // TODO: raise error ?
                    return parser;
                }

                let short = main_short
                    .trim_start_matches("-")
                    .chars()
                    .next()
                    .expect("short name should have at least one character");
                arg = arg.short(short);
            }

            for long_name in &long_names {
                if sanitize_str(long_name).is_empty() {
                    continue;
                }

                let long = long_name.trim_start_matches("-").to_string();
                arg = arg.visible_alias(long);
            }

            for short_name in &short_names {
                if sanitize_str(short_name).is_empty() {
                    continue;
                }

                let short = short_name
                    .trim_start_matches("-")
                    .chars()
                    .next()
                    .expect("short name should have at least one character");
                arg = arg.visible_short_alias(short);
            }
        }

        // Set the placeholder if any
        if !self.placeholders.is_empty() {
            let placeholders = match &self.num_values {
                Some(n) => match n.max() {
                    Some(max) => self
                        .placeholders
                        .iter()
                        .cycle()
                        .take(max)
                        .map(|ph| ph.to_string())
                        .collect::<Vec<_>>(),
                    None => self.placeholders.clone(),
                },
                None => self.placeholders.clone(),
            };
            arg = arg.value_names(placeholders);
        }

        // Set the default value
        if let Some(default) = &self.default {
            arg = arg.default_value(default);
        }

        // Set the default missing value
        if let Some(default_missing_value) = &self.default_missing_value {
            arg = arg.default_missing_value(default_missing_value);
        }

        // Set how to parse the values
        if let Some(num_values) = &self.num_values {
            arg = arg.num_args(*num_values);
        }
        if let Some(value_delimiter) = &self.value_delimiter {
            arg = arg.value_delimiter(*value_delimiter);
        }
        if self.last_arg_double_hyphen {
            arg = arg.last(true);
        }
        if self.leftovers {
            arg = arg.trailing_var_arg(true);
        }
        if self.allow_hyphen_values {
            arg = arg.allow_hyphen_values(true);
        }
        if self.allow_negative_numbers {
            arg = arg.allow_negative_numbers(true);
        }

        // Set conflicts and requirements
        for require_arg in &self.requires {
            let require_arg = sanitize_str(require_arg);
            arg = arg.requires(&require_arg);
        }
        for conflict_arg in &self.conflicts_with {
            let conflict_arg = sanitize_str(conflict_arg);
            arg = arg.conflicts_with(&conflict_arg);
        }
        if !self.required_without.is_empty() {
            let required_without = self
                .required_without
                .iter()
                .map(|name| sanitize_str(name))
                .collect::<Vec<String>>();
            arg = arg.required_unless_present_any(&required_without);
        }
        if !self.required_without_all.is_empty() {
            let required_without_all = self
                .required_without_all
                .iter()
                .map(|name| sanitize_str(name))
                .collect::<Vec<String>>();
            arg = arg.required_unless_present_all(&required_without_all);
        }
        if !self.required_if_eq.is_empty() {
            arg = arg.required_if_eq_any(
                self.required_if_eq
                    .iter()
                    .map(|(k, v)| (sanitize_str(k), v.clone()))
                    .collect::<Vec<(String, String)>>(),
            );
        }
        if !self.required_if_eq_all.is_empty() {
            arg = arg.required_if_eq_all(
                self.required_if_eq_all
                    .iter()
                    .map(|(k, v)| (sanitize_str(k), v.clone()))
                    .collect::<Vec<(String, String)>>(),
            );
        }
        if self.required {
            arg = arg.required(true);
        }

        // Set the action, i.e. how the values are stored when the selfeter is used
        match &self.arg_type() {
            SyntaxOptArgType::String
            | SyntaxOptArgType::DirPath
            | SyntaxOptArgType::FilePath
            | SyntaxOptArgType::RepoPath
            | SyntaxOptArgType::Integer
            | SyntaxOptArgType::Float
            | SyntaxOptArgType::Boolean
            | SyntaxOptArgType::Enum(_) => {
                arg = arg.action(clap::ArgAction::Set);
            }
            SyntaxOptArgType::Array(_) => {
                arg = arg.action(clap::ArgAction::Append);
            }
            SyntaxOptArgType::Flag => {
                if str_to_bool(&self.default.clone().unwrap_or_default()).unwrap_or(false) {
                    arg = arg.action(clap::ArgAction::SetFalse);
                } else {
                    arg = arg.action(clap::ArgAction::SetTrue);
                }
            }
            SyntaxOptArgType::Counter => {
                arg = arg.action(clap::ArgAction::Count);
            }
        };

        // Set the validators, i.e. how the values are checked when the parameter is used
        match &self.arg_type().terminal_type() {
            SyntaxOptArgType::Integer => {
                arg = arg.value_parser(clap::value_parser!(i64));
            }
            SyntaxOptArgType::Float => {
                arg = arg.value_parser(clap::value_parser!(f64));
            }
            SyntaxOptArgType::Boolean => {
                arg = arg.value_parser(clap::value_parser!(bool));
            }
            SyntaxOptArgType::Enum(possible_values) => {
                arg = arg.value_parser(possible_values.clone());
            }
            _ => {}
        }

        parser.arg(arg)
    }

    pub fn add_to_args(
        &self,
        args: &mut BTreeMap<String, ParseArgsValue>,
        matches: &clap::ArgMatches,
        override_dest: Option<String>,
    ) -> Result<(), ParseArgsErrorKind> {
        let dest = self.dest();

        // has_occurrences is when an argument can take multiple values
        let has_occurrences = self
            .num_values
            .as_ref()
            .is_some_and(|num_values| num_values.is_many());

        // has_multi is when an argument can be called multiple times
        let arg_type = self.arg_type();
        let has_multi = arg_type.is_array();

        let terminal_type = &arg_type.terminal_type();
        match terminal_type {
            SyntaxOptArgType::String
            | SyntaxOptArgType::DirPath
            | SyntaxOptArgType::FilePath
            | SyntaxOptArgType::RepoPath
            | SyntaxOptArgType::Enum(_) => {
                extract_value_to_typed::<String>(
                    matches,
                    &dest,
                    &self.default,
                    args,
                    override_dest,
                    has_occurrences,
                    has_multi,
                    self.group_occurrences,
                    match terminal_type {
                        SyntaxOptArgType::DirPath | SyntaxOptArgType::FilePath => {
                            Some(transform_path)
                        }
                        SyntaxOptArgType::RepoPath => Some(transform_repo_path),
                        _ => None,
                    },
                )?;
            }
            SyntaxOptArgType::Integer => {
                extract_value_to_typed::<i64>(
                    matches,
                    &dest,
                    &self.default,
                    args,
                    override_dest,
                    has_occurrences,
                    has_multi,
                    self.group_occurrences,
                    None,
                )?;
            }
            SyntaxOptArgType::Counter => {
                extract_value_to_typed::<u8>(
                    matches,
                    &dest,
                    &self.default,
                    args,
                    override_dest,
                    has_occurrences,
                    has_multi,
                    self.group_occurrences,
                    None,
                )?;
            }
            SyntaxOptArgType::Float => {
                extract_value_to_typed::<f64>(
                    matches,
                    &dest,
                    &self.default,
                    args,
                    override_dest,
                    has_occurrences,
                    has_multi,
                    self.group_occurrences,
                    None,
                )?;
            }
            SyntaxOptArgType::Boolean | SyntaxOptArgType::Flag => {
                let default = Some(
                    str_to_bool(&self.default.clone().unwrap_or_default())
                        .unwrap_or(false)
                        .to_string(),
                );
                extract_value_to_typed::<bool>(
                    matches,
                    &dest,
                    &default,
                    args,
                    override_dest,
                    has_occurrences,
                    has_multi,
                    self.group_occurrences,
                    None,
                )?;
            }
            SyntaxOptArgType::Array(_) => unreachable!("array type should be handled differently"),
        }

        Ok(())
    }
}

/// If the provided value is a path, we want to return the
/// absolute path no matter what was passed (relative, absolute, ~, etc.)
fn transform_path(value: Option<String>) -> Result<Option<String>, ParseArgsErrorKind> {
    let value = match value {
        Some(value) => value,
        None => return Ok(None),
    };

    let path = abs_path(&value);
    Ok(Some(path.to_string_lossy().to_string()))
}

/// If the provided value is a path to a repository, we want to return the
/// absolute path no matter what was passed (relative, absolute, ~, etc.)
fn transform_repo_path(value: Option<String>) -> Result<Option<String>, ParseArgsErrorKind> {
    let value = match value {
        Some(value) => value,
        None => return Ok(None),
    };

    if let Ok(path) = std::fs::canonicalize(&value) {
        return Ok(Some(path.to_string_lossy().to_string()));
    }

    let only_worktree = false;
    if let Some(path) = ORG_LOADER.find_repo(&value, only_worktree, false, true) {
        return Ok(Some(path.to_string_lossy().to_string()));
    }

    Err(ParseArgsErrorKind::InvalidValue(format!(
        "invalid repository path: {value}"
    )))
}

trait ParserExtractType<T> {
    type BaseType;
    type Output;

    fn extract(matches: &clap::ArgMatches, dest: &str, default: &Option<String>) -> Self::Output;
}

impl<T: Into<ParseArgsValue> + Clone + FromStr + Send + Sync + 'static> ParserExtractType<T>
    for Option<T>
{
    type BaseType = T;
    type Output = Option<T>;

    fn extract(matches: &clap::ArgMatches, dest: &str, default: &Option<String>) -> Self::Output {
        match (matches.get_one::<T>(dest), default) {
            (Some(value), _) => Some(value.clone()),
            (None, Some(default)) => default.parse::<T>().ok(),
            _ => None,
        }
    }
}

impl<T: Into<ParseArgsValue> + Clone + FromStr + Send + Sync + 'static> ParserExtractType<T>
    for Vec<Option<T>>
{
    type BaseType = T;
    type Output = Vec<Option<T>>;

    fn extract(matches: &clap::ArgMatches, dest: &str, default: &Option<String>) -> Self::Output {
        match (matches.get_many::<T>(dest), default) {
            (Some(values), _) => values
                .collect::<Vec<_>>()
                .into_iter()
                .map(|value| Some(value.clone()))
                .collect(),
            (None, Some(default)) => default
                .split(',')
                .flat_map(|part| part.trim().parse::<T>())
                .map(|value| Some(value.clone()))
                .collect(),
            _ => vec![],
        }
    }
}

impl<T: Into<ParseArgsValue> + Clone + FromStr + Send + Sync + 'static> ParserExtractType<T>
    for Vec<Vec<Option<T>>>
{
    type BaseType = T;
    type Output = Vec<Vec<Option<T>>>;

    fn extract(matches: &clap::ArgMatches, dest: &str, default: &Option<String>) -> Self::Output {
        match (matches.get_occurrences(dest), default) {
            (Some(occurrences), _) => occurrences
                .into_iter()
                .map(|values| {
                    values
                        .into_iter()
                        .map(|value: &T| Some(value.clone()))
                        .collect()
                })
                .collect(),
            (None, Some(default)) => vec![default
                .split(',')
                .flat_map(|part| part.trim().parse::<T>().map(|value| Some(value.clone())))
                .collect()],
            _ => vec![],
        }
    }
}

/// A function that can transform a value into another value of the same type
type TransformFn<T> = fn(Option<T>) -> Result<Option<T>, ParseArgsErrorKind>;

/// Extracts a value from the matches and inserts it into the args map
/// The value is extracted based on the type of the argument and the number of values
/// The value is then transformed if a transform function is provided
/// The value is then inserted into the args map with the correct destination
#[allow(clippy::too_many_arguments)]
#[inline]
fn extract_value_to_typed<T>(
    matches: &clap::ArgMatches,
    dest: &str,
    default: &Option<String>,
    args: &mut BTreeMap<String, ParseArgsValue>,
    override_dest: Option<String>,
    has_occurrences: bool,
    has_multi: bool,
    group_occurrences: bool,
    transform_fn: Option<TransformFn<T>>,
) -> Result<(), ParseArgsErrorKind>
where
    T: Into<ParseArgsValue> + Clone + Send + Sync + FromStr + 'static,
    ParseArgsValue: From<Option<T>>,
    ParseArgsValue: From<Vec<Option<T>>>,
    ParseArgsValue: From<Vec<Vec<Option<T>>>>,
{
    let arg_dest = override_dest.unwrap_or(dest.to_string());

    let value = if has_occurrences && has_multi && group_occurrences {
        let value = <Vec<Vec<Option<T>>> as ParserExtractType<T>>::extract(matches, dest, default);
        let value = if let Some(transform_fn) = transform_fn {
            value
                .into_iter()
                .map(|values| {
                    values
                        .into_iter()
                        .map(transform_fn)
                        .collect::<Result<_, _>>()
                })
                .collect::<Result<_, _>>()?
        } else {
            value
        };
        ParseArgsValue::from(value)
    } else if has_multi || has_occurrences {
        let value = <Vec<Option<T>> as ParserExtractType<T>>::extract(matches, dest, default);
        let value = if let Some(transform_fn) = transform_fn {
            value
                .into_iter()
                .map(transform_fn)
                .collect::<Result<_, _>>()?
        } else {
            value
        };
        ParseArgsValue::from(value)
    } else {
        let value = <Option<T> as ParserExtractType<T>>::extract(matches, dest, default);
        let value = if let Some(transform_fn) = transform_fn {
            transform_fn(value)?
        } else {
            value
        };
        ParseArgsValue::from(value)
    };

    args.insert(arg_dest, value);

    Ok(())
}

pub fn parse_arg_name(arg_name: &str) -> (Vec<String>, SyntaxOptArgType, Vec<String>, bool) {
    let mut names = Vec::new();
    let mut arg_type = SyntaxOptArgType::String;
    let mut placeholders = vec![];
    let mut leftovers = false;

    // Parse the argument name; it can be a single name or multiple names separated by commas.
    // There can be short names (starting with `-`) and long names (starting with `--`).
    // Each name can have one or more placeholders, or the placeholders can be put at the end.
    // The placeholders are separated by a space from the name, and by a space from each other.
    // If the argument name does not start with `-`, only this value will be kept as part of
    // the names and the others will be ignored.
    let def_parts: Vec<&str> = arg_name.split(',').map(str::trim).collect();

    for part in def_parts {
        let name_parts = part.splitn(2, [' ', '\t', '=']).collect::<Vec<&str>>();
        if name_parts.is_empty() {
            continue;
        }

        let name = name_parts[0];
        let (name, ends_with_dots) = if name.ends_with("...") {
            (name.trim_end_matches("..."), true)
        } else {
            (name, false)
        };

        if name.starts_with('-') {
            if name_parts.len() > 1 {
                placeholders.extend(
                    name_parts[1]
                        .split_whitespace()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<String>>(),
                );
            }

            if ends_with_dots {
                // If the name ends with `...`, we consider it a counter
                arg_type = SyntaxOptArgType::Counter;
            }

            names.push(name.to_string());
        } else {
            names.clear();
            names.push(name.to_string());

            if ends_with_dots {
                // If the name ends with `...`, we consider it as a last argument
                leftovers = true;
            }

            if name_parts.len() > 1 {
                placeholders.push(
                    name_parts[1]
                        .split_whitespace()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect(),
                );
            }

            // If we have a parameter without a leading `-`, we stop parsing
            // the rest of the arg name since this is a positional argument
            break;
        }
    }

    (names, arg_type, placeholders, leftovers)
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Copy)]
pub enum SyntaxOptArgNumValues {
    Any,
    Exactly(usize),
    AtLeast(usize),
    AtMost(usize),
    Between(usize, usize),
}

impl fmt::Display for SyntaxOptArgNumValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => write!(f, ".."),
            Self::Exactly(value) => write!(f, "{value}"),
            Self::AtLeast(min) => write!(f, "{min}.."),
            Self::AtMost(max) => write!(f, "..={max}"),
            Self::Between(min, max) => write!(f, "{min}..={max}"),
        }
    }
}

impl From<SyntaxOptArgNumValues> for clap::builder::ValueRange {
    fn from(val: SyntaxOptArgNumValues) -> Self {
        match val {
            SyntaxOptArgNumValues::Any => clap::builder::ValueRange::from(..),
            SyntaxOptArgNumValues::Exactly(value) => clap::builder::ValueRange::from(value),
            SyntaxOptArgNumValues::AtLeast(min) => clap::builder::ValueRange::from(min..),
            SyntaxOptArgNumValues::AtMost(max) => clap::builder::ValueRange::from(..=max),
            SyntaxOptArgNumValues::Between(min, max) => clap::builder::ValueRange::from(min..=max),
        }
    }
}

impl From<std::ops::RangeToInclusive<usize>> for SyntaxOptArgNumValues {
    fn from(range: std::ops::RangeToInclusive<usize>) -> Self {
        let max = range.end;
        Self::AtMost(max)
    }
}

impl From<std::ops::RangeTo<usize>> for SyntaxOptArgNumValues {
    fn from(range: std::ops::RangeTo<usize>) -> Self {
        let max = range.end;
        Self::AtMost(max - 1)
    }
}

impl From<std::ops::RangeFrom<usize>> for SyntaxOptArgNumValues {
    fn from(range: std::ops::RangeFrom<usize>) -> Self {
        let min = range.start;
        Self::AtLeast(min)
    }
}

impl From<std::ops::RangeInclusive<usize>> for SyntaxOptArgNumValues {
    fn from(range: std::ops::RangeInclusive<usize>) -> Self {
        let (min, max) = range.into_inner();
        Self::Between(min, max)
    }
}

impl From<std::ops::Range<usize>> for SyntaxOptArgNumValues {
    fn from(range: std::ops::Range<usize>) -> Self {
        let (min, max) = (range.start, range.end);
        Self::Between(min, max)
    }
}

impl From<std::ops::RangeFull> for SyntaxOptArgNumValues {
    fn from(_: std::ops::RangeFull) -> Self {
        Self::Any
    }
}

impl From<usize> for SyntaxOptArgNumValues {
    fn from(value: usize) -> Self {
        Self::Exactly(value)
    }
}

impl SyntaxOptArgNumValues {
    pub fn from_str(value: &str, error_handler: &ConfigErrorHandler) -> Option<Self> {
        let value = value.trim();

        if value.contains("..") {
            let mut parts = value.split("..");

            let min = parts.next()?.trim();
            let max = parts.next()?.trim();
            let (max, max_inclusive) = if let Some(max) = max.strip_prefix('=') {
                (max, true)
            } else {
                (max, false)
            };

            let max = match max {
                "" => None,
                value => match value.parse::<usize>() {
                    Ok(value) => Some(value),
                    Err(_) => {
                        error_handler
                            .with_expected("positive integer")
                            .with_actual(value)
                            .error(ConfigErrorKind::InvalidValueType);
                        return None;
                    }
                },
            };

            let min = match min {
                "" => None,
                value => match value.parse::<usize>() {
                    Ok(value) => Some(value),
                    Err(_) => {
                        error_handler
                            .with_expected("positive integer")
                            .with_actual(value)
                            .error(ConfigErrorKind::InvalidValueType);
                        return None;
                    }
                },
            };

            match (min, max, max_inclusive) {
                (None, None, _) => Some(Self::Any),
                (None, Some(max), true) => Some(Self::AtMost(max)),
                (None, Some(max), false) => {
                    if max > 0 {
                        Some(Self::AtMost(max - 1))
                    } else {
                        error_handler
                            .with_context("min", 0)
                            .with_context("max", 0)
                            .error(ConfigErrorKind::InvalidRange);
                        None
                    }
                }
                (Some(min), None, _) => Some(Self::AtLeast(min)),
                (Some(min), Some(max), true) => {
                    if min <= max {
                        Some(Self::Between(min, max))
                    } else {
                        error_handler
                            .with_context("min", min)
                            .with_context("max", max + 1)
                            .error(ConfigErrorKind::InvalidRange);
                        None
                    }
                }
                (Some(min), Some(max), false) => {
                    if min < max {
                        Some(Self::Between(min, max - 1))
                    } else {
                        error_handler
                            .with_context("min", min)
                            .with_context("max", max)
                            .error(ConfigErrorKind::InvalidRange);
                        None
                    }
                }
            }
        } else {
            let value = match value.parse::<usize>() {
                Ok(value) => Some(value),
                Err(_) => {
                    error_handler
                        .with_expected("positive integer")
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValueType);
                    None
                }
            }?;
            Some(Self::Exactly(value))
        }
    }

    fn from_config_value(
        config_value: Option<&ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
        let config_value = config_value?;

        if let Some(value) = config_value.as_integer() {
            Some(Self::Exactly(value as usize))
        } else if let Some(value) = config_value.as_str_forced() {
            Self::from_str(&value, error_handler)
        } else {
            error_handler
                .with_expected("positive integer or range")
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);
            None
        }
    }

    fn is_many(&self) -> bool {
        match self {
            Self::Any => true,
            Self::Exactly(value) => *value > 1,
            Self::AtLeast(_min) => true, // AtLeast is always many since it is not bounded by a maximum
            Self::AtMost(max) => *max > 1,
            Self::Between(_min, max) => *max > 1,
        }
    }

    pub fn max(&self) -> Option<usize> {
        match self {
            Self::Any => None,
            Self::Exactly(value) => Some(*value),
            Self::AtLeast(_min) => None,
            Self::AtMost(max) => Some(*max),
            Self::Between(_min, max) => Some(*max),
        }
    }

    pub fn min(&self) -> Option<usize> {
        match self {
            Self::Any => None,
            Self::Exactly(value) => Some(*value),
            Self::AtLeast(min) => Some(*min),
            Self::AtMost(_max) => None,
            Self::Between(min, _max) => Some(*min),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum SyntaxOptArgType {
    #[default]
    #[serde(rename = "str", alias = "string")]
    String,
    #[serde(rename = "dir", alias = "dirpath", alias = "path")]
    DirPath,
    #[serde(rename = "file", alias = "filepath")]
    FilePath,
    #[serde(rename = "repopath")]
    RepoPath,
    #[serde(rename = "int", alias = "integer")]
    Integer,
    #[serde(rename = "float")]
    Float,
    #[serde(rename = "bool")]
    Boolean,
    #[serde(rename = "flag")]
    Flag,
    #[serde(rename = "count", alias = "counter")]
    Counter,
    #[serde(rename = "enum")]
    Enum(Vec<String>),
    #[serde(rename = "array")]
    Array(Box<SyntaxOptArgType>),
}

impl fmt::Display for SyntaxOptArgType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl SyntaxOptArgType {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::String)
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::String => "string",
            Self::DirPath => "dir",
            Self::FilePath => "file",
            Self::RepoPath => "repopath",
            Self::Integer => "int",
            Self::Float => "float",
            Self::Boolean => "bool",
            Self::Flag => "flag",
            Self::Counter => "counter",
            Self::Enum(_) => "enum",
            Self::Array(inner) => match **inner {
                Self::String => "array/str",
                Self::DirPath => "array/dir",
                Self::FilePath => "array/file",
                Self::RepoPath => "array/repopath",
                Self::Integer => "array/int",
                Self::Float => "array/float",
                Self::Boolean => "array/bool",
                Self::Enum(_) => "array/enum",
                _ => unimplemented!("unsupported array type: {:?}", self),
            },
        }
    }

    fn from_config_value(
        config_value_type: Option<&ConfigValue>,
        config_value_values: Option<&ConfigValue>,
        value_delimiter: Option<char>,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
        let config_value_type = config_value_type?;

        // Check if type is an array (list) - if so, treat as enum with those values
        if let Some(array) = config_value_type.as_array() {
            let values = array
                .iter()
                .filter_map(|value| value.as_str_forced())
                .collect::<Vec<String>>();
            return Some(Self::Enum(values));
        }

        let obj = Self::from_str(
            &config_value_type.as_str_forced().or_else(|| {
                error_handler
                    .with_expected("string or array")
                    .with_actual(config_value_type)
                    .error(ConfigErrorKind::InvalidValueType);
                None
            })?,
            error_handler,
        )?;

        match obj {
            Self::Enum(values) if values.is_empty() => {
                if let Some(values) = config_value_values {
                    if let Some(array) = values.as_array() {
                        let values = array
                            .iter()
                            .filter_map(|value| value.as_str_forced())
                            .collect::<Vec<String>>();
                        return Some(Self::Enum(values));
                    } else if let Some(value) = values.as_str_forced() {
                        if let Some(value_delimiter) = value_delimiter {
                            let values = value
                                .split(value_delimiter)
                                .map(|value| value.to_string())
                                .collect::<Vec<String>>();
                            return Some(Self::Enum(values));
                        } else {
                            return Some(Self::Enum(vec![value.to_string()]));
                        }
                    }
                }
                // TODO: add error for empty enum
            }
            _ => return Some(obj),
        }

        None
    }

    pub fn from_str(value: &str, error_handler: &ConfigErrorHandler) -> Option<Self> {
        let mut is_array = false;

        let normalized = value.trim().to_lowercase();
        let mut value = normalized.trim();

        if value.starts_with("array/") {
            value = &value[6..];
            is_array = true;
        } else if value.starts_with("[") && value.ends_with("]") {
            value = &value[1..value.len() - 1];
            is_array = true;
        } else if value == "array" {
            return Some(Self::Array(Box::new(Self::String)));
        }

        let obj = match value.to_lowercase().as_str() {
            "int" | "integer" => Self::Integer,
            "float" => Self::Float,
            "bool" | "boolean" => Self::Boolean,
            "flag" => Self::Flag,
            "count" | "counter" => Self::Counter,
            "str" | "string" => Self::String,
            "dir" | "path" | "dirpath" => Self::DirPath,
            "file" | "filepath" => Self::FilePath,
            "repopath" => Self::RepoPath,
            "enum" => Self::Enum(vec![]),
            _ => {
                // If the string is in format array/enum(xx, yy, zz) or enum(xx, yy, zz) or (xx, yy, zz)
                // or [(xx, yy, zz)], then it's an enum and we need to extract the values
                let mut enum_contents = None;

                if value.starts_with("enum(") && value.ends_with(")") {
                    enum_contents = Some(&value[5..value.len() - 1]);
                } else if value.starts_with("(") && value.ends_with(")") {
                    enum_contents = Some(&value[1..value.len() - 1]);
                }

                if let Some(enum_contents) = enum_contents {
                    let values = enum_contents
                        .split(',')
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .collect::<Vec<String>>();

                    Self::Enum(values)
                } else {
                    error_handler
                        .with_expected(vec![
                            "int",
                            "float",
                            "bool",
                            "flag",
                            "count",
                            "str",
                            "path",
                            "enum",
                            "array/<type>",
                        ])
                        .with_actual(value)
                        .error(ConfigErrorKind::InvalidValue);

                    return None;
                }
            }
        };

        if is_array {
            Some(Self::Array(Box::new(obj)))
        } else {
            Some(obj)
        }
    }

    pub fn terminal_type(&self) -> &Self {
        match self {
            Self::Array(inner) => inner.terminal_type(),
            _ => self,
        }
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_))
    }

    pub fn possible_values(&self) -> Option<Vec<String>> {
        match self.terminal_type() {
            Self::Enum(values) => Some(values.clone()),
            Self::Boolean => Some(vec!["true".to_string(), "false".to_string()]),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SyntaxGroup {
    pub name: String,
    pub parameters: Vec<String>,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub multiple: bool,
    #[serde(skip_serializing_if = "cache_utils::is_false")]
    pub required: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts_with: Vec<String>,
}

impl Default for SyntaxGroup {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            parameters: vec![],
            multiple: false,
            required: false,
            requires: vec![],
            conflicts_with: vec![],
        }
    }
}

impl SyntaxGroup {
    /// Create a vector of groups from a config value that can contain multiple groups.
    /// This supports the groups being specified as:
    ///
    /// ```yaml
    /// groups:
    ///  - name: group1
    ///    parameters:
    ///    - param1
    ///    - param2
    ///    multiple: true
    ///    required: true
    /// - name: group2
    ///   parameters: param3
    ///   requires: group1
    ///   conflicts_with: group3
    /// - group3:
    ///     parameters: param4
    /// ```
    ///
    /// Or as:
    ///
    /// ```yaml
    /// groups:
    ///   group1:
    ///     parameters:
    ///     - param1
    ///     - param2
    ///     multiple: true
    ///     required: true
    ///   group2:
    ///     parameters: param3
    ///     requires: group1
    ///     conflicts_with: group3
    ///   group3:
    ///     parameters: param4
    /// ```
    ///
    /// The ConfigValue object received is the contents of the `groups` key in the config file.
    pub(super) fn from_config_value_multi(
        config_value: &ConfigValue,
        error_handler: &ConfigErrorHandler,
    ) -> Vec<Self> {
        let mut groups = vec![];

        if let Some(array) = config_value.as_array() {
            // If this is an array, we can simply iterate over it and create the groups
            for (idx, value) in array.iter().enumerate() {
                if let Some(group) =
                    Self::from_config_value(value, None, &error_handler.with_index(idx))
                {
                    groups.push(group);
                }
            }
        } else if let Some(table) = config_value.as_table() {
            // If this is a table, we need to iterate over the keys and create the groups
            for (name, value) in table {
                if let Some(group) = Self::from_config_value(
                    &value,
                    Some(name.to_string()),
                    &error_handler.with_key(name),
                ) {
                    groups.push(group);
                }
            }
        } else {
            error_handler
                .with_expected("array or table")
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);
        }

        groups
    }

    pub(super) fn from_config_value(
        config_value: &ConfigValue,
        name: Option<String>,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
        // Exit early if the value is not a table
        let table = if let Some(table) = config_value.as_table() {
            // Exit early if the table is empty
            if table.is_empty() {
                error_handler
                    .with_key("name")
                    .error(ConfigErrorKind::MissingKey);
                error_handler
                    .with_key("parameters")
                    .error(ConfigErrorKind::MissingKey);
                return None;
            }
            table
        } else {
            error_handler
                .with_expected("table")
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);
            return None;
        };

        let mut config_value = config_value;
        let mut error_handler = error_handler.clone();

        // Handle the group name
        let name = match name {
            Some(name) => name,
            None => {
                if table.len() == 1 {
                    // Extract the only key from the table, this will be the name of the group
                    let key = table.keys().next().unwrap().to_string();

                    // Change the config to be the value of the key, this will be the group's config
                    config_value = table.get(&key)?;
                    error_handler = error_handler.with_key(&key);

                    // Exit early if the value is not a table
                    if !config_value.is_table() {
                        error_handler
                            .with_expected("table")
                            .with_actual(config_value)
                            .error(ConfigErrorKind::InvalidValueType);
                        return None;
                    }

                    // Return the key as the name of the group
                    key
                } else if let Some(name_config_value) = config_value.get("name") {
                    if let Some(name) = name_config_value.as_str_forced() {
                        name.to_string()
                    } else {
                        error_handler
                            .with_key("name")
                            .with_expected("string")
                            .with_actual(name_config_value)
                            .error(ConfigErrorKind::InvalidValueType);
                        return None;
                    }
                } else {
                    error_handler
                        .with_key("name")
                        .error(ConfigErrorKind::MissingKey);
                    return None;
                }
            }
        };

        // Handle the group parameters
        let parameters =
            config_value.get_as_str_array("parameters", &error_handler.with_key("parameters"));
        // No parameters, skip this group
        if parameters.is_empty() {
            error_handler
                .with_key("parameters")
                .error(ConfigErrorKind::MissingKey);
            return None;
        }

        // Parse the rest of the group configuration
        let multiple = config_value.get_as_bool_or_default(
            "multiple",
            false,
            &error_handler.with_key("multiple"),
        );

        let required = config_value.get_as_bool_or_default(
            "required",
            false,
            &error_handler.with_key("required"),
        );

        let requires =
            config_value.get_as_str_array("requires", &error_handler.with_key("requires"));

        let conflicts_with = config_value
            .get_as_str_array("conflicts_with", &error_handler.with_key("conflicts_with"));

        Some(Self {
            name,
            parameters,
            multiple,
            required,
            requires,
            conflicts_with,
        })
    }

    fn dest(&self) -> String {
        sanitize_str(&self.name)
    }

    fn add_to_argparser(&self, parser: clap::Command) -> clap::Command {
        let args = self
            .parameters
            .iter()
            .map(|param| sanitize_str(param))
            .collect::<Vec<String>>();

        let mut group = clap::ArgGroup::new(self.dest())
            .args(&args)
            .multiple(self.multiple)
            .required(self.required);

        // Set conflicts and requirements
        for require_arg in &self.requires {
            let require_arg = sanitize_str(require_arg);
            group = group.requires(&require_arg);
        }
        for conflict_arg in &self.conflicts_with {
            let conflict_arg = sanitize_str(conflict_arg);
            group = group.conflicts_with(&conflict_arg);
        }

        parser.group(group)
    }

    fn add_to_args(
        &self,
        args: &mut BTreeMap<String, ParseArgsValue>,
        matches: &clap::ArgMatches,
        parameters: &[SyntaxOptArg],
    ) -> Result<(), ParseArgsErrorKind> {
        let dest = self.dest();

        let param_id = match matches.get_one::<clap::Id>(&dest) {
            Some(param_id) => param_id.to_string(),
            None => return Ok(()),
        };

        let param = match parameters.iter().find(|param| *param.dest() == param_id) {
            Some(param) => param,
            None => return Ok(()),
        };

        param.add_to_args(args, matches, Some(dest.clone()))
    }
}

fn sanitize_str(s: &str) -> String {
    let mut prev_is_sanitized = false;
    let s = s
        .chars()
        // Replace all non-alphanumeric characters with _
        .flat_map(|c| {
            if c.is_alphanumeric() {
                prev_is_sanitized = false;
                Some(c)
            } else if !prev_is_sanitized {
                prev_is_sanitized = true;
                Some('_')
            } else {
                None
            }
        })
        .collect::<String>();

    s.trim_matches('_').to_string()
}

#[cfg(test)]
#[path = "command_definition_test.rs"]
mod tests;
