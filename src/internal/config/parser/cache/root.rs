use crate::internal::config::parser::cache::CargoInstallCacheConfig;
use crate::internal::config::parser::cache::GithubReleaseCacheConfig;
use crate::internal::config::parser::cache::GoInstallCacheConfig;
use crate::internal::config::parser::cache::HomebrewCacheConfig;
use crate::internal::config::parser::cache::MiseCacheConfig;
use crate::internal::config::parser::cache::UpEnvironmentCacheConfig;
use crate::internal::env::cache_home;

/// CacheConfig using feuilletage's derive macro.
///
/// The feuilletage::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from feuilletage's Config
/// - `serde::Serialize` implementation for serialization
#[derive(Debug, Clone, feuilletage::Config)]
pub struct CacheConfig {
    #[feuilletage(default_fn = "cache_home")]
    pub path: String,

    #[feuilletage(default)]
    pub environment: UpEnvironmentCacheConfig,

    #[feuilletage(default)]
    pub github_release: GithubReleaseCacheConfig,

    #[feuilletage(default)]
    pub cargo_install: CargoInstallCacheConfig,

    #[feuilletage(default)]
    pub go_install: GoInstallCacheConfig,

    #[feuilletage(default)]
    pub homebrew: HomebrewCacheConfig,

    #[feuilletage(default)]
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
