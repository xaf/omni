//! Compote type aliases and custom types for omni.
//!
//! This module defines omni-specific extensions to compote's generic types
//! and re-exports all commonly used compote types for convenience.

use std::path::Path;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::PathEntryConfig;

// =============================================================================
// Custom Source Type
// =============================================================================

/// Custom source type for omni extending compote's Source enum.
///
/// This implements `compote::CustomSource` to extend compote's `Source` enum
/// with omni-specific source types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OmniSource {
    /// Source is a package with associated metadata
    Package(PathEntryConfig),
}

impl compote::CustomSource for OmniSource {
    fn display_name(&self) -> String {
        match self {
            OmniSource::Package(entry) => {
                format!("package:{}", entry.package.as_deref().unwrap_or("unknown"))
            }
        }
    }

    fn file_path(&self) -> Option<&Path> {
        match self {
            OmniSource::Package(entry) => {
                if entry.full_path.is_empty() {
                    None
                } else {
                    Some(Path::new(&entry.full_path))
                }
            }
        }
    }
}

// =============================================================================
// Type aliases for compote types
// =============================================================================

// Note: We use the default type parameters () for Source and Level because
// compote's FromContextValue trait expects the default types. OmniSource is
// available for specific cases where package source tracking is needed.

/// Standard Source type (uses defaults for FromContextValue compatibility)
pub type Source = compote::Source<()>;

/// Source type with OmniSource for package tracking
pub type OmniContextSource = compote::Source<OmniSource>;

/// Standard Level type (uses default)
pub type Level = compote::Level<()>;

/// Standard Context type (uses defaults for FromContextValue compatibility)
pub type Context = compote::Context<(), ()>;

/// Context type with OmniSource for package tracking
pub type OmniContext = compote::Context<OmniSource, ()>;

/// Standard ContextValue type (uses defaults for FromContextValue compatibility)
pub type ContextValue = compote::ContextValue<(), ()>;

/// ContextValue type with OmniSource for package tracking
pub type OmniContextValue = compote::ContextValue<OmniSource, ()>;

// =============================================================================
// Backward compatibility aliases (for gradual migration)
// =============================================================================

/// Alias for Source matching existing code patterns (uses defaults)
pub type CompoteConfigSource = Source;

/// Alias for Level matching existing code patterns (uses defaults)
pub type CompoteConfigLevel = Level;

/// Alias for Context matching existing code patterns (uses defaults)
pub type CompoteConfigContext = Context;

/// Alias for ContextValue matching existing code patterns (uses defaults)
pub type CompoteConfigValue = ContextValue;

/// Alias for Error matching existing code patterns
pub type CompoteError = compote::Error;

/// Alias for ErrorTracker matching existing code patterns
pub type CompoteErrorTracker = compote::ErrorTracker;

/// Alias for FromContextValue matching existing code patterns
/// Note: FromContextValue is a trait, so we just re-export it
pub use compote::FromContextValue as CompoteFromConfigValue;

// =============================================================================
// Re-export compote types that don't need generic parameters
// =============================================================================

pub use compote::{
    // Error handling
    ConfigWarning,
    Error,
    ErrorTracker,
    // Deserialization traits
    AllowMapKeys,
    FromContextValue,
    FromContextValueWithTag,
    FromTagValue,
    // Values and modifiers
    MergeModifier,
    Value,
    // Formats
    Format,
    // Loading
    ConfigLoaderBuilder,
    // Config container
    Config,
    // Edit API
    ConfigEntry,
    IntoPath,
    RemoveResult,
    // Traits (for custom implementations)
    CustomLevel,
    CustomSource,
    IsEmpty,
    // Mutability
    MutabilityConstraint,
    MutabilityHashMap,
    MutabilityInfo,
    // Template utilities
    TemplateError,
    extract_field_references,
    interpolate_template,
    topological_sort,
    value_to_string,
    // Serialization
    to_format,
};

// Re-export serialization functions (yaml and json are enabled for omni)
pub use compote::{to_json, to_json_compact, to_yaml};

// Re-export the derive macro with a distinct name to avoid confusion with Config type
pub use compote::Config as DeriveConfig;
