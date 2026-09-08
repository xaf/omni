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
use crate::internal::config::Level;
use crate::internal::config::OmniSource;
use crate::internal::config::Value as FeuilletageValue;
use crate::internal::user_interface::colors::StringColor;
use crate::internal::ORG_LOADER;

// Feuilletage type aliases for the YAML deserialization (uses concrete types)
type FeuilletageConfigValue = crate::internal::config::ContextValue;
type FeuilletageErrorTracker = feuilletage::ErrorTracker;

#[derive(Debug, Serialize, Clone, feuilletage::Config)]
#[feuilletage(skip_serialize)]
pub struct CommandDefinition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    pub run: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[feuilletage(default, allow_single)]
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
    #[feuilletage(default = "false")]
    pub argparser: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[feuilletage(default)]
    pub tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "cache_utils::is_false")]
    #[feuilletage(default = "false")]
    pub export: bool,
    #[serde(skip)]
    #[feuilletage(from_context_fn = "command_def_source_from_context")]
    pub source: OmniSource,
    #[serde(skip)]
    #[feuilletage(from_context_fn = "command_def_scope_from_context")]
    pub scope: Level,
}

#[derive(Debug, Serialize, Clone, PartialEq, Default, feuilletage::Config)]
#[feuilletage(parse_as = "CommandSyntaxWire", skip_serialize, skip_deserialize)]
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
        _error_handler: &ConfigErrorHandler,
    ) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;
        // Convert serde_yaml::Value to feuilletage ConfigValue
        let feuilletage_value = yaml_value_to_feuilletage_value(value);
        let mut tracker = FeuilletageErrorTracker::new();
        <Self as feuilletage::FromContextValue>::from_context_value(
            &feuilletage_value,
            &mut tracker,
        )
        .map_err(|_| serde::de::Error::custom("invalid command syntax"))
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

#[derive(Debug, Serialize, Clone, PartialEq)]
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

#[derive(Debug, Serialize, Clone, PartialEq, Copy)]
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

#[derive(Debug, Serialize, Clone, PartialEq, Default)]
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

#[cfg(test)]
#[path = "command_definition_test.rs"]
mod tests;

#[derive(Debug, Serialize, Clone, PartialEq)]
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

// ============================================================================
// Helper functions for feuilletage derive macro (from_context_fn)
// ============================================================================

fn command_def_source_from_context<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    ctx: &feuilletage::Context<S, L>,
) -> OmniSource {
    match ctx.source.file_path() {
        Some(p) => OmniSource::File(p.to_path_buf()),
        None => OmniSource::Default,
    }
}

