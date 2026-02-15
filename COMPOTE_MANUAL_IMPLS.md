# Compote Manual FromContextValue Implementations

This document tracks types that intentionally use manual `FromContextValue` implementations
instead of the `#[derive(compote::Config)]` macro, along with the technical reasons.

## Types with Manual Implementations

### 1. `ShellAliasesConfig` (parser/shell_aliases.rs)

**Reason**: Custom serialization behavior

The container serializes as a bare array, not as a struct with an "aliases" field:
```rust
impl Serialize for ShellAliasesConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.aliases.serialize(serializer)  // Just the Vec, no wrapper
    }
}
```

The compote derive macro always generates struct-style serialization which would break
backwards compatibility with existing config files.

**Note**: The inner `ShellAliasConfig` struct uses the derive macro.

---

### 2. `UpConfigNodejs` (up/nodejs.rs)

**Reason**: Backend requires tool name argument during construction

```rust
// The derive macro can't express passing "node" as an argument:
let backend = UpConfigMise::compote_from_context_value("node", Some(value), errors);
```

The mise backend needs to know which tool it's configuring at construction time,
which cannot be expressed with derive macro attributes.

**Note**: The nested `UpConfigNodejsParams` struct uses the derive macro.

---

### 3. `StoredConfig` (parser/suggest_config.rs)

**Reason**: Trivial newtype wrapper - derive adds no value

```rust
pub struct StoredConfig(pub CompoteValue);

impl<S, L> compote::FromContextValue<S, L> for StoredConfig {
    fn from_context_value(value: &ContextValue<S, L>, _tracker: &mut ErrorTracker) -> Result<Self, Error> {
        Ok(StoredConfig(CompoteValue::from(value)))  // Just strip context
    }
}
```

This is a 3-line passthrough implementation. A derive macro would generate more
boilerplate for no practical benefit.

---

### 4. `SuggestCloneRepositoryConfig` (parser/suggest_clone.rs)

**Reason**: Custom shell word parsing and scalar-as-object pattern

```rust
// args field uses shell_words::split() - type-changing transform
let args = if let Some(v) = table.get("args") {
    let args_str = String::from_context_value(v, tracker)?;
    shell_words::split(&args_str).unwrap_or_default()  // String -> Vec<String>
} else {
    vec![]
};
```

1. **Shell word parsing**: The `args` field takes a string input and transforms it
   to `Vec<String>` using `shell_words::split()`. Compote transforms must preserve types.

2. **Scalar-as-object**: Accepts both string (handle only) and object format.
   While `#[compote(scalar_as)]` exists, combining with the shell_words transform isn't possible.

---

### 5. `SuggestCloneConfig` (parser/suggest_clone.rs)

**Reason**: Level-based filtering and polymorphic input

```rust
// Only accepts values from Local (Workdir) scope
let filtered = match select_local_scope(value) {
    Some(v) => v,
    None => return Ok(Self::default()),
};
```

The config filters values by level before parsing, only accepting Local scope values.
Compote doesn't support pre-processing filters on input values.

---

### 6. `SuggestConfig` (parser/suggest_config.rs)

**Reason**: Key-presence variant detection and custom serialization

```rust
// Checks for specific keys to determine variant
if let Some(config_val) = map.get("config") { ... }
else if let Some(template_val) = map.get("template") { ... }
else if let Some(template_file_val) = map.get("template_file") { ... }
```

1. **Key-presence detection**: Uses presence of specific keys (`config`, `template`,
   `template_file`) to determine which field to populate. Not a tagged enum pattern.

2. **Custom Serialize**: Serializes `config` field directly (not wrapped).
   Would conflict with derive-generated Serialize impl.

---

### 7. `CommandDefinition` (parser/command_definition.rs)

**Reason**: Context metadata injection

```rust
// source and scope come from context metadata, not the value
let source = match value.context().source.file_path() {
    Some(path) => OmniSource::File(path.to_path_buf()),
    None => OmniSource::Default,
};
let scope = match value.context().level.name() { ... };
```

The `source` and `scope` fields are populated from the `ContextValue`'s context
(file path and config level), not from the config value itself. These `#[serde(skip)]`
fields are runtime metadata that compote derive cannot inject.

---

## Future Considerations

These manual implementations could potentially be converted if compote adds:

1. **Custom serialization override**: Allow `#[compote(serialize_as = "inner")]` to
   serialize a wrapper type as its inner value.

2. **Transform with state**: Allow transforms that take additional parameters
   (like tool name for mise backend).

3. **Passthrough derive**: A lightweight `#[derive(CompotePassthrough)]` for
   newtype wrappers that just strip context.

