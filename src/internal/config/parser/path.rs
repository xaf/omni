use std::fmt;
use std::path::PathBuf;

use crate::internal::config::parser::errors::ConfigErrorHandler;
use crate::internal::config::parser::errors::ConfigErrorKind;
use crate::internal::git::package_path_from_handle;
use crate::internal::git::package_root_path;

// ============================================================================
// NEW IMPLEMENTATION USING COMPOTE
// ============================================================================

/// PathConfig using compote's derive macro.
///
/// The compote::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from compote's Config
/// - `serde::Serialize` implementation for serialization
///
/// We still need manual `serde::Deserialize` for compatibility with the existing
/// codebase that uses serde for some operations.
#[derive(Debug, Clone, compote::Config)]
pub struct PathConfig {
    #[compote(default = "Vec::new()", skip_if_empty)]
    pub append: Vec<PathEntryConfig>,

    #[compote(default = "Vec::new()", skip_if_empty)]
    pub prepend: Vec<PathEntryConfig>,
}


impl Default for PathConfig {
    fn default() -> Self {
        Self {
            append: Vec::new(),
            prepend: Vec::new(),
        }
    }
}

/// PathEntryConfig using compote's derive macro.
///
/// The compote::Config derive macro automatically generates:
/// - `FromConfigValue` implementation for deserialization from compote's Config
/// - `serde::Serialize` implementation for serialization
///
/// We still need manual `serde::Deserialize` for compatibility with the existing
/// codebase that uses serde for some operations.
///
/// Note: The `full_path` field is computed and uses `#[compote(skip)]` to exclude
/// it from both serialization and deserialization.
#[derive(Debug, Clone, PartialEq, compote::Config)]
pub struct PathEntryConfig {
    #[compote(default = "String::new()")]
    pub path: String,

    #[compote(skip_if_empty)]
    pub package: Option<String>,

    #[compote(skip, default = "String::new()")]
    pub full_path: String,
}

impl fmt::Display for PathEntryConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.full_path)
    }
}

impl PathEntryConfig {
    pub fn from_path(path: &str) -> Self {
        Self {
            path: path.to_string(),
            package: None,
            full_path: if path.starts_with('/') {
                path.to_string()
            } else {
                "".to_string()
            },
        }
    }

    pub fn is_package(&self) -> bool {
        self.package.is_some() || PathBuf::from(&self.full_path).starts_with(package_root_path())
    }

    pub fn package_path(&self) -> Option<PathBuf> {
        if let Some(package) = &self.package {
            return package_path_from_handle(package);
        }

        None
    }

    pub fn is_valid(&self) -> bool {
        !self.full_path.is_empty() && self.full_path.starts_with('/')
    }

    pub fn starts_with(&self, path_entry: &PathEntryConfig) -> bool {
        if !self.is_valid() {
            return false;
        }

        PathBuf::from(&self.full_path).starts_with(&path_entry.full_path)
    }

    pub fn includes_path(&self, path: PathBuf) -> bool {
        if !self.is_valid() {
            return false;
        }

        PathBuf::from(&path).starts_with(&self.full_path)
    }

    pub fn replace(&mut self, path_from: &PathEntryConfig, path_to: &PathEntryConfig) -> bool {
        if self.starts_with(path_from) {
            let new_full_path = format!(
                "{}/{}",
                path_to.full_path,
                PathBuf::from(&self.full_path)
                    .strip_prefix(&path_from.full_path)
                    .unwrap()
                    .display(),
            );
            if let Some(package) = path_to.package.clone() {
                if let Some(package_path) = package_path_from_handle(&package) {
                    self.full_path = new_full_path;
                    self.package = Some(package);
                    self.path = PathBuf::from(&self.full_path)
                        .strip_prefix(&package_path)
                        .unwrap()
                        .display()
                        .to_string();

                    return true;
                }
            } else {
                self.full_path = new_full_path;
                self.package = None;
                self.path.clone_from(&self.full_path);

                return true;
            }
        }
        false
    }

    /// Convert to compote Value for use with compote's Config API
    pub fn to_compote_value(&self) -> compote::Value {
        if let Some(package) = &self.package {
            let mut map = indexmap::IndexMap::new();
            map.insert(
                "path".to_string(),
                compote::Value::String(self.path.clone()),
            );
            map.insert(
                "package".to_string(),
                compote::Value::String(package.clone()),
            );
            compote::Value::Object(map)
        } else {
            // Just the path as a string
            compote::Value::String(self.full_path.clone())
        }
    }

    /// Convert from compote ConfigValue to PathEntryConfig
    pub fn from_compote_value(
        config_value: &compote::ContextValue,
        error_handler: &ConfigErrorHandler,
    ) -> Option<Self> {
        match config_value {
            compote::ContextValue::Object(map, _) => {
                let path = map
                    .get("path")
                    .and_then(|v| match v {
                        compote::ContextValue::String(s, _) => Some(s.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();

                let package = map.get("package").and_then(|v| match v {
                    compote::ContextValue::String(s, _) => Some(s.clone()),
                    _ => None,
                });

                let absolute_path = path.starts_with('/');

                if let Some(package) = package {
                    if absolute_path {
                        error_handler
                            .with_key("package")
                            .error(ConfigErrorKind::UnsupportedValueInContext);
                        None
                    } else if let Some(package_path) = package_path_from_handle(&package) {
                        let mut full_path = package_path;
                        if !path.is_empty() {
                            full_path = full_path.join(path.clone());
                        }

                        Some(Self {
                            path: path.clone(),
                            package: Some(package.to_string()),
                            full_path: full_path.to_str().unwrap().to_string(),
                        })
                    } else {
                        error_handler
                            .with_key("package")
                            .with_context("package", package)
                            .error(ConfigErrorKind::InvalidPackage);
                        None
                    }
                } else {
                    Some(Self {
                        path: path.clone(),
                        package: None,
                        full_path: path,
                    })
                }
            }
            compote::ContextValue::String(path, _) => Some(Self {
                path: path.clone(),
                package: None,
                full_path: path.clone(),
            }),
            compote::ContextValue::Int(i, _) => Some(Self {
                path: i.to_string(),
                package: None,
                full_path: i.to_string(),
            }),
            _ => {
                error_handler
                    .with_expected(vec!["string", "object"])
                    .error(ConfigErrorKind::InvalidValueType);
                None
            }
        }
    }
}

impl Default for PathEntryConfig {
    fn default() -> Self {
        Self {
            path: String::new(),
            package: None,
            full_path: String::new(),
        }
    }
}
