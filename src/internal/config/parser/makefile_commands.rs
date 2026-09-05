/// MakefileCommandsConfig using feuilletage's derive macro.
///
/// The feuilletage::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from feuilletage's Config
/// - `serde::Serialize` implementation for serialization
///
/// We still need manual `serde::Deserialize` for compatibility with the existing
/// codebase that uses serde for some operations (e.g., loading from cache files).
#[derive(Debug, Clone, feuilletage::Config)]
pub struct MakefileCommandsConfig {
    #[feuilletage(default = "true")]
    pub enabled: bool,

    #[feuilletage(default = "true")]
    pub split_on_dash: bool,

    #[feuilletage(default = "true")]
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
