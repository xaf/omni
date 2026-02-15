// ============================================================================
// NEW IMPLEMENTATION USING COMPOTE
// ============================================================================

/// ConfigCommandsConfig using compote's derive macro.
///
/// The compote::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from compote's Config
/// - `serde::Serialize` implementation for serialization
#[derive(Debug, Clone, compote::Config)]
pub struct ConfigCommandsConfig {
    #[compote(default = "true")]
    pub split_on_dash: bool,

    #[compote(default = "true")]
    pub split_on_slash: bool,
}

impl Default for ConfigCommandsConfig {
    fn default() -> Self {
        Self {
            split_on_dash: true,
            split_on_slash: true,
        }
    }
}

// ============================================================================
// OLD IMPLEMENTATION (COMMENTED OUT FOR REFERENCE)
// ============================================================================
/*
use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::ConfigErrorHandler;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConfigCommandsConfig {
    pub split_on_dash: bool,
    pub split_on_slash: bool,
}

impl Default for ConfigCommandsConfig {
    fn default() -> Self {
        Self {
            split_on_dash: Self::DEFAULT_SPLIT_ON_DASH,
            split_on_slash: Self::DEFAULT_SPLIT_ON_SLASH,
        }
    }
}

impl ConfigCommandsConfig {
    const DEFAULT_SPLIT_ON_DASH: bool = true;
    const DEFAULT_SPLIT_ON_SLASH: bool = true;

    pub(super) fn from_context_value(
        config_value: Option<ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        Self {
            split_on_dash: config_value.get_as_bool_or_default(
                "split_on_dash",
                Self::DEFAULT_SPLIT_ON_DASH,
                &error_handler.with_key("split_on_dash"),
            ),
            split_on_slash: config_value.get_as_bool_or_default(
                "split_on_slash",
                Self::DEFAULT_SPLIT_ON_SLASH,
                &error_handler.with_key("split_on_slash"),
            ),
        }
    }
}
*/
