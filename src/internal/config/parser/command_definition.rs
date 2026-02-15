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
use crate::internal::user_interface::colors::StringColor;
use crate::internal::ORG_LOADER;

// Compote type aliases for the YAML deserialization (uses concrete types)
type CompoteConfigValue = crate::internal::config::ContextValue;
type CompoteErrorTracker = compote::ErrorTracker;

#[derive(Debug, Serialize, Clone)]
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
    pub source: OmniSource,
    #[serde(skip)]
    pub scope: Level,
}


#[derive(Debug, Serialize, Clone, PartialEq, Default)]
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
        // Convert serde_yaml::Value to compote ConfigValue
        let compote_value = yaml_value_to_compote_value(value);
        let mut tracker = CompoteErrorTracker::new();
        if let Some(command_syntax) =
            CommandSyntax::from_compote_config_value(&compote_value, &mut tracker)
        {
            Ok(command_syntax)
        } else {
            Err(serde::de::Error::custom("invalid command syntax"))
        }
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
// CANNOT CONVERT TO DERIVE MACRO - TECHNICAL LIMITATIONS
// ============================================================================
//
// CommandDefinition requires manual FromContextValue because:
//
// 1. **Context metadata injection**: The `source` and `scope` fields are
//    populated from `value.context().source.file_path()` and
//    `value.context().level.name()`. These fields don't come from the
//    config value itself - they're metadata about WHERE the value came from.
//    Compote's derive macro only extracts data from the value, not context.
//
// 2. **Complex nested parsing**: The `syntax` field uses custom parsing
//    through `CommandSyntax::from_compote_config_value()` with special
//    logic for multiple input formats (array, object with various keys,
//    or string). This is not a simple field extraction.
//
// 3. **HashMap<String, CommandDefinition> recursive parsing**: The
//    `subcommands` field needs recursive parsing where each subcommand
//    inherits context information.
//
// To convert this, compote would need:
// - A `#[compote(from_context_source)]` and `#[compote(from_context_level)]`
//   attribute to inject context metadata into struct fields
// - These fields are marked `#[serde(skip)]` precisely because they're
//   not part of the serialized config - they're runtime metadata
// ============================================================================

