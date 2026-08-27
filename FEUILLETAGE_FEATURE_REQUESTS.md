# Feuilletage Feature Requests

This document consolidates the feuilletage features that would enable converting more manual
`FromContextValue` implementations to use the `#[derive(feuilletage::Config)]` macro.

See `FEUILLETAGE_MANUAL_IMPLS.md` for detailed documentation of each type that requires manual
implementation and why.

---

## Analysis Results (2026-01-27)

A comprehensive analysis of all remaining manual implementations was performed. Here are the findings:

### Files Analyzed

| File | Types Analyzed | Conclusion |
|------|---------------|------------|
| **check.rs** | `CheckPattern`, `CheckConfig`, `StringFilter` | Keep manual - context-dependent computation, polymorphic parsing |
| **env.rs** | `EnvOperationEnum`, `EnvOperationConfig`, `EnvConfig` | Keep manual - dynamic keys, one-to-many deserialization |
| **base.rs** | `UpConfig`, `UpConfigTool` | Keep manual - heterogeneous arrays, error accumulation, parameterized construction |
| **python.rs** | `UpConfigPythonParams`, `UpConfigPython` | Keep manual - multi-field dispatch, post-construction callbacks |
| **homebrew.rs** | `HomebrewTap`, `HomebrewInstall`, `UpConfigHomebrew`, etc. | Keep manual - multi-field key mapping, custom serialization |
| **golang.rs** | `UpConfigGolang` | Keep manual - BTreeSet unsupported, OnceCell issues, path normalization |

### Key Findings

1. **No remaining types can benefit from conversion** - All complex types have parsing logic that exceeds feuilletage's declarative capabilities
2. **Simple enums are used internally** - Types like `EnvOperationEnum`, `HomebrewInstallType`, and `HomebrewHandled` are constructed programmatically during parsing, not parsed via feuilletage
3. **Common blockers across files:**
   - Dynamic key-to-field mapping (key becomes field value)
   - Polymorphic input (same field accepts string/array/object with different handling)
   - One-to-many deserialization (one input → multiple outputs)
   - Multi-field derivation from single key
   - Context-dependent field computation
   - BTreeSet/OnceCell support gaps

---

## Completed Features

The following features from the original request list have been implemented:

| Feature | Implementation | Status |
|---------|---------------|--------|
| Context metadata injection | `from_context = "source.file_path"`, `"level.name"`, `"source.display_name"` | ✅ Done |
| Custom serialization override | `#[feuilletage(transparent)]` attribute | ✅ Done |
| Level-based filtering | `#[feuilletage(mutable_by = ["level"])]` attribute | ✅ Done |
| Heuristic predicates | `variant = predicate("fn")`, `starts_with`, `ends_with`, `contains`, `regex`, `range` | ✅ Done |
| Dual-nature variants | `scalar_variant` + `rename` combination | ✅ Done |
| BTreeSet/HashSet support | `FromContextValue` impl with `on_error` support | ✅ Done |
| Transform with arguments | Documented workaround: use wrapper functions | ✅ Workaround |
| Field fallback/copy | `#[feuilletage(fallback = "field")]` attribute | ✅ Done |
| Conditional field defaults | `#[feuilletage(default_fn = "fn(field1, field2)")]` | ✅ Done |
| OnceCell field skip | `#[feuilletage(skip)]` uses `Default::default()` | ✅ Done |
| Separate Serialize/Deserialize | `#[feuilletage(skip_serialize)]` container attribute | ✅ Done |
| Parent-object field extraction | Use `#[feuilletage(flatten)]` - nested type receives remaining object | ✅ Use flatten |
| Multiple aliases per variant | `#[feuilletage(aliases = ["alt1", "alt2"])]` on enum variants | ✅ Done |
| Post-construction callbacks | `#[feuilletage(post_process = "fn")]` - receives `&mut T`, original value, and error tracker | ✅ Done |

---

## Outstanding Feature Requests

### High Priority - Would Enable Multiple Conversions

#### 1. Dynamic Key Capture (`key_as`)

**Would unblock:** `EnvOperationConfig`, `HomebrewTap`, `HomebrewInstall`

**Current limitation:** These types parse from YAML/JSON where the map key becomes a field value:
```yaml
MY_VAR: "value"           # name="MY_VAR", value="value"
PATH:
  prepend: "/usr/local"   # name="PATH", operation=Prepend
```

The key (`MY_VAR`, `PATH`) is not a fixed schema key but a dynamic value that should populate a field.

**Proposed syntax:**
```rust
#[derive(feuilletage::Config)]
#[feuilletage(key_as = "name")]  // Capture the map key into the `name` field
struct EnvOperation {
    name: String,       // Populated from the map key
    value: String,      // Populated from the map value
}

// On Vec field:
#[feuilletage(allow_map(key_as = "name"))]  // Each map entry's key -> name field
items: Vec<EnvOperation>,
```

**Complexity:** High - requires fundamentally different parsing model

---

#### 2. One-to-Many Deserialization

