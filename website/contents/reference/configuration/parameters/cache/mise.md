---
description: Configuration of the `mise` parameter
---

# `mise`

## Parameters

Configuration of the cache for `mise` operations.

| Operation | Type | Description                                                    |
|-----------|------|---------------------------------------------------------|
| `update_expire` | duration | How long to cache the fact that updates for `mise` itself have been checked. This allows to avoid checking for updates on each `omni up` call. |
| `plugin_update_expire` | duration | How long to cache the fact that updates for a given `mise` plugin have been checked. This allows to avoid checking for updates on each `omni up` call. |
| `plugin_versions_expire` | duration | How long to cache a given `mise` plugin versions for. This allows to avoid listing available versions on each `omni up` call. |
| `plugin_versions_retention` | duration | How long to keep the cached list of versions around even after the `mise` plugin is no longer installed; this is calculated from the last time the versions were fetched. |
| `cleanup_after` | duration | The grace period before cleaning up the resources that are no longer needed. |

## Example

```yaml
cache:
  mise:
    update_expire: 1d
    plugin_update_expire: 1d
    plugin_versions_expire: 1h
    plugin_versions_retention: 90d
    cleanup_after: 1w
```
