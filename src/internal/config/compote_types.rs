//! Compote type aliases and custom types for omni.
//!
//! This module defines omni-specific source types and re-exports commonly used
//! compote types for convenience.
//!
//! # Source Types
//!
//! Omni uses a custom source type [`OmniSource`] that extends compote's built-in
//! sources with package tracking capability. This allows omni to track which
//! package a configuration value came from.
//!
//! # Usage
//!
//! Most code should use the type aliases defined here:
//! - [`Source`] - alias for `compote::Source` (built-in sources)
//! - [`Level`] - alias for `compote::Level` (built-in levels)
//! - [`Context`] - alias for `compote::Context<Source, Level>`
//! - [`ContextValue`] - alias for `compote::ContextValue<Source, Level>`
//!
//! For code that needs to track package sources, use `OmniSource` directly
//! with the appropriate generic parameters.

use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use crate::internal::config::parser::PathEntryConfig;

// =============================================================================
// Custom Source Type
// =============================================================================

/// Custom source type for omni extending compote's Source enum.
///
/// This type includes all the standard source variants (File, Environment,
/// Programmatic, Default) plus omni-specific variants like Package.
/// It implements `compote::CustomSource` so it can be used as a source type
/// parameter in `Config<OmniSource, Level>`, etc.
///
/// # Examples
///
/// ```ignore
/// use omni::internal::config::OmniSource;
/// use omni::internal::config::parser::PathEntryConfig;
///
/// // Standard sources
/// let file_source = OmniSource::File("/etc/omni/config.yaml".into());
/// let prog_source = OmniSource::Programmatic;
///
/// // Custom package source
/// let pkg = PathEntryConfig { /* ... */ };
/// let pkg_source = OmniSource::Package(pkg);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OmniSource {
    /// Loaded from a file
    File(PathBuf),
    /// Loaded from environment variables
    Environment,
    /// Programmatically set
    Programmatic,
    /// Default value
    Default,
    /// Source is a package with associated metadata
    Package(PathEntryConfig),
}

impl compote::CustomSource for OmniSource {
    fn display_name(&self) -> String {
        match self {
            OmniSource::File(path) => path.display().to_string(),
            OmniSource::Environment => "environment".to_string(),
            OmniSource::Programmatic => "programmatic".to_string(),
            OmniSource::Default => "default".to_string(),
            OmniSource::Package(entry) => {
                format!("package:{}", entry.package.as_deref().unwrap_or("unknown"))
            }
        }
    }

    fn file_path(&self) -> Option<&Path> {
        match self {
            OmniSource::File(path) => Some(path.as_path()),
            OmniSource::Package(entry) => {
                if entry.full_path.is_empty() {
                    None
                } else {
                    Some(Path::new(&entry.full_path))
                }
            }
            _ => None,
        }
    }
}

// =============================================================================
// Type aliases for compote types
// =============================================================================

/// Source type - re-export of compote's built-in Source enum.
///
/// Use this for most source tracking. For package-aware tracking, use `OmniSource`.
pub type Source = compote::Source;

/// Level type - re-export of compote's built-in Level enum.
pub type Level = compote::Level;

/// Context type using built-in Source and Level.
pub type Context = compote::Context<Source, Level>;

/// ContextValue type using built-in Source and Level.
pub type ContextValue = compote::ContextValue<Source, Level>;

/// Context type with OmniSource for package tracking.
pub type OmniContext = compote::Context<OmniSource, Level>;

/// ContextValue type with OmniSource for package tracking.
pub type OmniContextValue = compote::ContextValue<OmniSource, Level>;

// =============================================================================
// Backward compatibility aliases (for gradual migration)
// =============================================================================

/// Alias for Source matching existing code patterns.
pub type CompoteConfigSource = Source;

/// Alias for Level matching existing code patterns.
pub type CompoteConfigLevel = Level;

/// Alias for Context matching existing code patterns.
pub type CompoteConfigContext = Context;

/// Alias for ContextValue matching existing code patterns.
pub type CompoteConfigValue = ContextValue;

/// Alias for Error matching existing code patterns.
pub type CompoteError = compote::Error;

/// Alias for ErrorTracker matching existing code patterns.
pub type CompoteErrorTracker = compote::ErrorTracker;

/// Alias for FromContextValue matching existing code patterns.
/// Note: FromContextValue is a trait, so we just re-export it.
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
    LevelType,
    SourceType,
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
