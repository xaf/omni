/// MakefileCommandsConfig using compote's derive macro.
///
/// The compote::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from compote's Config
/// - `serde::Serialize` implementation for serialization
///
/// We still need manual `serde::Deserialize` for compatibility with the existing
/// codebase that uses serde for some operations (e.g., loading from cache files).
#[derive(Debug, Clone, compote::Config)]
pub struct MakefileCommandsConfig {
    #[compote(default = "true")]
    pub enabled: bool,

    #[compote(default = "true")]
    pub split_on_dash: bool,

    #[compote(default = "true")]
    pub split_on_slash: bool,
}

impl Default for MakefileCommandsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            split_on_dash: true,
            split_on_slash: true,
        }
    }
}

