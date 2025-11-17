use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::extend_strategy::ExtendStrategy;
use crate::scope::Scope;
use crate::source::Source;
use crate::transform::TransformFn;
use crate::value::ConfigValue;

/// Options for configuration operations
#[derive(Debug, Clone)]
pub struct Options {
    /// The extend/merge strategy to use
    pub extend_strategy: ExtendStrategy,
    /// Whether to apply transforms
    pub transform: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            extend_strategy: ExtendStrategy::Default,
            transform: true,
        }
    }
}

impl Options {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_extend_strategy(mut self, strategy: ExtendStrategy) -> Self {
        self.extend_strategy = strategy;
        self
    }

    pub fn with_transform(mut self, transform: bool) -> Self {
        self.transform = transform;
        self
    }
}

/// Builder for loading and configuring ConfigValue behavior
///
/// Allows customization of:
/// - Default extend strategy
/// - Key-specific extend strategies
/// - Transform functions
///
/// Configure once and reuse for multiple merges.
pub struct ConfigLoader<S: Source, C: Scope> {
    /// Default extend strategy
    default_extend_strategy: ExtendStrategy,
    /// Whether transforms are enabled
    transform_enabled: bool,
    /// Map of absolute keypaths to their extend strategies
    extend_strategy_overrides: HashMap<String, ExtendStrategy>,
    /// Transform function to apply to values
    transform_fn: Option<TransformFn<S, C>>,
}

impl<S: Source, C: Scope> Default for ConfigLoader<S, C> {
    fn default() -> Self {
        Self::new()
    }
}

/// Supported file formats for configuration files
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileFormat {
    /// YAML format (.yaml, .yml)
    Yaml,
    /// JSON format (.json)
    Json,
}

/// Builder for specifying files to load and merge
///
/// Provides a fluent API for configuring file loading parameters:
/// - File path (required)
/// - Source and Scope (optional, use defaults if not specified)
/// - Merge strategy (optional, uses loader's default if not specified)
/// - Whether file must exist (optional, defaults to optional/skip if not found)
///
/// # Example
/// ```ignore
/// use config_value::{FileDefinition, ExtendStrategy};
///
/// let files = vec![
///     // Minimal - uses default source and scope, optional file
///     FileDefinition::new("config.yaml"),
///
///     // With custom source, scope, and strategy
///     FileDefinition::new("paths.yaml")
///         .with_source(ConfigSource::User)
///         .with_scope(ConfigScope::User)
///         .with_strategy(ExtendStrategy::Append),
///
///     // Required file - error if not found
///     FileDefinition::new("required.yaml")
///         .require_exists(true),
/// ];
///
/// loader.load_and_merge_files(&mut config, files)?;
/// ```
#[derive(Debug, Clone)]
pub struct FileDefinition<'a, S: Source, C: Scope> {
    /// Path to the file to load
    path: &'a str,
    /// Source marker for the file (uses default if None)
    source: Option<S>,
    /// Scope marker for the file (uses default if None)
    scope: Option<C>,
    /// Optional merge strategy (uses loader's default if None)
    strategy: Option<ExtendStrategy>,
    /// Whether the file is required to exist (default: false)
    require_exists: bool,
    /// Whether the file is required to be valid (default: true)
    require_valid: bool,
}

impl<'a, S: Source + Default, C: Scope + Default> FileDefinition<'a, S, C> {
    /// Create a new FileDefinition with only the required file path
    ///
    /// # Arguments
    /// * `path` - Path to the configuration file
    ///
    /// By default:
    /// - Uses `ConfigSource::default()` for source
    /// - Uses `ConfigScope::default()` for scope
    /// - Uses the loader's default merge strategy
    /// - Does not require file to exist (skips if not found)
    /// - Requires file to be valid if found (error on parse failure)
    pub fn new(path: &'a str) -> Self {
        Self {
            path,
            source: None,
            scope: None,
            strategy: None,
            require_exists: false,
            require_valid: true,
        }
    }

    /// Specify a source for this file
    ///
    /// If not specified, uses `ConfigSource::default()`.
    pub fn with_source(mut self, source: S) -> Self {
        self.source = Some(source);
        self
    }

    /// Specify a scope for this file
    ///
    /// If not specified, uses `ConfigScope::default()`.
    pub fn with_scope(mut self, scope: C) -> Self {
        self.scope = Some(scope);
        self
    }

