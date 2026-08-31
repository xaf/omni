# Omni Config Migration to Feuilletage

The Omni configuration migration is complete in this combined worktree.

## Current State

- The external `config_value` dependency, imports, compatibility re-exports,
  and crate are gone.
- `OmniConfig` and `CommandDefinition` are derived Feuilletage Config
  projections; neither is pending migration.
- All 17 formerly manual `FromContextValue` implementations are derived Config
  projections.
- Complex domain types parse through typed wire schemas with `parse_as`, then
  convert through `FromParsed`.
- Projection mutability delegates to the wire schema's `MutabilityInfo`
  constraints.

## Compatibility Boundaries

`ConfigErrorKind` remains intentionally defined inside Omni. It preserves
Omni's diagnostic codes and compatibility behavior; it is not evidence of an
external `config_value` dependency.

`StoredConfig` is the only domain projection that legitimately passes through
dynamic `feuilletage::Value`. Suggestion config accepts arbitrary data, and the
stored representation must strip context and provenance. All other complex
domain conversions use typed wire schemas.

Suggestion and suggestion-clone parsing select local contributions
recursively, including nested objects and arrays. Non-local leaves do not leak
into the resulting suggestion config.

## Verification

Parity tests cover serialized public shapes and diagnostic behavior, including
complex command syntax and projected domain types. Future config changes should
extend the typed wire schema and `FromParsed` conversion rather than reintroduce
manual contextual parsing or an external compatibility crate.
