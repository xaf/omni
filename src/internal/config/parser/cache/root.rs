use crate::internal::config::parser::cache::CargoInstallCacheConfig;
use crate::internal::config::parser::cache::GithubReleaseCacheConfig;
use crate::internal::config::parser::cache::GoInstallCacheConfig;
use crate::internal::config::parser::cache::HomebrewCacheConfig;
use crate::internal::config::parser::cache::MiseCacheConfig;
use crate::internal::config::parser::cache::UpEnvironmentCacheConfig;
use crate::internal::env::cache_home;

/// CacheConfig using compote's derive macro.
///
/// The compote::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from compote's Config
/// - `serde::Serialize` implementation for serialization
#[derive(Debug, Clone, compote::Config)]
pub struct CacheConfig {
    #[compote(default_fn = "cache_home")]
    pub path: String,

    #[compote(default)]
    pub environment: UpEnvironmentCacheConfig,

    #[compote(default)]
    pub github_release: GithubReleaseCacheConfig,

    #[compote(default)]
    pub cargo_install: CargoInstallCacheConfig,

    #[compote(default)]
    pub go_install: GoInstallCacheConfig,

    #[compote(default)]
    pub homebrew: HomebrewCacheConfig,

    #[compote(default)]
    pub mise: MiseCacheConfig,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            path: cache_home(),
            environment: UpEnvironmentCacheConfig::default(),
            github_release: GithubReleaseCacheConfig::default(),
            cargo_install: CargoInstallCacheConfig::default(),
            go_install: GoInstallCacheConfig::default(),
            homebrew: HomebrewCacheConfig::default(),
            mise: MiseCacheConfig::default(),
        }
    }
}
