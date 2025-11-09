---
description: Configuration of the `cache.environment` parameter
slug: /reference/configuration/parameters/cache/environment
---

# `cache.environment`

## Parameters

Configuration of the cache for environment history tracking.

| Parameter | Type | Description |
|-----------|------|-------------|
| `retention` | duration | How long to keep closed environment entries *(default: `90d`)* |
| `retention_stale` | duration | How long before checking if an open entry is stale and cleaning it up if the workdir no longer exists *(default: `180d`)* |
| `max_per_workdir` | integer | Maximum number of environment entries to keep per workdir *(optional)* |
| `max_total` | integer | Maximum total number of environment entries to keep across all workdirs *(optional)* |

## Behavior

Omni tracks the history of environment configurations used for each workdir. This allows you to see when and how workdirs were used.

### Automatic Cleanup

Environment entries are automatically cleaned up during `omni up`:

1. **Closed entries**: When you run `omni down` or switch environments, entries are marked as closed. Closed entries older than `retention` are removed.

2. **Stale entries**: Open entries that haven't been seen for longer than `retention_stale` are checked:
   - If the workdir (repository, package, or sandbox) no longer exists, the entry is closed
   - If the workdir still exists, the `last_seen_at` timestamp is updated
   - Each time you run `omni up` in a workdir, its `last_seen_at` is updated

3. **Limits**: If `max_per_workdir` or `max_total` are set, older entries are removed to stay within the limits.

### Disabling Cleanup

To disable a cleanup mechanism, set its value to `0`:

```yaml
cache:
  environment:
    retention: 0         # Never clean up closed entries
    retention_stale: 0   # Never check if open entries are stale
```

## Example

```yaml
cache:
  environment:
    retention: 90d           # Keep closed entries for 90 days
    retention_stale: 180d    # Check stale entries after 6 months
    max_per_workdir: 10      # Keep max 10 entries per workdir
    max_total: 100           # Keep max 100 entries total
```