    /// Specify a merge strategy for this file
    ///
    /// If not specified, uses the loader's default strategy.
    pub fn with_strategy(mut self, strategy: ExtendStrategy) -> Self {
        self.strategy = Some(strategy);
        self
    }

    /// Control whether this file must exist
    ///
    /// # Arguments
    /// * `require_exists` - If true, return error if file not found; if false (default), skip missing files
    ///
    /// By default, files are optional and will be skipped if not found.
    ///
    /// # Examples
    /// ```ignore
    /// // Make file required
    /// FileDefinition::new("config.yaml").require_exists(true)
    ///
    /// // Make file optional (default behavior, usually not needed)
    /// FileDefinition::new("config.yaml").require_exists(false)
    ///
    /// // Conditional based on logic
    /// let is_required = check_environment();
    /// FileDefinition::new("config.yaml").require_exists(is_required)
    /// ```
    #[allow(clippy::wrong_self_convention)]
    pub fn require_exists(mut self, require_exists: bool) -> Self {
        self.require_exists = require_exists;
        self
    }

    /// Control whether this file must be valid (parseable)
    ///
    /// # Arguments
    /// * `require_valid` - If true (default), return error if file is invalid; if false, skip invalid files
    ///
    /// By default, files must be valid. If a file exists but cannot be parsed, an error is returned.
    /// Use `require_valid(false)` to silently skip files with parse errors.
    ///
    /// # Examples
    /// ```ignore
    /// // Require file to be valid (default behavior, usually not needed)
    /// FileDefinition::new("config.yaml").require_valid(true)
    ///
    /// // Allow invalid files to be skipped
    /// FileDefinition::new("config.yaml").require_valid(false)
    ///
    /// // Try to load optional config, but ignore if corrupted
    /// FileDefinition::new("optional.yaml")
    ///     .require_exists(false)  // Skip if missing
    ///     .require_valid(false)   // Skip if invalid
    /// ```
    #[allow(clippy::wrong_self_convention)]
    pub fn require_valid(mut self, require_valid: bool) -> Self {
        self.require_valid = require_valid;
        self
    }

    /// Get the file path
    pub fn path(&self) -> &str {
        self.path
    }

    /// Get the source (returns default if not specified)
    pub fn source(&self) -> S {
        self.source.clone().unwrap_or_default()
    }

    /// Get the scope (returns default if not specified)
    pub fn scope(&self) -> C {
        self.scope.clone().unwrap_or_default()
    }

    /// Get the strategy (None means use loader's default)
    pub fn strategy(&self) -> Option<&ExtendStrategy> {
        self.strategy.as_ref()
    }

    /// Check if file is required to exist
    pub fn is_require_exists(&self) -> bool {
        self.require_exists
    }

    /// Check if file is required to be valid
    pub fn is_require_valid(&self) -> bool {
        self.require_valid
    }
}

impl FileFormat {
    /// Detect format from file extension
    ///
    /// Returns None if extension is not recognized.
    pub fn from_extension(path: &str) -> Option<Self> {
        let path_lower = path.to_lowercase();
        if path_lower.ends_with(".yaml") || path_lower.ends_with(".yml") {
            Some(FileFormat::Yaml)
        } else if path_lower.ends_with(".json") {
            Some(FileFormat::Json)
        } else {
            None
        }
    }

    /// Try to detect format from file content
    ///
    /// Attempts to parse the content as YAML first, then JSON.
    /// Returns None if neither format can parse the content.
    pub fn from_content(content: &str) -> Option<Self> {
        // Try YAML first (more common for config files, also accepts JSON)
        if crate::Value::from_yaml_str(content).is_ok() {
            return Some(FileFormat::Yaml);
        }
        // Try JSON
        if crate::Value::from_json_str(content).is_ok() {
            return Some(FileFormat::Json);
        }
        None
    }

    /// Parse content with this format
    pub fn parse<S: Source + Clone, C: Scope + Clone>(
        &self,
        content: &str,
        source: S,
        scope: C,
    ) -> Result<crate::ConfigValue<S, C>, Box<dyn std::error::Error>> {
        let value = match self {
            FileFormat::Yaml => crate::Value::from_yaml_str(content)?,
            FileFormat::Json => crate::Value::from_json_str(content)?,
        };
        Ok(crate::ConfigValue::from_config_value(source, scope, value))
    }