**Would unblock:** `EnvOperationConfig`, `EnvConfig`

**Current limitation:** A single input entry can produce multiple output structs:
```yaml
MY_VAR:
  prepend: ["/first", "/second"]  # Produces TWO EnvOperationConfig instances
```

**Proposed syntax:**
```rust
#[derive(feuilletage::Config)]
#[feuilletage(expand_arrays)]  // Each array element in value produces separate struct
struct EnvOperation {
    name: String,
    value: String,
    operation: EnvOperationEnum,
}
```

**Complexity:** Very high - requires iterator-style deserialization

---

#### 3. Multi-Field Derivation from Single Key

**Would unblock:** `HomebrewInstall`

**Current limitation:** The presence of a `formula` or `cask` key sets TWO fields:
```yaml
formula: "package-name"  # Sets name="package-name" AND install_type=Formula
cask: "app-name"         # Sets name="app-name" AND install_type=Cask
```

**Proposed syntax:**
```rust
#[derive(feuilletage::Config)]
struct HomebrewInstall {
    #[feuilletage(from_key = "formula" | "cask")]  // Key name determines this field
    install_type: HomebrewInstallType,

    #[feuilletage(from_value_of = "formula" | "cask")]  // Key's value populates this
    name: String,
}

// Alternative: key-based variant selection
#[feuilletage(key_variant = "formula" => Formula, "cask" => Cask)]
install_type: HomebrewInstallType,
```

**Complexity:** High - requires key-aware field mapping

---

#### 4. Context-Dependent Field Computation (`from_context_fn`)

**Would unblock:** `CheckPattern`

**Current limitation:** Some fields need computed values from context, not raw injection:
```rust
// Current manual implementation:
is_global: value.context().level.name() != "local"
```

The existing `from_context = "level.name"` can only inject raw values, not compute derived values.

**Proposed syntax:**
```rust
#[derive(feuilletage::Config)]
struct CheckPattern {
    pattern: String,

    #[feuilletage(from_context_fn = "compute_is_global")]
    is_global: bool,
}

fn compute_is_global<S, L>(ctx: &feuilletage::Context<S, L>) -> bool {
    ctx.level.name() != "local"
}
```

**Complexity:** Medium - similar to existing `from_context` but with function call

---

#### 5. HashMap Array Conversion (`allow_array` for HashMap)

**Would unblock:** `CheckConfig` (tags field)

**Current limitation:** The `tags` field accepts both object and array formats:
```yaml
# Object format (already works)
tags:
  tag1: {contains: "foo"}
  tag2: null

# Array format (needs conversion)
tags: ["tag1", "tag2"]  # Should become {"tag1": default, "tag2": default}
```

**Proposed syntax:**
```rust
#[derive(feuilletage::Config)]
struct CheckConfig {
    #[feuilletage(allow_array)]  // ["a", "b"] -> {"a": default, "b": default}
    tags: HashMap<String, StringFilter>,
}
```

**Complexity:** Medium - similar to existing `allow_single` but for HashMap

---

### Medium Priority - Complex Architectural Features

#### 6. Error Accumulation Pattern

**Would unblock:** `UpConfig`

**Current limitation:** The struct collects errors during parsing (via an `errors` field) rather than failing fast. Invalid items are skipped and errors accumulated.

```rust
pub struct UpConfig {
    pub steps: Vec<UpConfigTool>,
    pub errors: Vec<UpError>,  // Populated during parsing, not from config
}
```

**Proposed syntax:**
```rust
#[derive(feuilletage::Config)]
#[feuilletage(accumulate_errors_in = "errors")]
struct UpConfig {
    steps: Vec<UpConfigTool>,

    #[feuilletage(errors)]
    errors: Vec<UpError>,
}
```

**Complexity:** High - requires different error handling model

---

#### 7. Parameterized Variant Construction

**Would unblock:** `UpConfigTool` (Bash variant), `UpConfigPython`, `UpConfigNodejs`

**Current limitation:** Some variants need extra parameters passed to their constructor:
```rust
// Current manual implementation:
"bash" => UpConfigMise::feuilletage_from_context_value_with_params(
    "bash",
    config_value,
    UpConfigMiseParams { tool_url: Some("https://...".into()) },
    error_tracker,
)
```

**Proposed syntax:**
```rust
#[derive(feuilletage::Config)]
#[feuilletage(external_tag)]
enum UpConfigTool {
    #[feuilletage(
        rename = "bash",
        construct_with = "UpConfigMise::with_params",
        args = ["bash", UpConfigMiseParams { tool_url: Some("...") }]
    )]
    Bash(UpConfigMise),
}
```

**Complexity:** Very high - requires expression evaluation at macro time

---

#### 9. Heterogeneous Array Input

**Would unblock:** `UpConfig`

**Current limitation:** Array elements can be multiple types:
```yaml
up:
  - python           # String -> tool name
  - 3.11             # Float -> version (as tool name)
  - {go: "1.21"}     # Object -> tool config
```

