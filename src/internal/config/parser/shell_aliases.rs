use serde::Deserialize;
use serde::Serialize;
// Note: ShellAliasConfig uses feuilletage::Config which auto-generates Serialize
// ShellAliasesConfig uses manual Serialize for custom array serialization

use crate::internal::cache::utils::Empty;

// ============================================================================
// NEW IMPLEMENTATION USING FEUILLETAGE
// ============================================================================

/// ShellAliasesConfig - container for shell aliases.
///
/// Uses feuilletage::Config with transparent to auto-generate FromContextValue.
/// Custom Serialize and Deserialize impls are kept for backwards compatibility
/// (serializes/deserializes as array directly, not as struct).
#[derive(Debug, Clone, feuilletage::Config)]
#[feuilletage(transparent, skip_serialize, skip_deserialize)]
#[derive(Default)]
pub struct ShellAliasesConfig {
    #[feuilletage(default)]
    pub aliases: Vec<ShellAliasConfig>,
}
impl Empty for ShellAliasesConfig {
    fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }
}

impl feuilletage::IsEmpty for ShellAliasesConfig {
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


/// ShellAliasConfig using feuilletage's derive macro.
///
/// The feuilletage::Config derive macro automatically generates:
/// - `FromContextValue` implementation for deserialization from feuilletage's Config
/// - `serde::Serialize` and `serde::Deserialize` implementations
#[derive(Debug, Clone, feuilletage::Config)]
#[derive(Default)]
pub struct ShellAliasConfig {
    #[feuilletage(default = "String::new()", skip_if_empty)]
    pub alias: String,

    #[feuilletage(skip_if_empty)]
    pub target: Option<String>,
}
