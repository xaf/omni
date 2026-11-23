pub(crate) mod feuilletage_loader;
pub(crate) use feuilletage_loader::OmniConfigLoader;

pub(crate) mod feuilletage_types;
pub(crate) use feuilletage_types::*;

pub(crate) mod loader;
pub(crate) use loader::ConfigLoader;

// feuilletage 0.1.3's Config derive emits `field: field` initializers.
// The generated impls do not inherit lint allowances on the derived type.
#[allow(clippy::redundant_field_names)]
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

// feuilletage::Config generates redundant field names in this module too.
#[allow(clippy::redundant_field_names)]
pub(crate) mod up;

pub(crate) mod bootstrap;
pub(crate) use bootstrap::ensure_bootstrap;

pub(crate) mod template;

pub(crate) mod utils;