fn command_def_scope_from_context<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    ctx: &feuilletage::Context<S, L>,
) -> Level {
    match ctx.level.name() {
        "system" => Level::System,
        "user" => Level::User,
        _ => Level::Local,
    }
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(
    scalar_as = "usage",
    array_as = "parameters",
    skip_serialize,
    skip_deserialize
)]
struct CommandSyntaxWire {
    #[feuilletage(default)]
    usage: Option<StringOrIntWire>,
    #[feuilletage(default, allow_single, allow_map)]
    parameters: Vec<SyntaxOptArgWire>,
    #[feuilletage(default, allow_single, allow_map)]
    arguments: Vec<SyntaxOptArgWire>,
    #[feuilletage(default, allow_single, allow_map)]
    argument: Vec<SyntaxOptArgWire>,
    #[feuilletage(default, allow_single, allow_map)]
    options: Vec<SyntaxOptArgWire>,
    #[feuilletage(default, allow_single, allow_map)]
    option: Vec<SyntaxOptArgWire>,
    #[feuilletage(default, allow_single, allow_map)]
    optional: Vec<SyntaxOptArgWire>,
    #[feuilletage(default, allow_map)]
    groups: Vec<SyntaxGroupWire>,
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(
    transform = "self::normalize_syntax_opt_arg_shape",
    scalar_as = "name",
    allow_map(key = "name", scalar_as = "desc"),
    post_process = "normalize_syntax_opt_arg_wire",
    skip_serialize,
    skip_deserialize
)]
struct SyntaxOptArgWire {
    name: StrictStringWire,
    #[feuilletage(default)]
    dest: Option<String>,
    #[feuilletage(default)]
    desc: Option<String>,
    #[feuilletage(default)]
    required: Option<StrictBoolWire>,
    #[feuilletage(default, allow_single)]
    placeholders: Vec<String>,
    #[feuilletage(default, allow_single)]
    placeholder: Vec<String>,
    #[feuilletage(default, rename = "type")]
    arg_type: Option<SyntaxOptArgTypeWire>,
    #[feuilletage(default)]
    values: Option<SyntaxStringListWire>,
    #[feuilletage(default)]
    default: Option<String>,
    #[feuilletage(default)]
    default_missing_value: Option<String>,
    #[feuilletage(default)]
    num_values: Option<StringOrIntWire>,
    #[feuilletage(default, rename = "delimiter")]
    value_delimiter: Option<String>,
    #[feuilletage(default, rename = "last")]
    last_arg_double_hyphen: StrictBoolWire,
    #[feuilletage(default)]
    leftovers: Option<StrictBoolWire>,
    #[feuilletage(default)]
    allow_hyphen_values: StrictBoolWire,
    #[feuilletage(default)]
    allow_negative_numbers: StrictBoolWire,
    #[feuilletage(default)]
    group_occurrences: StrictBoolWire,
    #[feuilletage(default, allow_single)]
    requires: Vec<String>,
    #[feuilletage(default, allow_single)]
    conflicts_with: Vec<String>,
    #[feuilletage(default, allow_single)]
    required_without: Vec<String>,
    #[feuilletage(default, allow_single)]
    required_without_all: Vec<String>,
    #[feuilletage(default)]
    required_if_eq: HashMap<String, String>,
    #[feuilletage(default)]
    required_if_eq_all: HashMap<String, String>,
    #[feuilletage(default, allow_single)]
    aliases: Vec<String>,
    #[feuilletage(default, rename = "__syntax_details_object")]
    details_object: bool,
    #[feuilletage(skip)]
    parsed_arg_type: SyntaxOptArgType,
    #[feuilletage(skip)]
    parsed_num_values: Option<SyntaxOptArgNumValues>,
    #[feuilletage(skip)]
    parsed_value_delimiter: Option<char>,
}

fn normalize_syntax_opt_arg_shape<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &mut feuilletage::ContextValue<S, L>,
    _context: &feuilletage::Context<S, L>,
) -> Result<(), feuilletage::Error> {
    let feuilletage::ContextValue::Object(fields, object_context) = value else {
        return Ok(());
    };
    let object_context = object_context.clone();

    if fields.contains_key("name") {
        let context = fields
            .values()
            .next()
            .expect("an object containing name is non-empty")
            .context()
            .clone();
        fields.insert(
            "__syntax_details_object".to_string(),
            feuilletage::ContextValue::bool(true, context),
        );
        return Ok(());
    }

    if fields.len() != 1 {
        return Ok(());
    }

    let (_, details) = fields.iter_mut().next().expect("single-entry object");
    match details {
        feuilletage::ContextValue::Object(details, _) => {
            let context = details
                .values()
                .next()
                .map(feuilletage::ContextValue::context)
                .cloned()
                .unwrap_or_else(|| object_context.clone());
            details.insert(
                "__syntax_details_object".to_string(),
                feuilletage::ContextValue::bool(true, context),
            );
        }
        feuilletage::ContextValue::Int(_, context)
        | feuilletage::ContextValue::Float(_, context)
        | feuilletage::ContextValue::Bool(_, context) => {
            *details = feuilletage::ContextValue::null(context.clone());
        }
        _ => {}
    }

    Ok(())
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(untagged, skip_serialize, skip_deserialize)]
enum StrictStringWire {
    #[feuilletage(variant = any_string)]
    Value(String),
    #[feuilletage(variant = any_bool)]
    InvalidBool,
    #[feuilletage(variant = any_int)]
    InvalidInt,
    #[feuilletage(variant = any_float)]
    InvalidFloat,
    #[feuilletage(variant = predicate("syntax_value_is_array"))]
    InvalidArray,
    #[feuilletage(variant = predicate("syntax_value_is_object"))]
    InvalidObject,
    #[feuilletage(variant = null)]
    InvalidNull,
}

