---
description: Configuration of the `cache` parameter
slug: /reference/configuration/parameters/cache
---

# `cache`

## Parameters

Configuration of the cache.

| Operation | Type | Description                                                    |
|-----------|------|---------------------------------------------------------|
| `path` | path | The path to the cache directory *(default: `~/.cache/omni`)* |
| `cargo_install` | [cargo_install](cache/cargo_install) | Configuration of the cache for `cargo-install` operations |
| `github_release` | [github_release](cache/github_release) | Configuration of the cache for `github-release` operations |
| `go_install` | [go_install](cache/go_install) | Configuration of the cache for `go-install` operations |
| `homebrew`  | [homebrew](cache/homebrew) | Configuration of the cache for `homebrew` operations |
| `mise` | [mise](cache/mise) | Configuration of the cache for `mise` operations |

## Example

```yaml
cache:
  path: ~/.cache/omni
  cargo_install:
    versions_expire: 1d
    cleanup_after: 1w
  github_release:
    versions_expire: 1d
    cleanup_after: 1w
  go_install:
    versions_expire: 1d
    cleanup_after: 1w
  homebrew:
    update_expire: 1d
    install_update_expire: 1d
    install_check_expire: 12h
    cleanup_after: 1w
  mise:
    update_expire: 1d
    plugin_update_expire: 1d
    plugin_versions_expire: 1h
    clean_after: 1w
```