impl<S: compote::CustomSource, L: compote::CustomLevel> compote::FromContextValue<S, L> for CommandDefinition {
    fn from_context_value(
        value: &compote::ContextValue<S, L>,
        tracker: &mut compote::ErrorTracker,
    ) -> Result<Self, compote::Error> {
        let table = match value {
            compote::ContextValue::Object(map, _) => map,
            compote::ContextValue::Null(_) => {
                return Err(compote::Error::TypeMismatch {
                    path: tracker.current_path(),
                    expected: "object".to_string(),
                    actual: "null".to_string(),
                });
            }
            _ => {
                return Err(compote::Error::TypeMismatch {
                    path: tracker.current_path(),
                    expected: "object".to_string(),
                    actual: value.type_name().to_string(),
                });
            }
        };

        // Parse desc
        let desc = compote_get_str_or_none(table, "desc", tracker);

        // Parse run (required)
        let run = compote_get_str_or_none(table, "run", tracker).unwrap_or_else(|| {
            tracker.push_field("run");
            tracker.record(compote::Error::MissingField {
                path: tracker.current_path(),
            });
            tracker.pop();
            "true".to_string()
        });

        // Parse aliases
        let aliases = compote_get_str_array(table, "aliases", tracker);

        // Parse syntax
        let syntax = if let Some(syntax_value) = table.get("syntax") {
            tracker.push_field("syntax");
            let result = CommandSyntax::from_compote_config_value(syntax_value, tracker);
            tracker.pop();
            result
        } else {
            None
        };

        // Parse tags
        let tags = if let Some(tags_value) = table.get("tags") {
            tracker.push_field("tags");
            let result = compote_parse_string_map(tags_value, tracker);
            tracker.pop();
            result
        } else {
            BTreeMap::new()
        };

        // Parse category
        let category = {
            let cat = compote_get_str_array(table, "category", tracker);
            if cat.is_empty() {
                None
            } else {
                Some(cat)
            }
        };

        // Parse dir
        let dir = compote_get_str_or_none(table, "dir", tracker);

        // Parse subcommands (recursive)
        let subcommands = if let Some(sub_value) = table.get("subcommands") {
            tracker.push_field("subcommands");
            let result = match sub_value {
                compote::ContextValue::Object(sub_table, _) => {
                    let mut subs = HashMap::new();
                    for (key, sub_def) in sub_table {
                        tracker.push_field(key);
                        match <CommandDefinition as compote::FromContextValue<S, L>>::from_context_value(sub_def, tracker) {
                            Ok(cmd_def) => {
                                subs.insert(key.clone(), cmd_def);
                            }
                            Err(e) => tracker.record(e),
                        }
                        tracker.pop();
                    }
                    Some(subs)
                }
                compote::ContextValue::Null(_) => None,
                _ => {
                    tracker.record(compote::Error::TypeMismatch {
                        path: tracker.current_path(),
                        expected: "object".to_string(),
                        actual: sub_value.type_name().to_string(),
                    });
                    None
                }
            };
            tracker.pop();
            result
        } else {
            None
        };

        // Parse argparser
        let argparser = compote_get_bool_or_default(table, "argparser", false, tracker);

        // Parse export
        let export = compote_get_bool_or_default(table, "export", false, tracker);

        // Convert source using CustomSource trait method
        let source = match value.context().source.file_path() {
            Some(path) => OmniSource::File(path.to_path_buf()),
            None => OmniSource::Default,
        };

        // Convert scope/level from generic CustomLevel to concrete Level
        let scope = match value.context().level.name() {
            "system" => Level::System,
            "user" => Level::User,
            _ => Level::Local, // Default to local for unknown levels
        };

        Ok(Self {
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
            source,
            scope,
        })
    }
}

// ============================================================================
// Compote helper functions for parsing
// ============================================================================

/// Convert a serde_yaml::Value to compote ConfigValue
fn yaml_value_to_compote_value(value: serde_yaml::Value) -> CompoteConfigValue {
    let context = compote::Context::new(compote::Source::Default, compote::Level::System);
    match value {
        serde_yaml::Value::Null => CompoteConfigValue::null(context),
        serde_yaml::Value::Bool(b) => CompoteConfigValue::bool(b, context),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                CompoteConfigValue::int(i, context)
            } else if let Some(f) = n.as_f64() {
                CompoteConfigValue::float(f, context)
            } else {
                CompoteConfigValue::null(context)
            }
        }
        serde_yaml::Value::String(s) => CompoteConfigValue::string(s, context),
        serde_yaml::Value::Sequence(seq) => {
            let arr: Vec<CompoteConfigValue> = seq.into_iter().map(yaml_value_to_compote_value).collect();
            CompoteConfigValue::array(arr, context)
        }
        serde_yaml::Value::Mapping(map) => {
            let obj: indexmap::IndexMap<String, CompoteConfigValue> = map
                .into_iter()
                .filter_map(|(k, v)| {
                    let key = match k {
                        serde_yaml::Value::String(s) => s,
                        _ => return None,
                    };
                    Some((key, yaml_value_to_compote_value(v)))
                })
                .collect();
            CompoteConfigValue::object(obj, context)
        }
        serde_yaml::Value::Tagged(_) => CompoteConfigValue::null(context),
    }
}