impl StrictStringWire {
    fn into_string(
        self,
        tracker: &feuilletage::ErrorTracker,
    ) -> Result<String, feuilletage::Error> {
        match self {
            Self::Value(value) => Ok(value),
            Self::InvalidBool => Err(syntax_string_type_mismatch(tracker, "bool")),
            Self::InvalidInt => Err(syntax_string_type_mismatch(tracker, "int")),
            Self::InvalidFloat => Err(syntax_string_type_mismatch(tracker, "float")),
            Self::InvalidArray => Err(syntax_string_type_mismatch(tracker, "array")),
            Self::InvalidObject => Err(syntax_string_type_mismatch(tracker, "object")),
            Self::InvalidNull => Err(syntax_string_type_mismatch(tracker, "null")),
        }
    }
}

fn syntax_string_type_mismatch(
    tracker: &feuilletage::ErrorTracker,
    actual: &str,
) -> feuilletage::Error {
    feuilletage::Error::TypeMismatch {
        path: tracker.current_path(),
        expected: "string".to_string(),
        actual: actual.to_string(),
    }
}

fn syntax_value_is_array<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &feuilletage::ContextValue<S, L>,
) -> bool {
    matches!(value, feuilletage::ContextValue::Array(_, _))
}

fn syntax_value_is_object<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    value: &feuilletage::ContextValue<S, L>,
) -> bool {
    matches!(value, feuilletage::ContextValue::Object(_, _))
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(
    transparent,
    post_process = "validate_strict_bool_wire",
    skip_serialize,
    skip_deserialize
)]
struct StrictBoolWire(FeuilletageValue);

impl Default for StrictBoolWire {
    fn default() -> Self {
        Self(FeuilletageValue::Bool(false))
    }
}

impl StrictBoolWire {
    fn value(&self) -> bool {
        match &self.0 {
            FeuilletageValue::Bool(value) => *value,
            _ => false,
        }
    }
}

fn validate_strict_bool_wire<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    _parsed: &mut StrictBoolWire,
    original: &feuilletage::ContextValue<S, L>,
    tracker: &mut feuilletage::ErrorTracker,
) -> Result<(), feuilletage::Error> {
    if matches!(original, feuilletage::ContextValue::Bool(_, _)) {
        Ok(())
    } else {
        Err(feuilletage::Error::TypeMismatch {
            path: tracker.current_path(),
            expected: "boolean".to_string(),
            actual: original.type_name().to_string(),
        })
    }
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(
    transparent,
    post_process = "validate_string_or_int_wire",
    skip_serialize,
    skip_deserialize
)]
struct StringOrIntWire(String);

fn validate_string_or_int_wire<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    _parsed: &mut StringOrIntWire,
    original: &feuilletage::ContextValue<S, L>,
    tracker: &mut feuilletage::ErrorTracker,
) -> Result<(), feuilletage::Error> {
    if matches!(
        original,
        feuilletage::ContextValue::String(_, _) | feuilletage::ContextValue::Int(_, _)
    ) {
        Ok(())
    } else {
        Err(feuilletage::Error::TypeMismatch {
            path: tracker.current_path(),
            expected: "string".to_string(),
            actual: original.type_name().to_string(),
        })
    }
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(untagged, skip_serialize, skip_deserialize)]
enum SyntaxOptArgTypeWire {
    Values(Vec<String>),
    Name(StringOrIntWire),
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(untagged, skip_serialize, skip_deserialize)]
enum SyntaxStringListWire {
    Values(Vec<String>),
    #[feuilletage(variant = any_string)]
    Value(String),
}

impl SyntaxStringListWire {
    fn into_vec(self, delimiter: Option<char>) -> Vec<String> {
        match self {
            Self::Values(values) => values,
            Self::Value(value) => delimiter
                .map(|delimiter| value.split(delimiter).map(str::to_string).collect())
                .unwrap_or_else(|| vec![value]),
        }
    }
}

