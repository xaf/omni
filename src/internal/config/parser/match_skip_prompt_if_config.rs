/// MatchSkipPromptIfConfig using compote's derive macro.
///
/// This configuration controls automatic prompt skipping based on match scores.
///
/// The compote::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from compote's Config
/// - `serde::Serialize` implementation for serialization
///
/// We still need manual `serde::Deserialize` for compatibility with the existing
/// codebase that uses serde for some operations (e.g., loading from cache files).
#[derive(Debug, Clone, compote::Config)]
pub struct MatchSkipPromptIfConfig {
    #[compote(default = "false")]
    pub enabled: bool,

    #[compote(default = "0.80")]
    pub first_min: f64,

    #[compote(default = "0.60")]
    pub second_max: f64,
}

impl Default for MatchSkipPromptIfConfig {
    fn default() -> Self {
        Self {
            // By default if nothing is specified, we disable this
            enabled: false,
            first_min: Self::DEFAULT_FIRST_MIN,
            second_max: Self::DEFAULT_SECOND_MAX,
        }
    }
}

impl MatchSkipPromptIfConfig {
    const DEFAULT_FIRST_MIN: f64 = 0.80;
    const DEFAULT_SECOND_MAX: f64 = 0.60;
}

