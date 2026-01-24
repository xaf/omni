pub(crate) mod compote_loader;
pub(crate) use compote_loader::OmniConfigLoader;

pub(crate) mod compote_types;
pub(crate) use compote_types::*;

// Legacy config_value module - to be removed after migration
pub(crate) mod config_value;
pub(crate) use config_value::ConfigScope;
pub(crate) use config_value::ConfigSource;

pub(crate) mod loader;
pub(crate) use loader::ConfigLoader;

pub(crate) mod parser;
pub(crate) use parser::config;
pub(crate) use parser::flush_config;
pub(crate) use parser::global_config;
pub(crate) use parser::CommandDefinition;
pub(crate) use parser::CommandSyntax;
pub(crate) use parser::OmniConfig;
pub(crate) use parser::OrgConfig;
pub(crate) use parser::SyntaxGroup;
pub(crate) use parser::SyntaxOptArg;
pub(crate) use parser::SyntaxOptArgNumValues;
pub(crate) use parser::SyntaxOptArgType;

pub(crate) mod up;

pub(crate) mod bootstrap;
pub(crate) use bootstrap::ensure_bootstrap;

pub(crate) mod template;

pub(crate) mod utils;