    /// Serialize a ConfigValue with this format
    pub fn serialize<S: Source, C: Scope>(&self, value: &crate::ConfigValue<S, C>) -> String {
        match self {
            FileFormat::Yaml => value.as_yaml(),
            FileFormat::Json => value.as_json(),
        }
    }
}

impl<S: Source, C: Scope> ConfigLoader<S, C> {
    /// Create a new ConfigLoader with default settings
    pub fn new() -> Self {
        Self {
            default_extend_strategy: ExtendStrategy::Default,
            transform_enabled: true,
            extend_strategy_overrides: HashMap::new(),
            transform_fn: None,
        }
    }

    /// Set the default extend strategy
    pub fn with_default_extend_strategy(mut self, strategy: ExtendStrategy) -> Self {
        self.default_extend_strategy = strategy;
        self
    }

    /// Enable or disable transforms
    pub fn with_transform_enabled(mut self, enabled: bool) -> Self {
        self.transform_enabled = enabled;
        self
    }

    /// Override the extend strategy for a specific keypath
    ///
    /// # Arguments
    /// * `keypath` - Dot-separated absolute path (e.g., "path.append")
    /// * `strategy` - The strategy to use for this keypath
    ///
    /// # Example
    /// ```ignore
    /// loader.with_extend_strategy_override("path.append", ExtendStrategy::Append);
    /// ```
    pub fn with_extend_strategy_override(
        mut self,
        keypath: &str,
        strategy: ExtendStrategy,
    ) -> Self {
        self.extend_strategy_overrides
            .insert(keypath.to_string(), strategy);
        self
    }

    /// Set the transform function to apply to values
    pub fn with_transform(mut self, transform: TransformFn<S, C>) -> Self {
        self.transform_fn = Some(transform);
        self
    }

    /// Get the extend strategy override for a keypath
    pub fn get_extend_strategy(&self, keypath: &[String]) -> Option<&ExtendStrategy> {
        let keypath_str = keypath.join(".");
        self.extend_strategy_overrides.get(&keypath_str)
    }

    /// Apply transform to a value if configured
    pub fn apply_transform(&self, value: &mut ConfigValue<S, C>, keypath: &[String]) {
        if self.transform_enabled {
            if let Some(transform) = self.transform_fn {
                transform(value, keypath);
            }
        }
    }

    /// Merge another ConfigValue into the base using this loader's configuration
    ///
    /// # Arguments
    /// * `base` - The base configuration to merge into
    /// * `other` - The new configuration to merge
    pub fn merge(&self, base: &mut ConfigValue<S, C>, other: ConfigValue<S, C>) {
        // Perform the merge
        base.extend(other, self.default_extend_strategy.clone());

        // Apply transforms if enabled
        if self.transform_enabled && self.transform_fn.is_some() {
            self.apply_transforms_recursive(base, &vec![]);
        }
    }

    /// Recursively apply transforms to a ConfigValue tree
    fn apply_transforms_recursive(&self, value: &mut ConfigValue<S, C>, keypath: &Vec<String>) {
        // Apply transform to current value
        self.apply_transform(value, keypath);

        // Recursively apply to children
        if let Some(mapping) = value.as_table() {
            for (key, _) in mapping {
                let mut child_keypath = keypath.clone();
                child_keypath.push(key.clone());

                if let Some(child) = value.get_mut(&key) {
                    self.apply_transforms_recursive(child, &child_keypath);
                }
            }
        } else if let Some(array) = value.as_array() {
            for index in 0..array.len() {
                let mut child_keypath = keypath.clone();
                child_keypath.push(index.to_string());

                if let Some(child) = value.as_array_mut().and_then(|arr| arr.get_mut(index)) {
                    self.apply_transforms_recursive(child, &child_keypath);
                }
            }
        }
    }

    /// Create Options from this loader's configuration
    pub fn options(&self) -> Options {
        Options {
            extend_strategy: self.default_extend_strategy.clone(),
            transform: self.transform_enabled,
        }
    }

