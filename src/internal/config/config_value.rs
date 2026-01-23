use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::PathEntryConfig;

// Omni-specific source type
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

impl From<ConfigSource> for compote::Source {
    fn from(source: ConfigSource) -> Self {
        match source {
            ConfigSource::File(path) => compote::Source::File(PathBuf::from(path)),
            ConfigSource::Package(entry) => {
                compote::Source::Package(entry.package.unwrap_or_default())
            }
            ConfigSource::Default => compote::Source::Default,
            ConfigSource::Null => compote::Source::Default,
        }
    }
}

impl From<&ConfigSource> for compote::Source {
    fn from(source: &ConfigSource) -> Self {
        match source {
            ConfigSource::File(path) => compote::Source::File(PathBuf::from(path)),
            ConfigSource::Package(entry) => {
                compote::Source::Package(entry.package.clone().unwrap_or_default())
            }
            ConfigSource::Default => compote::Source::Default,
            ConfigSource::Null => compote::Source::Default,
        }
    }
}

// Omni-specific scope type
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
