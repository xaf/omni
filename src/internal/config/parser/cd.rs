use crate::internal::config::parser::MatchSkipPromptIfConfig;

// ============================================================================
// NEW IMPLEMENTATION USING FEUILLETAGE
// ============================================================================

/// CdConfig using feuilletage's derive macro.
///
/// The feuilletage::Config derive macro automatically generates:
/// - `FromContextValue` implementation for deserialization from feuilletage's Config
/// - `serde::Serialize` and `serde::Deserialize` implementations
#[derive(Debug, Clone, feuilletage::Config)]
pub struct CdConfig {
    #[feuilletage(default = "true")]
    pub fast_search: bool,

    #[feuilletage(default = "0.12")]
    pub path_match_min_score: f64,

    pub path_match_skip_prompt_if: MatchSkipPromptIfConfig,
}

impl Default for CdConfig {
    fn default() -> Self {
        Self {
            fast_search: true,
            path_match_min_score: 0.12,
            path_match_skip_prompt_if: MatchSkipPromptIfConfig::default(),
        }
    }
}
