# Omni Config Migration to Feuilletage

This document tracks the migration of omni's configuration parsing from the legacy `ConfigValue` system to native feuilletage.

## Migration Status

### Completed Conversions (using feuilletage derive macros)
- `askpass.rs` - AskPassConfig
- `cd.rs` - CdConfig
- `check.rs` - CheckConfig, CheckPattern
- `clone.rs` - CloneConfig
- `config_commands.rs` - ConfigCommandsConfig
- `github.rs` - GithubConfig, StringFilter
- `makefile_commands.rs` - MakefileCommandsConfig
- `match_skip_prompt_if_config.rs` - MatchSkipPromptIfConfig
- `org.rs` - OrgConfig
- `path.rs` - PathConfig
- `path_repo_updates.rs` - PathRepoUpdatesConfig
- `suggest_config.rs` - SuggestConfig
- `up_command.rs` - UpCommandConfig
- `cache/*.rs` - Various cache config structs
- **`omniconfig.rs` - OmniConfig (main config container) - NOW USES DERIVE MACRO**

### Completed Native Feuilletage Conversions (FromConfigValue trait)
- [x] `env.rs` - EnvConfig, EnvOperationConfig, EnvOperationEnum
- [x] `omniconfig.rs` - OmniConfig (main config container) - **MIGRATED TO DERIVE MACRO**
- [x] `suggest_clone.rs` - SuggestCloneConfig, SuggestCloneRepositoryConfig, SuggestCloneTypeEnum
- [x] `check.rs` - CheckConfig, CheckPattern (native FromConfigValue)
- [x] `prompts.rs` - PromptsConfig, PromptConfig, PromptType, PromptScope, PromptChoicesConfig, PromptChoiceConfig
- [x] `shell_aliases.rs` - ShellAliasesConfig (container - uses derive-generated FromConfigValue for ShellAliasConfig)
- [x] `up/base.rs` - UpConfig (bridges to existing parsing for UpConfigTool)
- [x] `command_definition.rs` - CommandDefinition (bridges to existing parsing)

### Types Using Bridge Pattern
These types have `FromConfigValue` but bridge to existing parsing for complex sub-types:
- `up/base.rs` - UpConfig bridges to existing UpConfigTool parsing
- `command_definition.rs` - CommandDefinition bridges to existing parsing

## Conversion Notes

### General Approach
1. Replace `use crate::internal::config::ConfigValue` with feuilletage types
2. Replace `ConfigErrorHandler` usage with feuilletage's `ErrorTracker`
3. Implement `FromConfigValue` trait for complex custom parsing
4. Use `#[derive(feuilletage::Config)]` for simpler structs
5. Maintain serde compatibility for cache serialization

### Bridge Pattern for Backward Compatibility

For types with complex parsing that implement `feuilletage::FromConfigValue`, we use a bridge method to maintain backward compatibility with the existing code that uses the old `ConfigValue` type:

```rust
impl MyConfig {
    /// Bridge method for backward compatibility with old ConfigValue type
    pub(crate) fn from_config_value(
        config_value: Option<ConfigValue>,
        _error_handler: &ConfigErrorHandler,
    ) -> Self {
        // Convert old ConfigValue to feuilletage ConfigValue
        // Call the feuilletage FromConfigValue implementation
    }
}

impl feuilletage::FromConfigValue for MyConfig {
    fn from_config_value(
        value: &feuilletage::ConfigValue,
        tracker: &mut feuilletage::ErrorTracker,
    ) -> Result<Self, feuilletage::ConfigError> {
        // Native feuilletage implementation
    }
}
```

### Conversion Helper Functions

The `omniconfig.rs` module includes helper functions to convert between old and new types:
- `convert_feuilletage_to_old_config_value()` - Converts `feuilletage::ConfigValue` to old `ConfigValue`
- `convert_feuilletage_to_config_data()` - Converts feuilletage value to `ConfigData` with proper nesting
- `reject_local_scope()` - Filters out Local (Workdir) scope values (for org config)

The `suggest_clone.rs` module includes:
- `select_local_scope()` - Selects only Local (Workdir) scope values (opposite of reject)

The `env.rs` module includes:
- `convert_to_feuilletage_value()` - Converts old `ConfigValue` to `feuilletage::ConfigValue`
- `convert_to_feuilletage_inner_value()` - Converts the inner value
- `convert_to_feuilletage_context()` - Converts source/scope to feuilletage's `ConfigContext`

### Fully Qualified Syntax for Trait Methods

When a struct has both a manual `from_config_value` method (bridge) and a `FromConfigValue` trait implementation, use fully qualified syntax to call the trait:

```rust
// Instead of:
let result = MyConfig::from_config_value(v, tracker)?;

// Use:
let result = <MyConfig as FeuilletageFromConfigValue>::from_config_value(v, tracker)?;
```

