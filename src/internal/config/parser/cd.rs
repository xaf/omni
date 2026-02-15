use crate::internal::config::parser::MatchSkipPromptIfConfig;

// ============================================================================
// NEW IMPLEMENTATION USING COMPOTE
// ============================================================================

/// CdConfig using compote's derive macro.
///
/// The compote::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from compote's Config
/// - `serde::Serialize` implementation for serialization
///
/// We still need manual `serde::Deserialize` for compatibility with the existing
/// codebase that uses serde for some operations.
#[derive(Debug, Clone, compote::Config)]
pub struct CdConfig {
    #[compote(default = "true")]
    pub fast_search: bool,

    #[compote(default = "0.12")]
    pub path_match_min_score: f64,

    #[compote(nested)]
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