#[derive(Debug, feuilletage::Config)]
#[feuilletage(allow_map(key = "name"), skip_serialize, skip_deserialize)]
struct SyntaxGroupWire {
    name: StringOrIntWire,
    #[feuilletage(default, allow_single)]
    parameters: Vec<String>,
    #[feuilletage(default)]
    multiple: StrictBoolWire,
    #[feuilletage(default)]
    required: StrictBoolWire,
    #[feuilletage(default, allow_single)]
    requires: Vec<String>,
    #[feuilletage(default, allow_single)]
    conflicts_with: Vec<String>,
}

fn normalize_syntax_opt_arg_wire<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>(
    parsed: &mut SyntaxOptArgWire,
    _original: &feuilletage::ContextValue<S, L>,
    tracker: &mut feuilletage::ErrorTracker,
) -> Result<(), feuilletage::Error> {
    tracker.push_field("name");
    let name =
        std::mem::replace(&mut parsed.name, StrictStringWire::InvalidNull).into_string(tracker);
    tracker.pop();
    parsed.name = StrictStringWire::Value(name?);

    parsed.parsed_value_delimiter = parsed.value_delimiter.as_ref().and_then(|delimiter| {
        delimiter.chars().next().or_else(|| {
            tracker.push_field("delimiter");
            tracker.record(feuilletage::Error::InvalidValue {
                path: tracker.current_path(),
                message: "delimiter must be non-empty".to_string(),
            });
            tracker.pop();
            None
        })
    });

    parsed.parsed_arg_type = match parsed.arg_type.as_ref() {
        Some(SyntaxOptArgTypeWire::Values(values)) => SyntaxOptArgType::Enum(values.clone()),
        Some(SyntaxOptArgTypeWire::Name(value)) => {
            tracker.push_field("type");
            let arg_type = SyntaxOptArgType::from_str_feuilletage(&value.0, tracker);
            tracker.pop();
            match arg_type {
                Some(SyntaxOptArgType::Enum(values)) if values.is_empty() => parsed
                    .values
                    .take()
                    .map(|values| {
                        SyntaxOptArgType::Enum(values.into_vec(parsed.parsed_value_delimiter))
                    })
                    .unwrap_or(SyntaxOptArgType::String),
                Some(arg_type) => arg_type,
                None => SyntaxOptArgType::String,
            }
        }
        None => SyntaxOptArgType::String,
    };

    parsed.parsed_num_values = parsed.num_values.as_ref().and_then(|value| {
        tracker.push_field("num_values");
        let num_values = SyntaxOptArgNumValues::from_str_feuilletage(&value.0, tracker);
        tracker.pop();
        num_values
    });

    Ok(())
}

impl SyntaxOptArgWire {
    fn into_domain(
        self,
        required_default: Option<bool>,
        _tracker: &mut feuilletage::ErrorTracker,
    ) -> SyntaxOptArg {
        let (mut names, inferred_type, inferred_placeholders, inferred_leftovers) =
            parse_arg_name(match &self.name {
                StrictStringWire::Value(name) => name,
                _ => unreachable!("parameter name was validated during wire parsing"),
            });
        names.extend(self.aliases);

        let placeholders = if self.placeholders.is_empty() {
            if self.placeholder.is_empty() {
                inferred_placeholders
            } else {
                self.placeholder
            }
        } else {
            self.placeholders
        };

        let arg_type = self
            .arg_type
            .map_or(inferred_type, |_| self.parsed_arg_type);

        SyntaxOptArg {
            names,
            dest: self.dest,
            desc: self.desc,
            required: self
                .required
                .map(|required| required.value())
                .or(required_default)
                .unwrap_or(false),
            placeholders,
            arg_type,
            default: self.default,
            default_missing_value: self.default_missing_value,
            num_values: self.parsed_num_values,
            value_delimiter: self.parsed_value_delimiter,
            last_arg_double_hyphen: self.last_arg_double_hyphen.value(),
            leftovers: self
                .leftovers
                .map(|leftovers| leftovers.value())
                .unwrap_or_else(|| {
                    if self.details_object {
                        false
                    } else {
                        inferred_leftovers
                    }
                }),
            allow_hyphen_values: self.allow_hyphen_values.value(),
            allow_negative_numbers: self.allow_negative_numbers.value(),
            group_occurrences: self.group_occurrences.value(),
            requires: self.requires,
            conflicts_with: self.conflicts_with,
            required_without: self.required_without,
            required_without_all: self.required_without_all,
            required_if_eq: self.required_if_eq,
            required_if_eq_all: self.required_if_eq_all,
        }
    }
}

