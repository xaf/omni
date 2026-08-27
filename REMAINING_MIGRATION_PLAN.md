# Plan: Eliminate Remaining config_value Crate Dependencies

## Overview

This plan covers the final steps to completely eliminate the `config_value` crate from the omni codebase. The migration is ~85% complete. The remaining work is categorized into 3 phases.

**Current State:**
- 120 occurrences of `ConfigErrorKind::` across 17 files
- 8 files still importing `config_value::Value`
- Re-exports in `config/mod.rs` and `config/config_value.rs`
- Bridge functions in `config/utils.rs`

---

## Phase 5b: Replace ConfigErrorKind with Feuilletage Errors

### Background

`ConfigErrorKind` is an enum in `config_value/src/error.rs` with ~40 variants for different error types. The `ConfigErrorHandler` in `src/internal/config/parser/errors.rs` has a `to_feuilletage_error()` mapping that converts these to feuilletage's `ConfigError`.

### Strategy

**Option A (Recommended): Direct Feuilletage Errors**
Replace `ConfigErrorKind` usages with direct `feuilletage::ConfigError` construction.

**Option B: Keep ConfigErrorKind as Omni-local Enum**
Move the enum definition from config_value to omni's codebase (as it's omni-specific anyway).

### Files to Update (17 files, ~120 occurrences)

| File | Occurrences | Complexity |
|------|-------------|------------|
| `src/internal/config/parser/path.rs` | 6 | Low |
| `src/internal/config/parser/github.rs` | 4 | Low |
| `src/internal/config/parser/askpass.rs` | 1 | Low |
| `src/internal/config/parser/command_definition.rs` | 7 | Medium |
| `src/internal/config/up/bundler.rs` | 1 | Low |
| `src/internal/config/up/custom.rs` | 2 | Low |
| `src/internal/config/up/nix.rs` | 6 | Low |
| `src/internal/config/up/python.rs` | 2 | Low |
| `src/internal/config/up/tool.rs` | 2 | Low |
| `src/internal/config/up/base.rs` | 3 | Low |
| `src/internal/config/up/homebrew.rs` | 6 | Low |
| `src/internal/config/up/cargo_install.rs` | 9 | Medium |
| `src/internal/config/up/go_install.rs` | 9 | Medium |
| `src/internal/config/up/github_release.rs` | 14 | High |
| `src/internal/commands/frompath.rs` | 29 | High |
| `src/internal/commands/frompath_test.rs` | 14 | Medium |
| `src/internal/commands/builtin/config/check.rs` | 5 | Low |

### Implementation Steps

1. **Update `ConfigErrorHandler`** in `errors.rs`:
   - Add methods that accept `feuilletage::ConfigError` directly
   - Keep `ConfigErrorKind` methods as deprecated aliases during transition

2. **Update each file** (in order of complexity, low to high):
   - Replace `error_handler.error(ConfigErrorKind::Foo)` with `error_handler.feuilletage_error(FeuilletageConfigError::Foo {...})`
   - Update imports

3. **Remove `ConfigErrorKind` re-export** from `errors.rs`

### Example Transformation

**Before:**
```rust
error_handler
    .with_key("field")
    .with_expected("string")
    .with_actual(value)
    .error(ConfigErrorKind::InvalidValueType);
```

**After (Option A):**
```rust
error_handler.record(ConfigError::TypeMismatch {
    path: error_handler.current_path(),
    expected: "string".to_string(),
    actual: value.type_name().to_string(),
});
```

---

## Phase 5c: Replace config_value::Value Usages

### Files Using config_value::Value

| File | Usage | Replacement Strategy |
|------|-------|---------------------|
| `src/internal/commands/frompath.rs` | Metadata header parsing | Use `feuilletage::Value` |
| `src/internal/commands/builtin/status.rs` | Config display | Use `feuilletage::Value` |
| `src/internal/commands/builtin/up.rs` | Config manipulation | Use `feuilletage::Value` |
| `src/internal/config/parser/omniconfig.rs` | ? | Investigate |
| `src/internal/config/parser/errors.rs` | ErrorHandler | Use `feuilletage::Value` |
| `src/internal/cache/prompts.rs` | Prompt storage | Use `feuilletage::Value` |
| `src/internal/cache/prompts_test.rs` | Tests | Update with prompts.rs |
| `src/internal/config/utils.rs` | Bridge function | Remove when bridge is gone |

