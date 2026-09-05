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
//! Package metadata is tracked by `PathEntryConfig`, not by the source type.

use std::path::Path;
use std::path::PathBuf;

use serde::Serialize;

// =============================================================================
// Custom Source Type
// =============================================================================

/// Custom source type for omni extending feuilletage's Source enum.
///
/// This type includes the standard source variants used by Omni.
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
}


impl feuilletage::CustomSource for OmniSource {
    fn display_name(&self) -> String {
        match self {
            OmniSource::File(path) => path.display().to_string(),
            OmniSource::Environment => "environment".to_string(),
            OmniSource::Programmatic => "programmatic".to_string(),
            OmniSource::Default => "default".to_string(),
        }
    }

    fn file_path(&self) -> Option<&Path> {
        match self {
            OmniSource::File(path) => Some(path.as_path()),
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
/// Use this for source tracking. Package metadata is kept on `PathEntryConfig`.
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

// =============================================================================
// Re-export feuilletage types that don't need generic parameters
// =============================================================================

pub use feuilletage::{
    // Values
    Value,
};
