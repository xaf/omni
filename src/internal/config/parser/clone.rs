/// CloneConfig using compote's derive macro.
///
/// The compote::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from compote's Config
/// - `serde::Serialize` implementation for serialization
///
/// We still need manual `serde::Deserialize` for compatibility with the existing
/// codebase that uses serde for some operations (e.g., cache files).
#[derive(Debug, Clone, compote::Config)]
pub struct CloneConfig {
    #[compote(default = "true")]
    pub auto_up: bool,

    #[compote(default = "5")]
    pub ls_remote_timeout: u64,
}

impl Default for CloneConfig {
    fn default() -> Self {
        Self {
            auto_up: true,
            ls_remote_timeout: 5,
        }
    }
}
