/// CloneConfig using feuilletage's derive macro.
///
/// The feuilletage::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from feuilletage's Config
/// - `serde::Serialize` implementation for serialization
///
/// We still need manual `serde::Deserialize` for compatibility with the existing
/// codebase that uses serde for some operations (e.g., cache files).
#[derive(Debug, Clone, feuilletage::Config)]
pub struct CloneConfig {
    #[feuilletage(default = "true")]
    pub auto_up: bool,

    #[feuilletage(default = "5")]
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
