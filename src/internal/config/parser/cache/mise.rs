/// MiseCacheConfig using compote's derive macro.
///
/// The compote::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from compote's Config
/// - `serde::Serialize` implementation for serialization
#[derive(Debug, Clone, compote::Config)]
pub struct MiseCacheConfig {
    #[compote(duration, default = "86400")]
    pub update_expire: u64, // 1 day

    #[compote(duration, default = "86400")]
    pub plugin_update_expire: u64, // 1 day

    #[compote(duration, default = "3600")]
    pub plugin_versions_expire: u64, // 1 hour

    #[compote(duration, default = "7776000")]
    pub plugin_versions_retention: u64, // 90 days

    #[compote(duration, default = "604800")]
    pub cleanup_after: u64, // 1 week
}

impl MiseCacheConfig {
    const DEFAULT_UPDATE_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_PLUGIN_UPDATE_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_PLUGIN_VERSIONS_EXPIRE: u64 = 3600; // 1 hour
    const DEFAULT_PLUGIN_VERSIONS_RETENTION: u64 = 7776000; // 90 days
    const DEFAULT_CLEANUP_AFTER: u64 = 604800; // 1 week
}

impl Default for MiseCacheConfig {
    fn default() -> Self {
        Self {
            update_expire: Self::DEFAULT_UPDATE_EXPIRE,
            plugin_update_expire: Self::DEFAULT_PLUGIN_UPDATE_EXPIRE,
            plugin_versions_expire: Self::DEFAULT_PLUGIN_VERSIONS_EXPIRE,
            plugin_versions_retention: Self::DEFAULT_PLUGIN_VERSIONS_RETENTION,
            cleanup_after: Self::DEFAULT_CLEANUP_AFTER,
        }
    }
}
