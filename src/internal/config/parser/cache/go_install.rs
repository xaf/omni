/// GoInstallCacheConfig using feuilletage's derive macro.
///
/// The feuilletage::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from feuilletage's Config
/// - `serde::Serialize` implementation for serialization
#[derive(Debug, Clone, feuilletage::Config)]
pub struct GoInstallCacheConfig {
    #[feuilletage(duration, default = "86400")]
    pub versions_expire: u64, // 1 day

    #[feuilletage(duration, default = "7776000")]
    pub versions_retention: u64, // 90 days

    #[feuilletage(duration, default = "604800")]
    pub cleanup_after: u64, // 1 week
}

impl GoInstallCacheConfig {
    const DEFAULT_VERSIONS_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_VERSIONS_RETENTION: u64 = 7776000; // 90 days
    const DEFAULT_CLEANUP_AFTER: u64 = 604800; // 1 week
}

impl Default for GoInstallCacheConfig {
    fn default() -> Self {
        Self {
            versions_expire: Self::DEFAULT_VERSIONS_EXPIRE,
            versions_retention: Self::DEFAULT_VERSIONS_RETENTION,
            cleanup_after: Self::DEFAULT_CLEANUP_AFTER,
        }
    }
}
