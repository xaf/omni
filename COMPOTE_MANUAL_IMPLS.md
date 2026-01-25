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

### 2. `UpConfigGolang` (up/golang.rs)

**Reason**: Three technical blockers

1. **BTreeSet<String>**: Compote's `allow_single` attribute produces `Vec`, not `BTreeSet`.
   The `dirs` field needs ordered, deduplicated entries.

2. **Custom path normalization**: Each `dirs` entry goes through `PathBuf::normalize()`,
   which cannot be expressed with a simple transform attribute.

3. **OnceCell<UpConfigMise>**: The backend field uses lazy initialization with `OnceCell`,
   which requires special handling during deserialization.

---

### 3. `UpConfigNodejs` (up/nodejs.rs)

**Reason**: Backend requires tool name argument during construction

```rust
// The derive macro can't express passing "node" as an argument:
let backend = UpConfigMise::compote_from_context_value("node", Some(value), errors);
```

The mise backend needs to know which tool it's configuring at construction time,
which cannot be expressed with derive macro attributes.

**Note**: The nested `UpConfigNodejsParams` struct uses the derive macro.

---

### 4. `StoredConfig` (parser/suggest_config.rs)

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

## Future Considerations

These manual implementations could potentially be converted if compote adds:

1. **Custom serialization override**: Allow `#[compote(serialize_as = "inner")]` to
   serialize a wrapper type as its inner value.

2. **BTreeSet support**: Add `FromContextValue` impl for `BTreeSet<T>` or allow
   `allow_single` to target collection types other than `Vec`.

3. **Transform with state**: Allow transforms that take additional parameters
   (like tool name for mise backend).

4. **Passthrough derive**: A lightweight `#[derive(CompotePassthrough)]` for
   newtype wrappers that just strip context.
