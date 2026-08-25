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
/// Uses compote::Config with transparent to auto-generate FromContextValue.
/// Custom Serialize and Deserialize impls are kept for backwards compatibility
/// (serializes/deserializes as array directly, not as struct).
#[derive(Debug, Clone, compote::Config)]
#[compote(transparent, skip_serialize, skip_deserialize)]
pub struct ShellAliasesConfig {
    #[compote(default)]
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
        Self {
            aliases: Vec::new(),
        }
    }
}

/// ShellAliasConfig using compote's derive macro.
///
/// The compote::Config derive macro automatically generates:
/// - `FromContextValue` implementation for deserialization from compote's Config
/// - `serde::Serialize` and `serde::Deserialize` implementations
#[derive(Debug, Clone, compote::Config)]
pub struct ShellAliasConfig {
    #[compote(default = "String::new()", skip_if_empty)]
    pub alias: String,

    #[compote(skip_if_empty)]
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
