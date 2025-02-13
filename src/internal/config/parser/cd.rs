use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::ConfigErrorHandler;
use crate::internal::config::parser::MatchSkipPromptIfConfig;
use crate::internal::config::ConfigValue;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CdConfig {
    pub fast_search: bool,
    pub path_match_min_score: f64,
    pub path_match_skip_prompt_if: MatchSkipPromptIfConfig,
}

impl Default for CdConfig {
    fn default() -> Self {
        Self {
            fast_search: Self::DEFAULT_FAST_SEARCH,
            path_match_min_score: Self::DEFAULT_PATH_MATCH_MIN_SCORE,
            path_match_skip_prompt_if: MatchSkipPromptIfConfig::default(),
        }
    }
}

impl CdConfig {
    const DEFAULT_FAST_SEARCH: bool = true;
    const DEFAULT_PATH_MATCH_MIN_SCORE: f64 = 0.12;

    pub(super) fn from_config_value(
        config_value: Option<ConfigValue>,
        error_handler: &ConfigErrorHandler,
    ) -> Self {
        let config_value = match config_value {
            Some(config_value) => config_value,
            None => {
                return Self::default();
            }
        };

        Self {
            fast_search: config_value.get_as_bool_or_default(
                "fast_search",
                Self::DEFAULT_FAST_SEARCH,
                &error_handler.with_key("fast_search"),
            ),
            path_match_min_score: config_value.get_as_float_or_default(
                "path_match_min_score",
                Self::DEFAULT_PATH_MATCH_MIN_SCORE,
                &error_handler.with_key("path_match_min_score"),
            ),
            path_match_skip_prompt_if: MatchSkipPromptIfConfig::from_config_value(
                config_value.get("path_match_skip_prompt_if"),
                &error_handler.with_key("path_match_skip_prompt_if"),
            ),
        }
    }
}