---

## File-by-File Conversion Details

### env.rs
Status: **COMPLETED**

**Implementation:**
- Implemented `feuilletage::FromConfigValue` for `EnvConfig`
- Added helper methods on `EnvOperationConfig` for parsing
- Bridge method converts old `ConfigValue` to feuilletage types
- Uses feuilletage's `ErrorTracker` for error handling
- Path resolution uses `feuilletage::ConfigSource::File` for source path

**Key features:**
- Supports both array and table formats at top level
- Table format is sorted by key for deterministic output
- Supports multiple operations per entry (set, prepend, append, remove, prefix, suffix)
- Path type values are resolved relative to config file location
- Null values supported for "set" operation to unset variables

### omniconfig.rs
Status: **COMPLETED** (manual FromConfigValue, ready for derive macro once CommandDefinition is converted)

**Implementation:**
- Implemented `feuilletage::FromConfigValue` for `OmniConfig`
- Uses fully qualified syntax to call trait implementations
- All sub-types now have `FromConfigValue` except `CommandDefinition`
- Handles special cases:
  - `commands`: HashMap iteration with `CommandDefinition` (still uses old API via conversion)
  - `org`: Scope rejection (reject Local/Workdir scope) - can use `mutable_by = ["system", "user"]` once converted to derive
  - Scalar fields with lazy_static defaults - can use `default_fn` once converted to derive

**Sub-parsers by implementation type:**
- **Uses feuilletage FromConfigValue (via derive):** AskPassConfig, CacheConfig, CdConfig, CloneConfig, ConfigCommandsConfig, EnvConfig, GithubConfig, MakefileCommandsConfig, MatchSkipPromptIfConfig, OrgConfig, PathConfig, PathRepoUpdatesConfig, SuggestConfig, SuggestCloneConfig, UpCommandConfig
- **Uses feuilletage FromConfigValue (native impl):** CheckConfig, PromptsConfig, ShellAliasesConfig, UpConfig
- **Uses old API (pending conversion):** CommandDefinition

**Ready for derive macro conversion:**
Once `CommandDefinition` has `FromConfigValue`, `OmniConfig` can be converted to use `#[derive(feuilletage::Config)]` with:
- `#[feuilletage(mutable_by = ["system", "user"])]` for `org` field
- `#[feuilletage(default_fn = "get_default_sandbox")]` for `sandbox`
- `#[feuilletage(default_fn = "get_default_worktree")]` for `worktree`
- `#[feuilletage(absolute_path)]` for `sandbox` and `worktree` to ensure proper path handling

### suggest_clone.rs
Status: **COMPLETED**

**Implementation:**
- Implemented `feuilletage::FromConfigValue` for:
  - `SuggestCloneConfig`
  - `SuggestCloneRepositoryConfig`
  - `SuggestCloneTypeEnum`
- Added `select_local_scope()` helper (opposite of reject_local_scope)
- Supports both array and table formats
- Template and template_file support for dynamic repositories

**Key features:**
- Only accepts Local (Workdir) scope values
- Array format: list of repository configs
- Table format: can have `repositories`, `template`, or `template_file`
- Repository config can be simple string (handle only) or full table

### command_definition.rs
Status: **PENDING**

**Analysis:**
- Large file (~98KB) with complex parsing logic
- Multiple related types need conversion
- Currently uses old `ConfigValue` API
- Used by `OmniConfig.commands` field via conversion helper

---

## Type Mappings

### Config Source Mapping
| Feuilletage | Omni |
|---------|------|
| `ConfigSource::File(PathBuf)` | `ConfigSource::File(String)` |
| `ConfigSource::Default` | `ConfigSource::Default` |
| `ConfigSource::Programmatic` | `ConfigSource::Default` |
| `ConfigSource::Environment` | `ConfigSource::Default` |
| `ConfigSource::Custom(_)` | `ConfigSource::Default` |

### Config Level/Scope Mapping
| Feuilletage | Omni |
|---------|------|
| `ConfigLevel::System` | `ConfigScope::System` |
| `ConfigLevel::User` | `ConfigScope::User` |
| `ConfigLevel::Local` | `ConfigScope::Workdir` |
| `ConfigLevel::Custom { .. }` | `ConfigScope::Default` |

### Value Type Mapping
| Feuilletage | config_value |
|---------|--------------|
| `Value::Null` | `None` / `Value::Null` |
| `Value::Bool(b)` | `Value::Bool(b)` |
| `Value::Int(i)` | `Value::Integer(i)` |
| `Value::Float(f)` | `Value::Float(f)` |
| `Value::String(s)` | `Value::String(s)` |
| `Value::Array(arr)` | `Value::Sequence(vec)` |
| `Value::Object(map)` | `Value::Mapping(hashmap)` |