    /// Load a file with auto-detected format
    ///
    /// This function:
    /// 1. Auto-detects format (YAML/JSON) from file extension and content
    /// 2. Parses the file content
    /// 3. Returns a ConfigValue
    ///
    /// Note: This does NOT apply transformations or merge strategies.
    /// Use this for loading a single file in isolation.
    ///
    /// # Arguments
    /// * `file_path` - Path to the configuration file
    /// * `source` - Source marker for the loaded config
    /// * `scope` - Scope marker for the loaded config
    ///
    /// # Example
    /// ```ignore
    /// let config = loader.load_file(
    ///     "/path/to/config.yaml",
    ///     ConfigSource::User,
    ///     ConfigScope::User,
    /// )?;
    /// ```
    pub fn load_file(&self, file_path: &str, source: S, scope: C) -> io::Result<ConfigValue<S, C>>
    where
        S: Clone,
        C: Clone,
    {
        use std::fs;

        // Read file content
        let contents = fs::read_to_string(file_path)?;

        // Detect format
        let format = if contents.is_empty() {
            // Empty file - use extension or default to YAML
            FileFormat::from_extension(file_path).unwrap_or(FileFormat::Yaml)
        } else {
            // Try extension first, then content
            FileFormat::from_extension(file_path)
                .or_else(|| FileFormat::from_content(&contents))
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "Unable to detect config format")
                })?
        };

        // Parse with detected format
        if contents.is_empty() {
            Ok(ConfigValue::new_null_with(source, scope))
        } else {
            format.parse(&contents, source, scope).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Failed to parse config as {:?}: {}", format, e),
                )
            })
        }
    }

    /// Load a file and merge it into a base configuration using the loader's default strategy
    ///
    /// This function:
    /// 1. Auto-detects and loads the file (using `load_file`)
    /// 2. Merges it into the base configuration using the loader's default extend strategy
    /// 3. Applies transforms if enabled
    ///
    /// # Arguments
    /// * `base` - The base configuration to merge into
    /// * `file_path` - Path to the configuration file
    /// * `source` - Source marker for the loaded config
    /// * `scope` - Scope marker for the loaded config
    ///
    /// # Example
    /// ```ignore
    /// let mut config = ConfigValue::new_null(ConfigSource::Default, ConfigScope::Default);
    /// loader.load_and_merge_file(
    ///     &mut config,
    ///     "/path/to/config.yaml",
    ///     ConfigSource::User,
    ///     ConfigScope::User,
    /// )?;
    /// ```
    pub fn load_and_merge_file(
        &self,
        base: &mut ConfigValue<S, C>,
        file_path: &str,
        source: S,
        scope: C,
    ) -> io::Result<()>
    where
        S: Clone,
        C: Clone,
    {
        // Load file with auto-detection
        let loaded = self.load_file(file_path, source, scope)?;

        // Merge into base using loader's configuration
        self.merge(base, loaded);

        Ok(())
    }

    /// Load a file and merge it into a base configuration using a specific strategy
    ///
    /// This function:
    /// 1. Auto-detects and loads the file (using `load_file`)
    /// 2. Merges it into the base configuration using the specified extend strategy
    /// 3. Applies transforms if enabled
    ///
    /// This allows overriding the loader's default strategy for this specific file.
    ///
    /// # Arguments
    /// * `base` - The base configuration to merge into
    /// * `file_path` - Path to the configuration file
    /// * `source` - Source marker for the loaded config
    /// * `scope` - Scope marker for the loaded config
    /// * `strategy` - The extend strategy to use for this merge
    ///
    /// # Example
    /// ```ignore
    /// let mut config = ConfigValue::new_null(ConfigSource::Default, ConfigScope::Default);
    /// loader.load_and_merge_file_with_strategy(
    ///     &mut config,
    ///     "/path/to/config.yaml",
    ///     ConfigSource::User,
    ///     ConfigScope::User,
    ///     ExtendStrategy::Raw,
    /// )?;
    /// ```
    pub fn load_and_merge_file_with_strategy(
        &self,
        base: &mut ConfigValue<S, C>,
        file_path: &str,
        source: S,
        scope: C,
        strategy: ExtendStrategy,
    ) -> io::Result<()>
    where
        S: Clone,
        C: Clone,
    {
        // Load file with auto-detection
        let loaded = self.load_file(file_path, source, scope)?;

        // Merge into base using the specified strategy
        base.extend(loaded, strategy);

        // Apply transforms if enabled
        if self.transform_enabled && self.transform_fn.is_some() {
            self.apply_transforms_recursive(base, &vec![]);
        }

        Ok(())
    }

    /// Load multiple files and merge them into a base configuration
    ///
    /// This function:
    /// 1. Loads each file in order (using `load_file`)
    /// 2. Merges each into the base configuration using its specified strategy
    ///    (or the loader's default if not specified)
    /// 3. Applies transforms if enabled (after each merge)
    /// 4. Handles missing files according to each file's configuration
    ///    (by default, missing files are skipped; use `require_exists()` to make them mandatory)
    /// 5. Returns a list of successfully loaded file paths
    ///
    /// Files are processed in order, so later files override earlier ones according to the merge strategy.
    ///
    /// # Arguments
    /// * `base` - The base configuration to merge into
    /// * `files` - Iterator of `FileDefinition` builders specifying files to load
    ///
    /// # Returns
    /// A vector of file paths that were successfully loaded and merged
    ///
    /// # Example
    /// ```ignore
    /// use config_value::{FileDefinition, ExtendStrategy};
    ///
    /// let mut config = ConfigValue::new_null(ConfigSource::Default, ConfigScope::Default);
    /// let loaded = loader.load_and_merge_files(
    ///     &mut config,
    ///     vec![
    ///         // Required system config
    ///         FileDefinition::new("/etc/app/config.yaml")
    ///             .with_source(ConfigSource::System)
    ///             .with_scope(ConfigScope::System)
    ///             .require_exists(true),
    ///         // Optional path config with append strategy
    ///         FileDefinition::new("/etc/app/paths.yaml")
    ///             .with_source(ConfigSource::System)
    ///             .with_scope(ConfigScope::System)
    ///             .with_strategy(ExtendStrategy::Append),
    ///         // Optional user config (default behavior)
    ///         FileDefinition::new("/home/user/.config/app.yaml")
    ///             .with_source(ConfigSource::User)
    ///             .with_scope(ConfigScope::User),
    ///     ],
    /// )?;
    ///
    /// println!("Loaded {} config files", loaded.len());
    /// ```
    pub fn load_and_merge_files<'a>(
        &self,
        base: &mut ConfigValue<S, C>,
        files: impl IntoIterator<Item = FileDefinition<'a, S, C>>,
    ) -> io::Result<Vec<String>>
    where
        S: Clone + Default + 'a,
        C: Clone + Default + 'a,
    {
        use std::path::Path;

        let mut loaded_files = Vec::new();

        for file_spec in files {
            let file_path = file_spec.path();

            // Check if file exists
            if !Path::new(file_path).exists() {
                if !file_spec.is_require_exists() {
                    // Skip missing file
                    continue;
                } else {
                    // Return error for missing file
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("Configuration file not found: {}", file_path),
                    ));
                }
            }

            // Load and merge this file with its specific strategy (or default)
            let result = if let Some(strategy) = file_spec.strategy() {
                self.load_and_merge_file_with_strategy(
                    base,
                    file_path,
                    file_spec.source(),
                    file_spec.scope(),
                    strategy.clone(),
                )
            } else {
                self.load_and_merge_file(
                    base,
                    file_path,
                    file_spec.source(),
                    file_spec.scope(),
                )
            };

            // Handle load/parse errors based on require_valid setting
            if let Err(err) = result {
                if !file_spec.is_require_valid() {
                    // Skip invalid file silently
                    continue;
                } else {
                    // Return error for invalid file
                    return Err(err);
                }
            }

            // Track successfully loaded file
            loaded_files.push(file_path.to_string());
        }

        Ok(loaded_files)
    }

    /// Edit a configuration file with auto-detected format
    ///
    /// This function:
    /// 1. Auto-detects format (YAML/JSON) from file extension and content
    /// 2. Creates parent directories if needed
    /// 3. Opens the file with exclusive lock (using fs4 crate on Unix)
    /// 4. Loads the existing configuration WITHOUT transformations or merge strategies
    /// 5. Calls the edit function with mutable access to the config
    /// 6. If edit function returns true, serializes and writes back to file in the same format
    ///
    /// Note: This loads the file in isolation without applying transformations.
    /// This is appropriate for editing a single file directly.
    ///
    /// Supported formats:
    /// - YAML: .yaml, .yml extensions
    /// - JSON: .json extension
    ///
    /// If extension is ambiguous, tries to parse as YAML first, then JSON.
    ///
    /// # Arguments
    /// * `file_path` - Path to the configuration file
    /// * `source` - Source marker for the loaded config
    /// * `scope` - Scope marker for the loaded config
    /// * `edit_fn` - Function to edit the config (returns true to save)
    ///
    /// # Example
    /// ```ignore
    /// loader.edit_file(
    ///     "/path/to/config.yaml",
    ///     ConfigSource::User,
    ///     ConfigScope::User,
    ///     |config| {
    ///         config.set("key", "value");
    ///         true // save changes
    ///     }
    /// )?;
    /// ```
    #[cfg(unix)]
    pub fn edit_file<EditFn>(
        &self,
        file_path: &str,
        source: S,
        scope: C,
        edit_fn: EditFn,
    ) -> io::Result<()>
    where
        EditFn: FnOnce(&mut ConfigValue<S, C>) -> bool,
        S: Clone,
        C: Clone,
    {
        use fs4::fs_std::FileExt;

        // Create parent directories if needed
        let file_path_obj = Path::new(file_path);
        if let Some(parent) = file_path_obj.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // Open file with read/write access, create if doesn't exist
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(file_path)?;

        // Take exclusive lock (released when file goes out of scope)
        let _lock = file.lock_exclusive();

        // Read existing content
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        // Detect format
        let format = if contents.is_empty() {
            // Empty file - use extension or default to YAML
            FileFormat::from_extension(file_path).unwrap_or(FileFormat::Yaml)
        } else {
            // Try extension first, then content
            FileFormat::from_extension(file_path)
                .or_else(|| FileFormat::from_content(&contents))
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Unable to detect config format (tried YAML and JSON)",
                    )
                })?
        };

        // Load configuration WITHOUT transformations
        let mut config_value = if contents.is_empty() {
            // Empty file, create null config
            ConfigValue::new_null_with(source, scope)
        } else {
            // Parse existing content with detected format
            format
                .parse(&contents, source.clone(), scope.clone())
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Failed to parse config as {:?}: {}", format, e),
                    )
                })?
        };

        // Call edit function
        if edit_fn(&mut config_value) {
            // Serialize the updated config in the same format
            let serialized = format.serialize(&config_value);

            // Write back to file
            file.set_len(0)?;
            file.seek(SeekFrom::Start(0))?;
            file.write_all(serialized.as_bytes())?;
        }

        Ok(())
    }

    /// Edit a configuration file (non-Unix platforms - no file locking)
    ///
    /// Same as `edit_file` but without file locking support.
    /// On non-Unix platforms, file locking is not available.
    #[cfg(not(unix))]
    pub fn edit_file<EditFn>(
        &self,
        file_path: &str,
        source: S,
        scope: C,
        edit_fn: EditFn,
    ) -> io::Result<()>
    where
        EditFn: FnOnce(&mut ConfigValue<S, C>) -> bool,
        S: Clone,
        C: Clone,
    {
        // Create parent directories if needed
        let file_path_obj = Path::new(file_path);
        if let Some(parent) = file_path_obj.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // Open file with read/write access, create if doesn't exist
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(file_path)?;

        // Read existing content
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        // Detect format
        let format = if contents.is_empty() {
            // Empty file - use extension or default to YAML
            FileFormat::from_extension(file_path).unwrap_or(FileFormat::Yaml)
        } else {
            // Try extension first, then content
            FileFormat::from_extension(file_path)
                .or_else(|| FileFormat::from_content(&contents))
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Unable to detect config format (tried YAML and JSON)",
                    )
                })?
        };

        // Load configuration WITHOUT transformations
        let mut config_value = if contents.is_empty() {
            // Empty file, create null config
            ConfigValue::new_null_with(source, scope)
        } else {
            // Parse existing content with detected format
            format
                .parse(&contents, source.clone(), scope.clone())
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Failed to parse config as {:?}: {}", format, e),
                    )
                })?
        };

        // Call edit function
        if edit_fn(&mut config_value) {
            // Serialize the updated config in the same format
            let serialized = format.serialize(&config_value);

            // Write back to file
            file.set_len(0)?;
            file.seek(SeekFrom::Start(0))?;
            file.write_all(serialized.as_bytes())?;
        }

        Ok(())
    }
}
#[cfg(test)]
mod test_file_definition_api {
    use super::FileDefinition;
    use crate::source::DefaultSource;
    use crate::scope::DefaultScope;