### Main Challenge: frompath.rs Metadata Header Parsing

The `PathCommandFileDetails` struct uses `config_value::Value` for parsing metadata headers from command files. This involves:

1. Deserializing YAML to `serde_yaml::Value`
2. Converting to `config_value::Value`
3. Using methods like `as_mapping()`, `as_str()`, `as_bool()`

**Solution:** Use `feuilletage::Value` directly, which provides similar methods.

### Implementation Steps

1. **Create `feuilletage::Value` from YAML helper** (if not exists):
   ```rust
   fn yaml_to_feuilletage_value(yaml: serde_yaml::Value) -> feuilletage::Value
   ```

2. **Update frompath.rs**:
   - Change `Value::from(yaml_value)` to `yaml_to_feuilletage_value(yaml_value)`
   - Update method calls (`as_mapping()` -> `as_object()`, etc.)

3. **Update remaining files** similarly

4. **Remove bridge function** `feuilletage_to_config_value()` from utils.rs

---

## Phase 5d: Remove Re-exports and Clean Up

### Files to Modify

1. **`src/internal/config/config_value.rs`**:
   - Remove `pub use config_value::{ ... }` re-exports
   - Keep `ConfigSource`, `ConfigScope` definitions (these are omni-specific)
   - Remove `to_feuilletage_config_value()` function
   - Remove `omni_config_loader()` function

2. **`src/internal/config/mod.rs`**:
   - Remove re-exports from config_value module
   - Add direct re-exports from feuilletage where needed:
     ```rust
     pub(crate) use feuilletage::Value;
     pub(crate) use feuilletage::ConfigValue;
     ```

3. **`src/internal/config/loader.rs`**:
   - Remove `use config_value::FileDefinition`
   - Replace with feuilletage equivalent or inline

4. **`Cargo.toml`**:
   - Remove `config-value` dependency

5. **Delete `config-value/` crate directory** (if local)

---

## Dependency Order

```
Phase 5b: ConfigErrorKind removal
    ↓
Phase 5c: config_value::Value removal
    ↓
Phase 5d: Re-export cleanup and crate removal
```

**Note:** 5b and 5c can be done in parallel on separate files, but 5d depends on both completing.

---

## Estimated Effort

| Phase | Files | Est. Effort |
|-------|-------|-------------|
| 5b | 17 files | Medium (2-3 hours) |
| 5c | 8 files | Medium (2-3 hours) |
| 5d | 4 files | Low (30 min) |

**Total:** ~5-6 hours of focused work

---

## Verification Checklist

After each phase:
- [ ] `cargo check -p omnicli` passes
- [ ] `cargo test -p omnicli` passes
- [ ] No `config_value::` imports remain (for that phase's targets)

After all phases:
- [ ] `grep -r "config_value::" src/` returns nothing
- [ ] `grep -r "use config_value" src/` returns nothing
- [ ] `grep -r "ConfigErrorKind" src/` returns nothing (unless kept as local)
- [ ] `config-value` crate is removed from Cargo.toml
- [ ] Manual test: `omni up` works
- [ ] Manual test: `omni config bootstrap` works
- [ ] Manual test: `omni tidy` works

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Breaking metadata header parsing | Thorough testing with various command files |
| Error message changes | Keep error codes (C001, C101, etc.) consistent |
| Performance regression | The old and new systems have similar performance characteristics |
| Test failures | Run tests after each file conversion |

---

## Files Summary

### Phase 5b (ConfigErrorKind)
- 17 files with ~120 occurrences
- Key files: `frompath.rs`, `github_release.rs`, `cargo_install.rs`, `go_install.rs`

### Phase 5c (Value)
- 8 files
- Key files: `frompath.rs`, `errors.rs`, `prompts.rs`

### Phase 5d (Cleanup)
- `config_value.rs`, `mod.rs`, `loader.rs`, `Cargo.toml`
