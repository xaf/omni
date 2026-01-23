use serde::Deserialize;
use serde::Serialize;
// Note: ShellAliasConfig uses compote::Config which auto-generates Serialize
// ShellAliasesConfig uses manual Serialize for custom array serialization

use crate::internal::cache::utils::Empty;

// Compote imports
use compote::Error as CompoteError;
use compote::ContextValue as CompoteConfigValue;
use compote::ErrorTracker as CompoteErrorTracker;
use compote::FromContextValue as CompoteFromConfigValue;

// ============================================================================
// NEW IMPLEMENTATION USING COMPOTE
// ============================================================================

/// ShellAliasesConfig - container for shell aliases.
///
/// This struct does NOT use compote::Config derive because it needs custom
/// serialization behavior (serializes as array directly, not as struct).
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


// Manual Deserialize implementation for compatibility with existing code
// (e.g., loading from cache files, etc.)
impl<'de> Deserialize<'de> for ShellAliasConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            #[serde(default)]
            alias: String,
            #[serde(default)]
            target: Option<String>,
        }

        let helper = Helper::deserialize(deserializer)?;
        Ok(ShellAliasConfig {
            alias: helper.alias,
            target: helper.target,
        })
    }
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
// Compote FromConfigValue implementation for ShellAliasesConfig
// ============================================================================

impl CompoteFromConfigValue for ShellAliasesConfig {
    fn from_config_value(
        value: &CompoteConfigValue,
        tracker: &mut CompoteErrorTracker,
    ) -> Result<Self, CompoteError> {
        match value {
            CompoteConfigValue::Array(arr, _) => {
                let mut aliases = Vec::new();
                for (idx, item) in arr.iter().enumerate() {
                    tracker.push_index(idx);
                    // ShellAliasConfig has FromConfigValue from the derive macro
                    match <ShellAliasConfig as CompoteFromConfigValue>::from_config_value(
                        item, tracker,
                    ) {
                        Ok(alias) => aliases.push(alias),
                        Err(e) => tracker.record(e),
                    }
                    tracker.pop();
                }
                Ok(Self { aliases })
            }
            CompoteConfigValue::Null(_) => Ok(Self::default()),
            _ => Err(CompoteError::TypeMismatch {
                expected: "array".to_string(),
                actual: value.type_name().to_string(),
                path: tracker.current_path(),
            }),
        }
    }
}
