---
description: Configuration of the `org` parameter
---

# `org`

## Parameters

Configuration for the default organizations.
This is expected to be a list of objects containing the following parameters:

| Parameter  | Type   | Description                                               |
|------------|--------|-----------------------------------------------------------|
| `handle` | string | the organization handle, e.g. `git@github.com:xaf`, `github.com/xaf` |
| `trusted` | boolean | whether or not the organization is to be trusted automatically for `omni up` *(default: true)* |
| `worktree` | dirpath | override the path to the worktree for that organization, see [`worktree`](worktree) *(default: null)* |
| `repo_path_format` | string | override the format string for the path to the repository, see [`repo_path_format`](repo_path_format) *(default: null)* |

## Example

```yaml
org:
  - handle: git@github.com:xaf
    trusted: true
    worktree: /home/xaf/my-stuff
    repo_path_format: "%{repo}"
  - handle: github.com/omnicli
    trusted: true
```

## Environment

The environment variable `OMNI_ORG` can be used to add organizations as a comma-separated list. Any organization added through the `OMNI_ORG` environment variable will be automatically trusted, and will prepend the configuration list of organizations. The worktree can be specified for each organization by appending `=<worktree`> to each entry in the environment variable. e.g.

```bash
export OMNI_ORG=git@github.com:xaf=/home/xaf/my-stuff,github.com/omnicli
```