**Proposed syntax:**
```rust
#[derive(feuilletage::Config)]
struct UpConfig {
    #[feuilletage(heterogeneous_array(
        string => UpConfigTool::from_name,
        number => UpConfigTool::from_version,
        object => UpConfigTool::from_config
    ))]
    steps: Vec<UpConfigTool>,
}
```

**Complexity:** Very high - requires multi-dispatch deserialization

---

#### 10. Dynamic Fallback Variant with Tag Capture

**Would unblock:** `UpConfigTool` (Mise fallback)

**Current limitation:** Unknown tool names should create a `Mise` variant using the tag as a parameter:
```rust
// Any unknown name like "ruby" creates:
UpConfigTool::Mise(UpConfigMise::new("ruby", config_value))
```

**Proposed syntax:**
```rust
#[derive(feuilletage::Config)]
#[feuilletage(external_tag)]
enum UpConfigTool {
    // ... known variants ...

    #[feuilletage(fallback, from_tag)]  // Captures the tag value
    Mise(UpConfigMise),  // UpConfigMise receives the tag as first arg
}
```

**Complexity:** High - requires tag capture + constructor injection

---

### Lower Priority - Nice to Have

#### 11. Multi-Field Dispatch from Single Input

**Would unblock:** `UpConfigPythonParams`

**Current limitation:** A single `pip` key maps to multiple fields based on value type/content:
```yaml
pip: ["file1.txt", "file2.txt"]  # pip_files = [...], pip_auto = false
pip: true                         # pip_auto = true
pip: false                        # pip_disabled = true
pip: "auto"                       # pip_auto = true
pip: "requirements.txt"           # pip_files = ["requirements.txt"]
```

**Proposed syntax:**
```rust
#[derive(feuilletage::Config)]
struct UpConfigPythonParams {
    #[feuilletage(from = "pip", when = array)]
    pip_files: Vec<String>,

    #[feuilletage(from = "pip", when = true | "auto")]
    pip_auto: bool,

    #[feuilletage(from = "pip", when = false)]
    pip_disabled: bool,
}
```

**Complexity:** Very high - requires conditional field mapping

---

#### 12. Path Normalization Transform

**Would unblock:** `UpConfigGolang`

**Current limitation:** Each directory entry is normalized via `PathBuf::normalize()`:
```rust
dirs.insert(
    PathBuf::from(dir_value)
        .normalize()
        .to_string_lossy()
        .to_string(),
);
```

**Proposed syntax:**
```rust
#[derive(feuilletage::Config)]
struct UpConfigGolang {
    #[feuilletage(transform_each = "normalize_path_string")]
    dirs: BTreeSet<String>,
}

fn normalize_path_string(s: String) -> String {
    PathBuf::from(s).normalize().to_string_lossy().to_string()
}
```

**Note:** `transform_each` exists but operates on `ContextValue` (before deserialization), not the final type. Could use `post_process` with manual iteration as a workaround, or extend `transform_each` to support post-deserialization transforms.

**Complexity:** Medium - requires new `transform_each_post` or similar attribute

---

## Summary Table

| # | Feature | Types Unblocked | Priority | Complexity |
|---|---------|-----------------|----------|------------|
| 1 | Dynamic key capture (`key_as`) | 3 | High | High |
| 2 | One-to-many deserialization | 2 | High | Very High |
| 3 | Multi-field derivation from key | 1 | High | High |
| 4 | Context-dependent computation (`from_context_fn`) | 1 | High | Medium |
| 5 | HashMap allow_array | 1 | High | Medium |
| 6 | Error accumulation pattern | 1 | Medium | High |
| 7 | Parameterized variant construction | 3 | Medium | Very High |
| 8 | Heterogeneous array input | 1 | Medium | Very High |
| 9 | Dynamic fallback with tag capture | 1 | Medium | High |
| 10 | Multi-field dispatch from input | 1 | Lower | Very High |
| 11 | Path normalization transform | 1 | Lower | Medium |

---

## Recommendations

### Short-term (Medium Complexity)

The following features have reasonable implementation effort and clear value:

1. **Context-dependent computation (`from_context_fn`)** - Add function-based context injection for computed values
2. **HashMap allow_array** - Similar pattern to existing `allow_single` but for HashMap

### Long-term (High Complexity)

These features require significant architectural changes and may be better left as manual implementations:

3. **Dynamic key capture** - Fundamentally different parsing model
4. **One-to-many deserialization** - Iterator-style deserialization
5. **Multi-field derivation** - Key-aware field mapping
6. **Error accumulation** - Different error handling model
7. **Parameterized construction** - Expression evaluation at macro time
8. **Dynamic fallback with tag capture** - Tag capture + constructor injection

### Keep Manual

Some patterns are simply too complex for declarative macros:
- Heterogeneous array input with multiple dispatch
- Multi-field dispatch from single input with value-dependent routing
- Path normalization (can use `post_process` with manual iteration)

These are best kept as manual implementations where the code clearly expresses the complex logic.