impl SyntaxGroupWire {
    fn into_domain(self, tracker: &mut feuilletage::ErrorTracker) -> Option<SyntaxGroup> {
        if self.parameters.is_empty() {
            tracker.push_field("parameters");
            tracker.record(feuilletage::Error::MissingField {
                path: tracker.current_path(),
            });
            tracker.pop();
            return None;
        }

        Some(SyntaxGroup {
            name: self.name.0,
            parameters: self.parameters,
            multiple: self.multiple.value(),
            required: self.required.value(),
            requires: self.requires,
            conflicts_with: self.conflicts_with,
        })
    }
}

impl<S: feuilletage::CustomSource, L: feuilletage::CustomLevel>
    feuilletage::FromParsed<CommandSyntaxWire, S, L> for CommandSyntax
{
    fn from_parsed(
        parsed: CommandSyntaxWire,
        original: &feuilletage::ContextValue<S, L>,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::Error> {
        let mut parameters = Vec::new();
        parameters.extend(
            parsed
                .parameters
                .into_iter()
                .map(|parameter| parameter.into_domain(None, tracker)),
        );
        parameters.extend(
            parsed
                .arguments
                .into_iter()
                .map(|parameter| parameter.into_domain(Some(true), tracker)),
        );
        parameters.extend(
            parsed
                .argument
                .into_iter()
                .map(|parameter| parameter.into_domain(Some(true), tracker)),
        );
        parameters.extend(
            parsed
                .options
                .into_iter()
                .map(|parameter| parameter.into_domain(Some(false), tracker)),
        );
        parameters.extend(
            parsed
                .option
                .into_iter()
                .map(|parameter| parameter.into_domain(Some(false), tracker)),
        );
        parameters.extend(
            parsed
                .optional
                .into_iter()
                .map(|parameter| parameter.into_domain(Some(false), tracker)),
        );

        let mut groups = Vec::new();
        tracker.push_field("groups");
        for (index, group) in parsed.groups.into_iter().enumerate() {
            tracker.push_index(index);
            if let Some(group) = group.into_domain(tracker) {
                groups.push(group);
            }
            tracker.pop();
        }
        tracker.pop();

        if parameters.is_empty() && groups.is_empty() && parsed.usage.is_none() {
            Err(feuilletage::Error::TypeMismatch {
                path: tracker.current_path(),
                expected: "command syntax (array or object)".to_string(),
                actual: original.type_name().to_string(),
            })
        } else {
            Ok(Self {
                usage: parsed.usage.map(|usage| usage.0),
                parameters,
                groups,
            })
        }
    }
}

// ============================================================================
// Feuilletage helper functions for parsing
// ============================================================================

/// Convert a serde_yaml::Value to feuilletage ConfigValue
fn yaml_value_to_feuilletage_value(value: serde_yaml::Value) -> FeuilletageConfigValue {
    let context =
        feuilletage::Context::new(feuilletage::Source::Default, feuilletage::Level::System);
    match value {
        serde_yaml::Value::Null => FeuilletageConfigValue::null(context),
        serde_yaml::Value::Bool(b) => FeuilletageConfigValue::bool(b, context),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                FeuilletageConfigValue::int(i, context)
            } else if let Some(f) = n.as_f64() {
                FeuilletageConfigValue::float(f, context)
            } else {
                FeuilletageConfigValue::null(context)
            }
        }
        serde_yaml::Value::String(s) => FeuilletageConfigValue::string(s, context),
        serde_yaml::Value::Sequence(seq) => {
            let arr: Vec<FeuilletageConfigValue> = seq
                .into_iter()
                .map(yaml_value_to_feuilletage_value)
                .collect();
            FeuilletageConfigValue::array(arr, context)
        }
        serde_yaml::Value::Mapping(map) => {
            let obj: indexmap::IndexMap<String, FeuilletageConfigValue> = map
                .into_iter()
                .filter_map(|(k, v)| {
                    let key = match k {
                        serde_yaml::Value::String(s) => s,
                        _ => return None,
                    };
                    Some((key, yaml_value_to_feuilletage_value(v)))
                })
                .collect();
            FeuilletageConfigValue::object(obj, context)
        }
        serde_yaml::Value::Tagged(_) => FeuilletageConfigValue::null(context),
    }
}