4. **Type-changing transforms**: Allow transforms that change the field type,
   e.g., `#[compote(parse_fn = "shell_words::split")]` for String -> Vec<String>.

5. **Level-based filtering**: Add `#[compote(only_from_level = "local")]` to filter
   values by config level before parsing.

6. **Key-presence variants**: Support `#[compote(key_presence)]` for structs where
   the presence of certain keys determines which fields are populated.

7. **Context metadata injection**: Add `#[compote(from_context_source)]` and
   `#[compote(from_context_level)]` to inject context metadata into struct fields.

---

### 8. `PromptsConfig` (parser/prompts.rs)

**Reason**: Custom serialization behavior (same pattern as ShellAliasesConfig)

```rust
impl Serialize for PromptsConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.prompts.serialize(serializer)  // Just the Vec, no wrapper
    }
}
```

The container serializes as a bare array, not as a struct with a "prompts" field.

---

### 9. `PromptConfig` (parser/prompts.rs)

**Reason**: Complex parent-object extraction pattern

The struct contains fields (`prompt_type`, `scope`) that are deserialized from the SAME parent
object, not from sub-objects. The `PromptScope::from_context_value` receives the entire parent
object and extracts the "scope" field from it. Similarly, `PromptType` extracts "type" and
sibling fields (choices, min, max) from the parent.

This "sibling field extraction" pattern cannot be expressed with derive macros.

---

### 10. `PromptScope` (parser/prompts.rs)

**Reason**: Extracts from parent object with case-insensitive matching

```rust
// Reads "scope" from parent object, not from the value itself
let scope_value = match table.get("scope") { ... };
let scope = s.trim().to_lowercase();  // Case-insensitive
```

1. **Parent extraction**: Receives parent object and reads "scope" field
2. **Case-insensitive**: Normalizes with `.trim().to_lowercase()` before matching

---

### 11. `PromptType` (parser/prompts.rs)

**Reason**: Complex internally-tagged enum with sibling field extraction

```rust
// Reads "type" from parent, plus reads sibling fields (choices, min, max)
let type_str = type_value.trim().to_lowercase();  // Case-insensitive
match type_str.as_str() {
    "choice" | "select" | "choices" | "multichoice" | "multiselect" => { ... }
    "int" => { let min = table.get("min"); let max = table.get("max"); ... }
}
```

1. **Parent extraction**: Reads "type" field from parent object
2. **Sibling fields**: Reads "choices", "min", "max" from the same object level
3. **Multiple aliases**: "choices", "multichoice", "multiselect" all map to MultiChoice
4. **Case-insensitive**: Normalizes type string before matching

---

### 12. `PromptChoicesConfig` (parser/prompts.rs)

**Reason**: Union type with custom serialization

Can be either:
- `Vec<PromptChoiceConfig>` (array of choices)
- `String` (template that gets evaluated later)

Custom Serialize impl serializes the inner value directly. Would need `#[compote(untagged)]`
but that conflicts with the custom serialization requirement.

---

### 13. `PromptChoiceConfig` (parser/prompts.rs)

**Reason**: Custom string-or-object with fallback logic

```rust
// String input -> id and choice both set to the string
// Object with only id -> choice defaults to id
// Object with only choice -> id defaults to choice
```

Has complex fallback logic where missing fields inherit from present ones.

---

### 14. `StringFilter` (parser/github.rs)

**Reason**: Dual-nature variant pattern

```rust
// String -> Glob variant (bare string becomes glob pattern)
// Null -> Any variant
// Object {glob: "..."} -> also Glob variant
// Object {contains: "..."} -> Contains variant
```

The `Glob` variant needs to handle BOTH:
1. Bare string input (any string becomes a glob pattern)
2. Object with "glob" key

This dual-nature pattern (scalar + map-key for same variant) isn't supported.

---

### 15. `GithubAuthConfig` (parser/github.rs)

**Reason**: Heuristic-based string parsing

```rust
// String "skip" -> Skip(true)
// String "gh" -> GhCli { hostname: None, user: None }
// String all-caps -> TokenEnvVar(s)  // e.g., "GITHUB_TOKEN"
// Other string -> Token(s)
// Object with various keys (skip, token, token_env_var, gh) -> corresponding variants
// gh value can be string (hostname) or object {hostname, user}
```

1. **All-caps heuristic**: Uses `s.chars().all(|c| c.is_uppercase() || c == '_')` to
   distinguish TokenEnvVar from Token - can't express this with derive attributes
2. **Multiple object keys**: Checks for skip, token_env_var, token, gh keys in priority order
3. **Polymorphic gh**: The gh field accepts string (hostname only) or object
