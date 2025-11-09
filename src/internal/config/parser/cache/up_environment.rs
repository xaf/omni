use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::errors::ConfigErrorHandler;
use crate::internal::config::parser::errors::ConfigErrorKind;
use crate::internal::config::utils::parse_duration_or_default;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpEnvironmentCacheConfig {
    pub retention: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_per_workdir: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total: Option<usize>,
    #[serde(default)]
    pub retention_active: u64,
    #[serde(default)]
    pub stale_check_every: u64,
}

impl Default for UpEnvironmentCacheConfig {
    fn default() -> Self {
        Self {
            retention: Self::DEFAULT_RETENTION,
            max_per_workdir: None,
            max_total: None,
            retention_active: Self::DEFAULT_RETENTION_ACTIVE,
            stale_check_every: Self::DEFAULT_STALE_CHECK_EVERY,
        }
    }
}

impl UpEnvironmentCacheConfig {
    const DEFAULT_RETENTION: u64 = 7776000; // 90 days
    const DEFAULT_RETENTION_ACTIVE: u64 = 15552000; // 180 days (6 months)
    const DEFAULT_STALE_CHECK_EVERY: u64 = 604800; // 7 days

    pub fn from_config_value(
        config_value: Option<ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => return Self::default(),
        };

        let retention = parse_duration_or_default(
            config_value.get("retention").as_ref(),
            Self::DEFAULT_RETENTION,
            &error_handler.with_key("retention"),
        );

        let max_per_workdir = match config_value.get("max_per_workdir") {
            Some(v) => match v.as_unsigned_integer() {
                Some(v) => Some(v as usize),
                None => {
                    error_handler
                        .with_key("max_per_workdir")
                        .with_expected("unsigned integer")
                        .with_actual(v)
                        .error(ConfigErrorKind::InvalidValueType);

                    None
                }
            },
            None => None,
        };

        let max_total = match config_value.get("max_total") {
            Some(v) => match v.as_unsigned_integer() {
                Some(v) => Some(v as usize),
                None => {
                    error_handler
                        .with_key("max_total")
                        .with_expected("unsigned integer")
                        .with_actual(v)
                        .error(ConfigErrorKind::InvalidValueType);

                    None
                }
            },
            None => None,
        };

        let retention_active = parse_duration_or_default(
            config_value.get("retention_active").as_ref(),
            Self::DEFAULT_RETENTION_ACTIVE,
            &error_handler.with_key("retention_active"),
        );

        let stale_check_every = parse_duration_or_default(
            config_value.get("stale_check_every").as_ref(),
            Self::DEFAULT_STALE_CHECK_EVERY,
            &error_handler.with_key("stale_check_every"),
        );

        Self {
            retention,
            max_per_workdir,
            max_total,
            retention_active,
            stale_check_every,
        }
    }
}