    #[test]
    fn test_require_exists_true() {
        let f = FileDefinition::<DefaultSource, DefaultScope>::new("test.yaml")
            .require_exists(true);
        assert!(f.is_require_exists());
    }

    #[test]
    fn test_require_exists_false() {
        let f = FileDefinition::<DefaultSource, DefaultScope>::new("test.yaml")
            .require_exists(false);
        assert!(!f.is_require_exists());
    }

    #[test]
    fn test_default_exists_is_optional() {
        let f = FileDefinition::<DefaultSource, DefaultScope>::new("test.yaml");
        assert!(!f.is_require_exists());
    }

    #[test]
    fn test_require_exists_conditional() {
        let is_production = true;
        let f = FileDefinition::<DefaultSource, DefaultScope>::new("test.yaml")
            .require_exists(is_production);
        assert!(f.is_require_exists());

        let is_production = false;
        let f = FileDefinition::<DefaultSource, DefaultScope>::new("test.yaml")
            .require_exists(is_production);
        assert!(!f.is_require_exists());
    }

    #[test]
    fn test_require_valid_true() {
        let f = FileDefinition::<DefaultSource, DefaultScope>::new("test.yaml")
            .require_valid(true);
        assert!(f.is_require_valid());
    }

    #[test]
    fn test_require_valid_false() {
        let f = FileDefinition::<DefaultSource, DefaultScope>::new("test.yaml")
            .require_valid(false);
        assert!(!f.is_require_valid());
    }

