# Omni Config Migration Completion Record

No config migration phases remain in this combined worktree.

## Completed

- Removed the external `config_value` dependency and crate.
- Removed `config_value` imports, re-exports, and bridge-only compatibility
  code.
- Converted `OmniConfig` and `CommandDefinition` to derived Feuilletage Config
  projections.
- Replaced all 17 manual `FromContextValue` implementations with derived
  projections.
- Added typed wire schemas plus `FromParsed` domain conversions for complex
  config shapes.
- Made suggestion config recursively local-only.
- Preserved serialization and diagnostic behavior with parity tests.

## Intentional Residuals

- `ConfigErrorKind` is an Omni-local compatibility diagnostic enum. It should
  remain unless Omni's public diagnostic codes and behavior are deliberately
  redesigned.
- `StoredConfig` projects through dynamic `feuilletage::Value` to remove
  context and provenance from arbitrary stored suggestion config. This is not
  a migration bridge.

## Ongoing Guardrails

- Prefer a typed wire schema and `FromParsed` for new complex config domains.
- Keep projection mutability aligned with the wire schema constraints.
- Do not reintroduce the external `config_value` crate or manual
  `FromContextValue` implementations.
- Preserve serialization and diagnostic parity when changing config schemas.
