// ============================================================================
// NEW IMPLEMENTATION USING FEUILLETAGE
// ============================================================================

/// AskPassConfig using feuilletage's derive macro.
///
/// This configuration is restricted to system and user levels only - workdir
/// (local) level cannot override these settings. This is enforced via
/// `mutable_by = ["system", "user"]` on each field.
///
/// Note: The `mutable_by` attribute is defined but currently the enforcement
/// happens in the `from_context_value` bridge method via `reject_scope`.
/// Full integration with feuilletage's mutability system would require porting
/// the config loading layer to use feuilletage's Config type directly.
///
/// The feuilletage::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from feuilletage's Config
/// - `serde::Serialize` implementation for serialization
#[derive(Debug, Clone, feuilletage::Config)]
pub struct AskPassConfig {
    #[feuilletage(default = "true", mutable_by = ["system", "user"])]
    pub enabled: bool,

    #[feuilletage(default = "true", mutable_by = ["system", "user"])]
    pub enable_gui: bool,

    #[feuilletage(default = "false", mutable_by = ["system", "user"])]
    pub prefer_gui: bool,
}

impl Default for AskPassConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_gui: true,
            prefer_gui: false,
        }
    }
}

// ============================================================================
// OLD IMPLEMENTATION (COMMENTED OUT FOR REFERENCE)
// ============================================================================
/*
use crate::internal::config::parser::errors::ConfigErrorHandler;
use crate::internal::config::parser::errors::ConfigErrorKind;
use crate::internal::config::ConfigScope;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AskPassConfig {
    pub enabled: bool,
    pub enable_gui: bool,
    pub prefer_gui: bool,
}

impl Default for AskPassConfig {
    fn default() -> Self {
        Self {
            enabled: Self::DEFAULT_ENABLED,
            enable_gui: Self::DEFAULT_ENABLE_GUI,
            prefer_gui: Self::DEFAULT_PREFER_GUI,
        }
    }
}

impl AskPassConfig {
    const DEFAULT_ENABLED: bool = true;
    const DEFAULT_ENABLE_GUI: bool = true;
    const DEFAULT_PREFER_GUI: bool = false;

    pub(super) fn from_context_value(
        config_value: Option<ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let config_value = match config_value.reject_scope(&ConfigScope::Workdir) {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        if !config_value.is_table() {
            error_handler
                .with_expected("table")
                .with_actual(config_value)
                .error(ConfigErrorKind::InvalidValueType);

            return Self::default();
        }

        Self {
            enabled: config_value.get_as_bool_or_default(
                "enabled",
                Self::DEFAULT_ENABLED,
                &error_handler.with_key("enabled"),
            ),
            enable_gui: config_value.get_as_bool_or_default(
                "enable_gui",
                Self::DEFAULT_ENABLE_GUI,
                &error_handler.with_key("enable_gui"),
            ),
            prefer_gui: config_value.get_as_bool_or_default(
                "prefer_gui",
                Self::DEFAULT_PREFER_GUI,
                &error_handler.with_key("prefer_gui"),
            ),
        }
    }
}
*/