/// Get a string value from a compote object, returning None if not present
fn compote_get_str_or_none<S: compote::CustomSource, L: compote::CustomLevel>(
    table: &indexmap::IndexMap<String, compote::ContextValue<S, L>>,
    key: &str,
    tracker: &mut compote::ErrorTracker,
) -> Option<String> {
    let value = table.get(key)?;
    match value {
        compote::ContextValue::String(s, _) => Some(s.clone()),
        compote::ContextValue::Int(i, _) => Some(i.to_string()),
        compote::ContextValue::Float(f, _) => Some(f.to_string()),
        compote::ContextValue::Bool(b, _) => Some(b.to_string()),
        compote::ContextValue::Null(_) => None,
        _ => {
            tracker.push_field(key);
            tracker.record(compote::Error::TypeMismatch {
                path: tracker.current_path(),
                expected: "string".to_string(),
                actual: value.type_name().to_string(),
            });
            tracker.pop();
            None
        }
    }
}

/// Get a boolean value from a compote object with a default
fn compote_get_bool_or_default<S: compote::CustomSource, L: compote::CustomLevel>(
    table: &indexmap::IndexMap<String, compote::ContextValue<S, L>>,
    key: &str,
    default: bool,
    tracker: &mut compote::ErrorTracker,
) -> bool {
    let Some(value) = table.get(key) else {
        return default;
    };
    match value {
        compote::ContextValue::Bool(b, _) => *b,
        compote::ContextValue::Null(_) => default,
        _ => {
            tracker.push_field(key);
            tracker.record(compote::Error::TypeMismatch {
                path: tracker.current_path(),
                expected: "boolean".to_string(),
                actual: value.type_name().to_string(),
            });
            tracker.pop();
            default
        }
    }
}

/// Get an array of strings from a compote object
fn compote_get_str_array<S: compote::CustomSource, L: compote::CustomLevel>(
    table: &indexmap::IndexMap<String, compote::ContextValue<S, L>>,
    key: &str,
    tracker: &mut compote::ErrorTracker,
) -> Vec<String> {
    let Some(value) = table.get(key) else {
        return Vec::new();
    };

    tracker.push_field(key);
    let result = compote_value_to_str_array(value, tracker);
    tracker.pop();
    result
}

/// Convert a compote value to an array of strings
fn compote_value_to_str_array<S: compote::CustomSource, L: compote::CustomLevel>(
    value: &compote::ContextValue<S, L>,
    tracker: &mut compote::ErrorTracker,
) -> Vec<String> {
    match value {
        compote::ContextValue::Array(arr, _) => {
            let mut result = Vec::new();
            for (idx, item) in arr.iter().enumerate() {
                match item {
                    compote::ContextValue::String(s, _) => result.push(s.clone()),
                    compote::ContextValue::Int(i, _) => result.push(i.to_string()),
                    compote::ContextValue::Float(f, _) => result.push(f.to_string()),
                    compote::ContextValue::Bool(b, _) => result.push(b.to_string()),
                    _ => {
                        tracker.push_index(idx);
                        tracker.record(compote::Error::TypeMismatch {
                            path: tracker.current_path(),
                            expected: "string".to_string(),
                            actual: item.type_name().to_string(),
                        });
                        tracker.pop();
                    }
                }
            }
            result
        }
        compote::ContextValue::String(s, _) => vec![s.clone()],
        compote::ContextValue::Null(_) => Vec::new(),
        _ => {
            tracker.record(compote::Error::TypeMismatch {
                path: tracker.current_path(),
                expected: "array or string".to_string(),
                actual: value.type_name().to_string(),
            });
            Vec::new()
        }
    }
}

/// Parse a string map from a compote value
fn compote_parse_string_map<S: compote::CustomSource, L: compote::CustomLevel>(
    value: &compote::ContextValue<S, L>,
    tracker: &mut compote::ErrorTracker,
) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    match value {
        compote::ContextValue::Object(map, _) => {
            for (key, val) in map {
                match val {
                    compote::ContextValue::String(s, _) => {
                        result.insert(key.clone(), s.clone());
                    }
                    compote::ContextValue::Int(i, _) => {
                        result.insert(key.clone(), i.to_string());
                    }
                    compote::ContextValue::Float(f, _) => {
                        result.insert(key.clone(), f.to_string());
                    }
                    compote::ContextValue::Bool(b, _) => {
                        result.insert(key.clone(), b.to_string());
                    }
                    _ => {
                        tracker.push_field(key);
                        tracker.record(compote::Error::TypeMismatch {
                            path: tracker.current_path(),
                            expected: "string".to_string(),
                            actual: val.type_name().to_string(),
                        });
                        tracker.pop();
                    }
                }
            }
        }
        compote::ContextValue::Null(_) => {}
        _ => {
            tracker.record(compote::Error::TypeMismatch {
                path: tracker.current_path(),
                expected: "object".to_string(),
                actual: value.type_name().to_string(),
            });
        }
    }
    result
}

