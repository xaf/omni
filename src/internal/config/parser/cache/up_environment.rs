/// UpEnvironmentCacheConfig using feuilletage's derive macro.
///
/// The feuilletage::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from feuilletage's Config
/// - `serde::Serialize` implementation for serialization
#[derive(Debug, Clone, feuilletage::Config)]
pub struct UpEnvironmentCacheConfig {
    #[feuilletage(duration, default = "7776000")]
    pub retention: u64, // 90 days

    pub max_per_workdir: Option<usize>,

    pub max_total: Option<usize>,

    #[feuilletage(duration, default = "15552000")]
    pub retention_stale: u64, // 180 days (6 months)
}

impl UpEnvironmentCacheConfig {
    const DEFAULT_RETENTION: u64 = 7776000; // 90 days
    const DEFAULT_RETENTION_STALE: u64 = 15552000; // 180 days (6 months)
}

impl Default for UpEnvironmentCacheConfig {
    fn default() -> Self {
        Self {
            retention: Self::DEFAULT_RETENTION,
            max_per_workdir: None,
            max_total: None,
            retention_stale: Self::DEFAULT_RETENTION_STALE,
        }
    }
}
