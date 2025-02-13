---
description: Configuration of the `cargo_install` parameter
---

# `cargo_install`

## Parameters

Configuration of the cache for `cargo-install` operations.

| Operation | Type | Description                                                    |
|-----------|------|---------------------------------------------------------|
| `versions_expire` | duration | How long to cache a given versions that can be used for a given crate for. This allows to avoid listing available versions on each `omni up` call. The versions are automatically re-listed if the cache does not contain any matching version. |
| `versions_retention` | duration | How long to keep the cached list of versions around even after the crate is no longer installed; this is calculated from the last time the versions were fetched. |
| `cleanup_after` | duration | The grace period before cleaning up the resources that are no longer needed. |

## Example

```yaml
cache:
  cargo_install:
    versions_expire: 1d
    cleanup_after: 1w
```
