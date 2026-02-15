/// HomebrewCacheConfig using compote's derive macro.
///
/// The compote::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from compote's Config
/// - `serde::Serialize` implementation for serialization
#[derive(Debug, Clone, compote::Config)]
pub struct HomebrewCacheConfig {
    #[compote(duration, default = "86400")]
    pub update_expire: u64, // 1 day

    #[compote(duration, default = "86400")]
    pub tap_update_expire: u64, // 1 day

    #[compote(duration, default = "86400")]
    pub install_update_expire: u64, // 1 day

    #[compote(duration, default = "43200")]
    pub install_check_expire: u64, // 12 hours

    #[compote(duration, default = "604800")]
    pub cleanup_after: u64, // 1 week
}

impl HomebrewCacheConfig {
    const DEFAULT_UPDATE_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_TAP_UPDATE_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_INSTALL_UPDATE_EXPIRE: u64 = 86400; // 1 day
    const DEFAULT_INSTALL_CHECK_EXPIRE: u64 = 43200; // 12 hours
    const DEFAULT_CLEANUP_AFTER: u64 = 604800; // 1 week
}

impl Default for HomebrewCacheConfig {
    fn default() -> Self {
        Self {
            update_expire: Self::DEFAULT_UPDATE_EXPIRE,
            tap_update_expire: Self::DEFAULT_TAP_UPDATE_EXPIRE,
            install_update_expire: Self::DEFAULT_INSTALL_UPDATE_EXPIRE,
            install_check_expire: Self::DEFAULT_INSTALL_CHECK_EXPIRE,
            cleanup_after: Self::DEFAULT_CLEANUP_AFTER,
        }
    }
}
