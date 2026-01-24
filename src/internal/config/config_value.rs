//! Legacy configuration source and scope types.
//!
//! These types are deprecated and should be replaced with compote types
//! from `crate::internal::config::compote_types`.

use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::PathEntryConfig;

/// Legacy omni-specific source type.
///
/// Deprecated: Use `compote::Source` with appropriate type parameters instead.
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConfigSource {
    #[default]
    Default,
    File(String),
    Package(PathEntryConfig),
    Null,
}

impl ConfigSource {
    pub fn path(&self) -> Option<String> {
        match self {
            Self::File(path) => Some(path.to_string()),
            Self::Package(package) => Some(package.path.clone()),
            _ => None,
        }
    }
}

/// Legacy omni-specific scope type.
///
/// Deprecated: Use `compote::Level` instead.
/// Mapping: System -> System, User -> User, Workdir -> Local
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq, Ord, PartialOrd)]
pub enum ConfigScope {
    Null,
    #[default]
    Default,
    System,
    User,
    Workdir,
}

impl From<ConfigScope> for compote::Level {
    fn from(scope: ConfigScope) -> Self {
        match scope {
            ConfigScope::System => compote::Level::System,
            ConfigScope::User => compote::Level::User,
            ConfigScope::Workdir => compote::Level::Local,
            ConfigScope::Default => compote::Level::Local,
            ConfigScope::Null => compote::Level::Local,
        }
    }
}

impl From<&ConfigScope> for compote::Level {
    fn from(scope: &ConfigScope) -> Self {
        match scope {
            ConfigScope::System => compote::Level::System,
            ConfigScope::User => compote::Level::User,
            ConfigScope::Workdir => compote::Level::Local,
            ConfigScope::Default => compote::Level::Local,
            ConfigScope::Null => compote::Level::Local,
        }
    }
}
