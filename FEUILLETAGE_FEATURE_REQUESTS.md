# Feuilletage Features Used by Omni

The Feuilletage work needed for the Omni migration is complete. There are no
remaining Omni config types blocked on manual `FromContextValue`
implementations.

## Delivered Projection Model

- `parse_as` lets a derived Config project through a separate typed wire
  schema.
- `FromParsed` converts the validated wire value into the domain type while
  retaining access to the original contextual value and error tracker.
- Generated projection `MutabilityInfo` delegates to the wire schema, including
  its level constraints.

Together these features cover the patterns previously described as requiring
manual parsing: polymorphic input, dynamic keys, one-to-many expansion,
heterogeneous arrays, parameterized construction, context-dependent values,
normalization, and partial-success diagnostics. Each pattern is expressed in a
typed wire schema followed by domain conversion rather than a hand-written
`FromContextValue` implementation.

## Intentional Dynamic Projection

`StoredConfig` uses `parse_as = "feuilletage::Value"`. Suggestion config is
arbitrary by design, and this projection strips context and provenance before
storage. Other domain conversions use typed wire schemas.

## Status

`CommandDefinition`, `OmniConfig`, and the 17 formerly manual projections are
complete. Serialization and diagnostic parity tests protect the public shapes
and error behavior. New feature requests should be based on a new concrete
Feuilletage limitation, not the obsolete manual-implementation inventory.