// ============================================================================
// Compote parsing for CommandSyntax
// ============================================================================

impl CommandSyntax {
    fn from_compote_config_value<S: compote::CustomSource, L: compote::CustomLevel>(
        value: &compote::ContextValue<S, L>,
        tracker: &mut compote::ErrorTracker,
    ) -> Option<Self> {
        let mut usage = None;
        let mut parameters = vec![];
        let mut groups = vec![];

        match value {
            compote::ContextValue::Array(array, _) => {
                for (idx, item) in array.iter().enumerate() {
                    tracker.push_index(idx);
                    if let Some(param) =
                        SyntaxOptArg::from_compote_config_value(item, None, tracker)
                    {
                        parameters.push(param);
                    }
                    tracker.pop();
                }
            }
            compote::ContextValue::Object(table, _) => {
                let keys = [
                    ("parameters", None),
                    ("arguments", Some(true)),
                    ("argument", Some(true)),
                    ("options", Some(false)),
                    ("option", Some(false)),
                    ("optional", Some(false)),
                ];

                for (key, required) in keys {
                    if let Some(param_value) = table.get(key) {
                        tracker.push_field(key);
                        match param_value {
                            compote::ContextValue::Array(arr, _) => {
                                for (idx, item) in arr.iter().enumerate() {
                                    tracker.push_index(idx);
                                    if let Some(param) = SyntaxOptArg::from_compote_config_value(
                                        item, required, tracker,
                                    ) {
                                        parameters.push(param);
                                    }
                                    tracker.pop();
                                }
                            }
                            _ => {
                                if let Some(param) = SyntaxOptArg::from_compote_config_value(
                                    param_value,
                                    required,
                                    tracker,
                                ) {
                                    parameters.push(param);
                                }
                            }
                        }
                        tracker.pop();
                    }
                }

                if let Some(groups_value) = table.get("groups") {
                    tracker.push_field("groups");
                    groups = SyntaxGroup::from_compote_config_value_multi(groups_value, tracker);
                    tracker.pop();
                }

                if let Some(usage_value) = table.get("usage") {
                    tracker.push_field("usage");
                    match usage_value {
                        compote::ContextValue::String(s, _) => usage = Some(s.clone()),
                        compote::ContextValue::Int(i, _) => usage = Some(i.to_string()),
                        _ => {
                            tracker.record(compote::Error::TypeMismatch {
                                path: tracker.current_path(),
                                expected: "string".to_string(),
                                actual: usage_value.type_name().to_string(),
                            });
                        }
                    }
                    tracker.pop();
                }
            }
            compote::ContextValue::String(s, _) => {
                usage = Some(s.clone());
            }
            compote::ContextValue::Int(i, _) => {
                usage = Some(i.to_string());
            }
            compote::ContextValue::Null(_) => return None,
            _ => {
                tracker.record(compote::Error::TypeMismatch {
                    path: tracker.current_path(),
                    expected: "array, object, or string".to_string(),
                    actual: value.type_name().to_string(),
                });
                return None;
            }
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
}

// ============================================================================
// Compote parsing for SyntaxOptArg
// ============================================================================

impl SyntaxOptArg {
    fn from_compote_config_value<S: compote::CustomSource, L: compote::CustomLevel>(
        value: &compote::ContextValue<S, L>,
        required: Option<bool>,
        tracker: &mut compote::ErrorTracker,
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

        match value {
            compote::ContextValue::Object(table, _) => {
                let value_for_details: Option<&compote::ContextValue<S, L>>;

                if let Some(name_value) = table.get("name") {
                    match name_value {
                        compote::ContextValue::String(s, _) => {
                            (names, arg_type, placeholders, leftovers) = parse_arg_name(s);
                            value_for_details = Some(value);
                        }
                        _ => {
                            tracker.push_field("name");
                            tracker.record(compote::Error::TypeMismatch {
                                path: tracker.current_path(),
                                expected: "string".to_string(),
                                actual: name_value.type_name().to_string(),
                            });
                            tracker.pop();
                            return None;
                        }
                    }
                } else if table.len() == 1 {
                    let (key, val) = table.iter().next()?;
                    (names, arg_type, placeholders, leftovers) = parse_arg_name(key);
                    value_for_details = Some(val);
                } else {
                    tracker.push_field("name");
                    tracker.record(compote::Error::MissingField {
                        path: tracker.current_path(),
                    });
                    tracker.pop();
                    return None;
                }

                if let Some(details) = value_for_details {
                    match details {
                        compote::ContextValue::String(s, _) => {
                            desc = Some(s.clone());
                        }
                        compote::ContextValue::Object(details_table, _) => {
                            desc = compote_get_str_or_none(details_table, "desc", tracker);
                            dest = compote_get_str_or_none(details_table, "dest", tracker);

                            if required.is_none() {
                                required = Some(compote_get_bool_or_default(
                                    details_table,
                                    "required",
                                    false,
                                    tracker,
                                ));
                            }

                            // Try to load placeholders
                            for key in &["placeholders", "placeholder"] {
                                let ph = compote_get_str_array(details_table, key, tracker);
                                if !ph.is_empty() {
                                    placeholders = ph;
                                    break;
                                }
                            }

                            default = compote_get_str_or_none(details_table, "default", tracker);
                            default_missing_value =
                                compote_get_str_or_none(details_table, "default_missing_value", tracker);

                            num_values = SyntaxOptArgNumValues::from_compote_config_value(
                                details_table.get("num_values"),
                                tracker,
                            );

                            value_delimiter =
                                compote_get_str_or_none(details_table, "delimiter", tracker)
                                    .and_then(|v| {
                                        v.chars().next().or_else(|| {
                                            tracker.push_field("delimiter");
                                            tracker.record(compote::Error::InvalidValue {
                                                path: tracker.current_path(),
                                                message: "delimiter must be non-empty".to_string(),
                                            });
                                            tracker.pop();
                                            None
                                        })
                                    });

                            last_arg_double_hyphen =
                                compote_get_bool_or_default(details_table, "last", false, tracker);
                            leftovers =
                                compote_get_bool_or_default(details_table, "leftovers", false, tracker);
                            allow_hyphen_values = compote_get_bool_or_default(
                                details_table,
                                "allow_hyphen_values",
                                false,
                                tracker,
                            );
                            allow_negative_numbers = compote_get_bool_or_default(
                                details_table,
                                "allow_negative_numbers",
                                false,
                                tracker,
                            );
                            group_occurrences = compote_get_bool_or_default(
                                details_table,
                                "group_occurrences",
                                false,
                                tracker,
                            );

                            arg_type = SyntaxOptArgType::from_compote_config_value(
                                details_table.get("type"),
                                details_table.get("values"),
                                value_delimiter,
                                tracker,
                            )
                            .unwrap_or(SyntaxOptArgType::String);

                            requires = compote_get_str_array(details_table, "requires", tracker);
                            conflicts_with =
                                compote_get_str_array(details_table, "conflicts_with", tracker);
                            required_without =
                                compote_get_str_array(details_table, "required_without", tracker);
                            required_without_all =
                                compote_get_str_array(details_table, "required_without_all", tracker);

                            if let Some(req_if_eq_value) = details_table.get("required_if_eq") {
                                tracker.push_field("required_if_eq");
                                match req_if_eq_value {
                                    compote::ContextValue::Object(map, _) => {
                                        for (k, v) in map {
                                            match v {
                                                compote::ContextValue::String(s, _) => {
                                                    required_if_eq.insert(k.clone(), s.clone());
                                                }
                                                compote::ContextValue::Int(i, _) => {
                                                    required_if_eq.insert(k.clone(), i.to_string());
                                                }
                                                compote::ContextValue::Float(f, _) => {
                                                    required_if_eq.insert(k.clone(), f.to_string());
                                                }
                                                compote::ContextValue::Bool(b, _) => {
                                                    required_if_eq.insert(k.clone(), b.to_string());
                                                }
                                                _ => {
                                                    tracker.push_field(k);
                                                    tracker.record(compote::Error::TypeMismatch {
                                                        path: tracker.current_path(),
                                                        expected: "string".to_string(),
                                                        actual: v.type_name().to_string(),
                                                    });
                                                    tracker.pop();
                                                }
                                            }
                                        }
                                    }
                                    _ => {
                                        tracker.record(compote::Error::TypeMismatch {
                                            path: tracker.current_path(),
                                            expected: "object".to_string(),
                                            actual: req_if_eq_value.type_name().to_string(),
                                        });
                                    }
                                }
                                tracker.pop();
                            }

                            if let Some(req_if_eq_all_value) =
                                details_table.get("required_if_eq_all")
                            {
                                tracker.push_field("required_if_eq_all");
                                match req_if_eq_all_value {
                                    compote::ContextValue::Object(map, _) => {
                                        for (k, v) in map {
                                            match v {
                                                compote::ContextValue::String(s, _) => {
                                                    required_if_eq_all.insert(k.clone(), s.clone());
                                                }
                                                compote::ContextValue::Int(i, _) => {
                                                    required_if_eq_all
                                                        .insert(k.clone(), i.to_string());
                                                }
                                                compote::ContextValue::Float(f, _) => {
                                                    required_if_eq_all
                                                        .insert(k.clone(), f.to_string());
                                                }
                                                compote::ContextValue::Bool(b, _) => {
                                                    required_if_eq_all
                                                        .insert(k.clone(), b.to_string());
                                                }
                                                _ => {
                                                    tracker.push_field(k);
                                                    tracker.record(compote::Error::TypeMismatch {
                                                        path: tracker.current_path(),
                                                        expected: "string".to_string(),
                                                        actual: v.type_name().to_string(),
                                                    });
                                                    tracker.pop();
                                                }
                                            }
                                        }
                                    }
                                    _ => {
                                        tracker.record(compote::Error::TypeMismatch {
                                            path: tracker.current_path(),
                                            expected: "object".to_string(),
                                            actual: req_if_eq_all_value.type_name().to_string(),
                                        });
                                    }
                                }
                                tracker.pop();
                            }

                            let aliases = compote_get_str_array(details_table, "aliases", tracker);
                            names.extend(aliases);
                        }
                        _ => {}
                    }
                }
            }
            compote::ContextValue::String(s, _) => {
                (names, arg_type, placeholders, leftovers) = parse_arg_name(s);
            }
            _ => {
                tracker.record(compote::Error::TypeMismatch {
                    path: tracker.current_path(),
                    expected: "string or object".to_string(),
                    actual: value.type_name().to_string(),
                });
                return None;
            }
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
}

// ============================================================================
// Compote parsing for SyntaxOptArgNumValues
// ============================================================================

impl SyntaxOptArgNumValues {
    fn from_compote_config_value<S: compote::CustomSource, L: compote::CustomLevel>(
        value: Option<&compote::ContextValue<S, L>>,
        tracker: &mut compote::ErrorTracker,
    ) -> Option<Self> {
        let value = value?;

        tracker.push_field("num_values");
        let result = match value {
            compote::ContextValue::Int(i, _) => Some(Self::Exactly(*i as usize)),
            compote::ContextValue::String(s, _) => Self::from_str_compote(s, tracker),
            compote::ContextValue::Null(_) => None,
            _ => {
                tracker.record(compote::Error::TypeMismatch {
                    path: tracker.current_path(),
                    expected: "integer or string".to_string(),
                    actual: value.type_name().to_string(),
                });
                None
            }
        };
        tracker.pop();
        result
    }

    fn from_str_compote(value: &str, tracker: &mut compote::ErrorTracker) -> Option<Self> {
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
                        tracker.record(compote::Error::InvalidValue {
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
                        tracker.record(compote::Error::InvalidValue {
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
                        tracker.record(compote::Error::InvalidValue {
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
                        tracker.record(compote::Error::InvalidValue {
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
                        tracker.record(compote::Error::InvalidValue {
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
                    tracker.record(compote::Error::InvalidValue {
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
// Compote parsing for SyntaxOptArgType
// ============================================================================

impl SyntaxOptArgType {
    pub(super) fn from_compote_config_value<S: compote::CustomSource, L: compote::CustomLevel>(
        type_value: Option<&compote::ContextValue<S, L>>,
        values_value: Option<&compote::ContextValue<S, L>>,
        value_delimiter: Option<char>,
        tracker: &mut compote::ErrorTracker,
    ) -> Option<Self> {
        let type_value = type_value?;

        tracker.push_field("type");

        // Check if type is an array - treat as enum with those values
        if let compote::ContextValue::Array(arr, _) = type_value {
            let values = arr
                .iter()
                .filter_map(|v| match v {
                    compote::ContextValue::String(s, _) => Some(s.clone()),
                    compote::ContextValue::Int(i, _) => Some(i.to_string()),
                    compote::ContextValue::Float(f, _) => Some(f.to_string()),
                    compote::ContextValue::Bool(b, _) => Some(b.to_string()),
                    _ => None,
                })
                .collect::<Vec<String>>();
            tracker.pop();
            return Some(Self::Enum(values));
        }

        let type_str = match type_value {
            compote::ContextValue::String(s, _) => s.clone(),
            compote::ContextValue::Int(i, _) => i.to_string(),
            compote::ContextValue::Null(_) => {
                tracker.pop();
                return None;
            }
            _ => {
                tracker.record(compote::Error::TypeMismatch {
                    path: tracker.current_path(),
                    expected: "string or array".to_string(),
                    actual: type_value.type_name().to_string(),
                });
                tracker.pop();
                return None;
            }
        };

        let obj = Self::from_str_compote(&type_str, tracker)?;
        tracker.pop();

        match obj {
            Self::Enum(values) if values.is_empty() => {
                if let Some(values_val) = values_value {
                    match values_val {
                        compote::ContextValue::Array(arr, _) => {
                            let values = arr
                                .iter()
                                .filter_map(|v| match v {
                                    compote::ContextValue::String(s, _) => Some(s.clone()),
                                    compote::ContextValue::Int(i, _) => Some(i.to_string()),
                                    compote::ContextValue::Float(f, _) => Some(f.to_string()),
                                    compote::ContextValue::Bool(b, _) => Some(b.to_string()),
                                    _ => None,
                                })
                                .collect::<Vec<String>>();
                            return Some(Self::Enum(values));
                        }
                        compote::ContextValue::String(s, _) => {
                            if let Some(delim) = value_delimiter {
                                let values = s
                                    .split(delim)
                                    .map(|v| v.to_string())
                                    .collect::<Vec<String>>();
                                return Some(Self::Enum(values));
                            } else {
                                return Some(Self::Enum(vec![s.clone()]));
                            }
                        }
                        _ => {}
                    }
                }
                None // Empty enum with no values
            }
            _ => Some(obj),
        }
    }

    fn from_str_compote(value: &str, tracker: &mut compote::ErrorTracker) -> Option<Self> {
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
                    tracker.record(compote::Error::InvalidValue {
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

// ============================================================================
// Compote parsing for SyntaxGroup
// ============================================================================

impl SyntaxGroup {
    fn from_compote_config_value_multi<S: compote::CustomSource, L: compote::CustomLevel>(
        value: &compote::ContextValue<S, L>,
        tracker: &mut compote::ErrorTracker,
    ) -> Vec<Self> {
        let mut groups = vec![];

        match value {
            compote::ContextValue::Array(array, _) => {
                for (idx, item) in array.iter().enumerate() {
                    tracker.push_index(idx);
                    if let Some(group) = Self::from_compote_config_value(item, None, tracker) {
                        groups.push(group);
                    }
                    tracker.pop();
                }
            }
            compote::ContextValue::Object(table, _) => {
                for (name, val) in table {
                    tracker.push_field(name);
                    if let Some(group) =
                        Self::from_compote_config_value(val, Some(name.clone()), tracker)
                    {
                        groups.push(group);
                    }
                    tracker.pop();
                }
            }
            compote::ContextValue::Null(_) => {}
            _ => {
                tracker.record(compote::Error::TypeMismatch {
                    path: tracker.current_path(),
                    expected: "array or object".to_string(),
                    actual: value.type_name().to_string(),
                });
            }
        }

        groups
    }

    fn from_compote_config_value<S: compote::CustomSource, L: compote::CustomLevel>(
        value: &compote::ContextValue<S, L>,
        name: Option<String>,
        tracker: &mut compote::ErrorTracker,
    ) -> Option<Self> {
        let table = match value {
            compote::ContextValue::Object(map, _) => {
                if map.is_empty() {
                    tracker.push_field("name");
                    tracker.record(compote::Error::MissingField {
                        path: tracker.current_path(),
                    });
                    tracker.pop();
                    return None;
                }
                map
            }
            _ => {
                tracker.record(compote::Error::TypeMismatch {
                    path: tracker.current_path(),
                    expected: "object".to_string(),
                    actual: value.type_name().to_string(),
                });
                return None;
            }
        };

        // Handle group name
        let (name, config_table) = match name {
            Some(n) => (n, table),
            None => {
                if table.len() == 1 {
                    let (key, val) = table.iter().next().unwrap();
                    match val {
                        compote::ContextValue::Object(inner, _) => (key.clone(), inner),
                        _ => {
                            tracker.push_field(key);
                            tracker.record(compote::Error::TypeMismatch {
                                path: tracker.current_path(),
                                expected: "object".to_string(),
                                actual: val.type_name().to_string(),
                            });
                            tracker.pop();
                            return None;
                        }
                    }
                } else if let Some(name_val) = table.get("name") {
                    match name_val {
                        compote::ContextValue::String(s, _) => (s.clone(), table),
                        compote::ContextValue::Int(i, _) => (i.to_string(), table),
                        _ => {
                            tracker.push_field("name");
                            tracker.record(compote::Error::TypeMismatch {
                                path: tracker.current_path(),
                                expected: "string".to_string(),
                                actual: name_val.type_name().to_string(),
                            });
                            tracker.pop();
                            return None;
                        }
                    }
                } else {
                    tracker.push_field("name");
                    tracker.record(compote::Error::MissingField {
                        path: tracker.current_path(),
                    });
                    tracker.pop();
                    return None;
                }
            }
        };

        // Parse parameters
        let parameters = compote_get_str_array(config_table, "parameters", tracker);
        if parameters.is_empty() {
            tracker.push_field("parameters");
            tracker.record(compote::Error::MissingField {
                path: tracker.current_path(),
            });
            tracker.pop();
            return None;
        }

        let multiple = compote_get_bool_or_default(config_table, "multiple", false, tracker);
        let required = compote_get_bool_or_default(config_table, "required", false, tracker);
        let requires = compote_get_str_array(config_table, "requires", tracker);
        let conflicts_with = compote_get_str_array(config_table, "conflicts_with", tracker);

        Some(Self {
            name,
            parameters,
            multiple,
            required,
            requires,
            conflicts_with,
        })
    }
}

#[cfg(test)]
#[path = "command_definition_test.rs"]
mod tests;