    #[test]
    fn test_default_valid_is_required() {
        let f = FileDefinition::<DefaultSource, DefaultScope>::new("test.yaml");
        assert!(f.is_require_valid());
    }

    #[test]
    fn test_both_require_settings() {
        // Most strict: must exist and be valid
        let f = FileDefinition::<DefaultSource, DefaultScope>::new("test.yaml")
            .require_exists(true)
            .require_valid(true);
        assert!(f.is_require_exists());
        assert!(f.is_require_valid());

        // Most lenient: optional and can be invalid
        let f = FileDefinition::<DefaultSource, DefaultScope>::new("test.yaml")
            .require_exists(false)
            .require_valid(false);
        assert!(!f.is_require_exists());
        assert!(!f.is_require_valid());

        // Mixed: must exist but can be invalid
        let f = FileDefinition::<DefaultSource, DefaultScope>::new("test.yaml")
            .require_exists(true)
            .require_valid(false);
        assert!(f.is_require_exists());
        assert!(!f.is_require_valid());
    }

    #[test]
    fn test_load_tracking() {
        use super::ConfigLoader;
        use crate::value::ConfigValue;
        use std::fs;
        use std::io::Write;

        // Create temp directory and files
        let temp_dir = std::env::temp_dir().join("config_value_test_tracking");
        fs::create_dir_all(&temp_dir).unwrap();

        let file1 = temp_dir.join("config1.yaml");
        let file2 = temp_dir.join("config2.yaml");
        let file3 = temp_dir.join("missing.yaml");

        // Write test files
        fs::File::create(&file1)
            .unwrap()
            .write_all(b"key1: value1")
            .unwrap();
        fs::File::create(&file2)
            .unwrap()
            .write_all(b"key2: value2")
            .unwrap();
        // file3 intentionally not created

        let loader = ConfigLoader::<DefaultSource, DefaultScope>::new();
        let mut config = ConfigValue::new_null();

        let loaded = loader
            .load_and_merge_files(
                &mut config,
                vec![
                    FileDefinition::new(file1.to_str().unwrap()),
                    FileDefinition::new(file2.to_str().unwrap()),
                    FileDefinition::new(file3.to_str().unwrap()), // Missing, but optional
                ],
            )
            .unwrap();

        // Should have loaded 2 files (file3 was skipped)
        assert_eq!(loaded.len(), 2);
        assert!(loaded.contains(&file1.to_string_lossy().to_string()));
        assert!(loaded.contains(&file2.to_string_lossy().to_string()));
        assert!(!loaded.contains(&file3.to_string_lossy().to_string()));

        // Cleanup
        fs::remove_dir_all(&temp_dir).ok();
    }
}
