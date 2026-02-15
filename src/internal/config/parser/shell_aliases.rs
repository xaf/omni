use serde::Deserialize;
use serde::Serialize;
// Note: ShellAliasConfig uses compote::Config which auto-generates Serialize
// ShellAliasesConfig uses manual Serialize for custom array serialization

use crate::internal::cache::utils::Empty;

// ============================================================================
// NEW IMPLEMENTATION USING COMPOTE
// ============================================================================

/// ShellAliasesConfig - container for shell aliases.
///
/// This struct does NOT use compote::Config derive because it needs custom
/// serialization behavior (serializes as array directly, not as struct).
/// Instead, it has a manual FromContextValue implementation that delegates
/// to the derive-generated impl for ShellAliasConfig.
///
/// The inner ShellAliasConfig structs use compote::Config.
#[derive(Debug, Clone)]
pub struct ShellAliasesConfig {
    pub aliases: Vec<ShellAliasConfig>,
}

impl Empty for ShellAliasesConfig {
    fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }
}

impl compote::IsEmpty for ShellAliasesConfig {
    fn is_empty(&self) -> bool {
        Empty::is_empty(self)
    }
}

// Custom Serialize implementation for backwards compatibility
// (serializes as array directly, not as struct with "aliases" field)
impl Serialize for ShellAliasesConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.aliases.serialize(serializer)
    }
}


// Manual Deserialize implementation for compatibility with existing code
// (e.g., loading from cache files, etc.)
impl<'de> Deserialize<'de> for ShellAliasesConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize as Vec<ShellAliasConfig> directly for backwards compatibility
        let aliases = Vec::<ShellAliasConfig>::deserialize(deserializer)?;
        Ok(ShellAliasesConfig { aliases })
    }
}

impl Default for ShellAliasesConfig {
    fn default() -> Self {
        Self { aliases: Vec::new() }
    }
}

/// ShellAliasConfig using compote's derive macro.
///
/// The compote::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from compote's Config
/// - `serde::Serialize` implementation for serialization
///
/// We still need manual `serde::Deserialize` for compatibility with the existing
/// codebase that uses serde for some operations.
#[derive(Debug, Clone, compote::Config)]
pub struct ShellAliasConfig {
    #[compote(default = "String::new()")]
    #[compote(serde_skip_serializing_if = "String::is_empty")]
    pub alias: String,

    #[compote(serde_skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}


impl Default for ShellAliasConfig {
    fn default() -> Self {
        Self {
            alias: String::new(),
            target: None,
        }
    }
}

// ============================================================================
// Compote FromContextValue implementation for ShellAliasesConfig
// ============================================================================
// Manual implementation for ShellAliasesConfig to handle array input directly
// and graceful error handling (skip invalid items).
// ShellAliasConfig uses the compote::Config derive macro.

impl<S: compote::CustomSource, L: compote::CustomLevel> compote::FromContextValue<S, L> for ShellAliasesConfig {
    fn from_context_value(
        value: &compote::ContextValue<S, L>,
        tracker: &mut compote::ErrorTracker,
    ) -> Result<Self, compote::Error> {
        match value {
            compote::ContextValue::Array(arr, _) => {
                let mut aliases = Vec::new();
                for (idx, item) in arr.iter().enumerate() {
                    tracker.push_index(idx);
                    // ShellAliasConfig has FromContextValue from the derive macro
                    match <ShellAliasConfig as compote::FromContextValue<S, L>>::from_context_value(
                        item, tracker,
                    ) {
                        Ok(alias) => aliases.push(alias),
                        Err(e) => tracker.record(e),
                    }
                    tracker.pop();
                }
                Ok(Self { aliases })
            }
            compote::ContextValue::Null(_) => Ok(Self::default()),
            _ => Err(compote::Error::TypeMismatch {
                expected: "array".to_string(),
                actual: value.type_name().to_string(),
                path: tracker.current_path(),
            }),
        }
    }
}
