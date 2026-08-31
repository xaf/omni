//! Feuilletage type aliases and custom types for omni.
//!
//! This module defines omni-specific source types and re-exports commonly used
//! feuilletage types for convenience.
//!
//! # Source Types
//!
//! Omni uses a custom source type [`OmniSource`] that extends feuilletage's built-in
//! sources with package tracking capability. This allows omni to track which
//! package a configuration value came from.
//!
//! # Usage
//!
//! Most code should use the type aliases defined here:
//! - [`Source`] - alias for `feuilletage::Source` (built-in sources)
//! - [`Level`] - alias for `feuilletage::Level` (built-in levels)
//! - [`Context`] - alias for `feuilletage::Context<Source, Level>`
//! - [`ContextValue`] - alias for `feuilletage::ContextValue<Source, Level>`
//!
//! For code that needs to track package sources, use `OmniSource` directly
//! with the appropriate generic parameters.

use std::path::Path;
use std::path::PathBuf;

use serde::Serialize;

use crate::internal::config::parser::PathEntryConfig;

// =============================================================================
// Custom Source Type
// =============================================================================

/// Custom source type for omni extending feuilletage's Source enum.
///
/// This type includes all the standard source variants (File, Environment,
/// Programmatic, Default) plus omni-specific variants like Package.
/// It implements `feuilletage::CustomSource` so it can be used as a source type
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
#[derive(Debug, Clone, Serialize, PartialEq)]
#[derive(Default)]
pub enum OmniSource {
    /// Loaded from a file
    File(PathBuf),
    /// Loaded from environment variables
    Environment,
    /// Programmatically set
    Programmatic,
    /// Default value
    #[default]
    Default,
    /// Source is a package with associated metadata
    Package(PathEntryConfig),
}


impl feuilletage::CustomSource for OmniSource {
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

    fn from_file(path: PathBuf) -> Self {
        OmniSource::File(path)
    }

    fn programmatic() -> Self {
        OmniSource::Programmatic
    }

    fn environment() -> Self {
        OmniSource::Environment
    }
}

// =============================================================================
// Type aliases for feuilletage types
// =============================================================================

/// Source type - re-export of feuilletage's built-in Source enum.
///
/// Use this for most source tracking. For package-aware tracking, use `OmniSource`.
pub type Source = feuilletage::Source;

/// Level type - re-export of feuilletage's built-in Level enum.
pub type Level = feuilletage::Level;

/// Context type using built-in Source and Level.
pub type Context = feuilletage::Context<Source, Level>;

/// ContextValue type using built-in Source and Level.
pub type ContextValue = feuilletage::ContextValue<Source, Level>;


// =============================================================================
// Backward compatibility aliases (for gradual migration)
// =============================================================================

/// Alias for Source matching existing code patterns.
pub type FeuilletageConfigSource = Source;

/// Alias for Level matching existing code patterns.
pub type FeuilletageConfigLevel = Level;

/// Alias for Context matching existing code patterns.
pub type FeuilletageConfigContext = Context;

/// Alias for ContextValue matching existing code patterns.
pub type FeuilletageConfigValue = ContextValue;

/// Alias for ErrorTracker matching existing code patterns.
pub type FeuilletageErrorTracker = feuilletage::ErrorTracker;

/// Re-export the context-value deserialization trait under Omni's naming convention.
pub use feuilletage::FromContextValue as FeuilletageFromContextValue;


// =============================================================================
// Re-export feuilletage types that don't need generic parameters
// =============================================================================

pub use feuilletage::{
    // Values
    Value,
    // Traits (for custom implementations)
    IsEmpty,
};