// ============================================================================
// Feuilletage parsing for SyntaxOptArgNumValues
// ============================================================================

impl SyntaxOptArgNumValues {
    fn from_str_feuilletage(value: &str, tracker: &mut feuilletage::ErrorTracker) -> Option<Self> {
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
                        tracker.record(feuilletage::Error::InvalidValue {
                            path: tracker.current_path(),
                            message: format!("expected positive integer, got '{}'", value),
                        });
                        return None;
                    }
                },
            };

            let min = match min {
                "" => None,
                value => match value.parse::<usize>() {
                    Ok(value) => Some(value),
                    Err(_) => {
                        tracker.record(feuilletage::Error::InvalidValue {
                            path: tracker.current_path(),
                            message: format!("expected positive integer, got '{}'", value),
                        });
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
                        tracker.record(feuilletage::Error::InvalidValue {
                            path: tracker.current_path(),
                            message: "invalid range: min 0 max 0".to_string(),
                        });
                        None
                    }
                }
                (Some(min), None, _) => Some(Self::AtLeast(min)),
                (Some(min), Some(max), true) => {
                    if min <= max {
                        Some(Self::Between(min, max))
                    } else {
                        tracker.record(feuilletage::Error::InvalidValue {
                            path: tracker.current_path(),
                            message: format!("invalid range: min {} > max {}", min, max),
                        });
                        None
                    }
                }
                (Some(min), Some(max), false) => {
                    if min < max {
                        Some(Self::Between(min, max - 1))
                    } else {
                        tracker.record(feuilletage::Error::InvalidValue {
                            path: tracker.current_path(),
                            message: format!("invalid range: min {} >= max {}", min, max),
                        });
                        None
                    }
                }
            }
        } else {
            match value.parse::<usize>() {
                Ok(value) => Some(Self::Exactly(value)),
                Err(_) => {
                    tracker.record(feuilletage::Error::InvalidValue {
                        path: tracker.current_path(),
                        message: format!("expected positive integer, got '{}'", value),
                    });
                    None
                }
            }
        }
    }
}

// ============================================================================
// Feuilletage parsing for SyntaxOptArgType
// ============================================================================

impl SyntaxOptArgType {
    fn from_str_feuilletage(value: &str, tracker: &mut feuilletage::ErrorTracker) -> Option<Self> {
        let mut is_array = false;

        let normalized = value.trim().to_lowercase();
        let mut value = normalized.trim();

        if value.starts_with("array/") {
            value = &value[6..];
            is_array = true;
        } else if value.starts_with('[') && value.ends_with(']') {
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
                // Check for enum formats like enum(xx, yy) or (xx, yy)
                let mut enum_contents = None;

                if value.starts_with("enum(") && value.ends_with(')') {
                    enum_contents = Some(&value[5..value.len() - 1]);
                } else if value.starts_with('(') && value.ends_with(')') {
                    enum_contents = Some(&value[1..value.len() - 1]);
                }

                if let Some(contents) = enum_contents {
                    let values = contents
                        .split(',')
                        .map(|v| v.trim().to_string())
                        .filter(|v| !v.is_empty())
                        .collect::<Vec<String>>();
                    Self::Enum(values)
                } else {
                    tracker.record(feuilletage::Error::InvalidValue {
                        path: tracker.current_path(),
                        message: format!(
                            "invalid type '{}', expected one of: int, float, bool, flag, count, str, path, enum, array/<type>",
                            value
                        ),
                    });
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
}
