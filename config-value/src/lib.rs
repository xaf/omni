//! Generic configuration value system with source tracking and merge strategies
//!
//! This crate provides a flexible configuration system that tracks where values come from
//! and how they should be merged together. It's designed to be generic over both the
//! source type (S) and scope type (C), allowing applications to define their own
//! source and scope tracking.
//!
//! # Key Features
//!
//! - **Generic source tracking**: Track where configuration values come from using your own types
//! - **Generic scope tracking**: Track what scope a value applies to using your own types
//! - **Extension strategies**: Built-in merge strategies (append, prepend, replace, keep, raw)
//! - **Transform pipeline**: Support for transforming values (e.g., path resolution)
//! - **Type-safe**: Full serde support for serialization/deserialization
//!
//! # Example
//!
//! ```rust,ignore
//! use config_value::{ConfigValue, Source, Scope, ExtendStrategy};
//!
//! // Define your own source type
//! #[derive(Debug, Clone, PartialEq)]
//! enum MySource {
//!     File(String),
//!     Env,
//!     Default,
//! }
//!
//! impl Source for MySource {
//!     fn priority(&self) -> u32 {
//!         match self {
//!             MySource::Default => 0,
//!             MySource::File(_) => 10,
//!             MySource::Env => 20,
//!         }
//!     }
//! }
//!
//! // Use ConfigValue with your source type
//! let value: ConfigValue<MySource> = ConfigValue::new(
//!     42,
//!     MySource::File("config.yaml".into()),
//!     MyScope::default(),
//! );
//! ```

pub mod error;
pub mod error_handler;
pub mod extend_strategy;
pub mod loader;
pub mod primitive;
pub mod scope;
pub mod source;
pub mod transform;
pub mod value;

pub use error::{ConfigError, ConfigErrorKind};
pub use error_handler::{ErrorHandler, NoOpErrorHandler};
pub use extend_strategy::ExtendStrategy;
pub use loader::{ConfigLoader, FileDefinition, FileFormat, Options};
pub use primitive::Value;
pub use scope::Scope;
pub use source::Source;
pub use transform::TransformFn;
pub use value::{ConfigData, ConfigValue};
