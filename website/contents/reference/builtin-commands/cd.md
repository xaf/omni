---
description: Builtin command `cd`
---

# `cd`

Change directory to the git directory of the specified work directory.

If no repository or path is specified, change to the first organization's worktree, or defaults to the default worktree.

The `...` special path can also be used to change to the root of the current work directory.

This command supports a wide variety of git URL formats, including web URLs from popular git hosting platforms (GitHub, GitLab, Bitbucket, Gitea, etc.). When a web URL is provided with a file path, the command will navigate to the appropriate directory. Line numbers in URLs are also preserved for use with the `--edit` flag.

## Parameters

| Parameter       | Required | Value type | Description                                         |
|-----------------|----------|------------|-----------------------------------------------------|
| `--locate` | no | `null` | If provided, will only return the path to the repository instead of switching directory to it. When this flag is passed, interactions are also disabled, as it is assumed to be used for command line purposes. This will exit with 0 if the repository is found, 1 otherwise. |
| `--edit` | no | `null` | If provided, will open the work directory or file in the editor specified by `VISUAL` or `EDITOR` environment variables, or fallback to vim or nano if available. When this flag is passed with a web URL containing a file path and line numbers, the editor will open at the specified location. Interactions are also disabled. |
| `--[no-]include-packages` | no | `null` | If provided, overrides the default behavior of considering or not packages when calling the command. When using `--locate`, packages will by default be included, otherwise they won't. |
| `repo` | no | string | The name of the repo to change directory to; this can be in the format of a full git URL, web URL (from GitHub, GitLab, Bitbucket, etc.), or `<org>/<repo>`, or just `<repo>`, in which case the repo will be searched for in all the organizations in the order in which they are defined, and then trying all the other repositories in the configured worktrees. |

## Examples

```bash
# Let's say:
#  - our worktree is /home/xaf/git
#  - we cloned omni to /home/xaf/git/github.com/xaf/omni

# We can cd using a full git URL
omni cd https://github.com/xaf/omni  # CWD: /home/xaf/git/github.com/xaf/omni

# Or using parts of the repo identifier -- this is faster if matched with an organization
omni cd xaf/omni  # CWD: /home/xaf/git/github.com/xaf/omni
omni cd omni      # CWD: /home/xaf/git/github.com/xaf/omni

# Will switch to the root of the first organization's worktree, or to the
# root of the default worktree if no organization is configured
omni cd  # CWD: /home/xaf/git

# Will act like the regular `cd` command if provided with paths
omni cd ~               # CWD: /home/xaf
omni cd relative/path   # CWD: /home/xaf/relative/path
omni cd -               # CWD: /home/xaf
omni cd /absolute/path  # CWD: /absolute/path
omni cd ..              # CWD: /absolute

# Will return the matching directory for the repository
omni cd --locate xaf/omni  # stdout: /home/xaf/git/github.com/xaf/omni ; exit code: 0
omni cd --locate unknown   # exit code: 1

# Will change to the root of the current work directory
# if CWD is /home/xaf/git/github.com/xaf/omni/relative/path
omni cd ...  # CWD: /home/xaf/git/github.com/xaf/omni

# Will error out if not in a work directory
omni cd ...  # exit code: 1

# Web URLs with directories
omni cd https://github.com/xaf/omni/tree/main/src/internal
# CWD: /home/xaf/git/github.com/xaf/omni/src/internal

# Web URLs with file paths will error (can't cd to a file)
omni cd https://github.com/xaf/omni/blob/main/src/main.rs

# Using --edit to open a file from a web URL
omni cd --edit https://github.com/xaf/omni/blob/main/README.md
# Opens README.md in your editor

# Using --edit with line numbers from a web URL
omni cd --edit https://github.com/xaf/omni/blob/main/src/main.rs#L50
# Opens src/main.rs at line 50 in your editor

# Using --edit with line ranges
omni cd --edit https://github.com/xaf/omni/blob/main/src/main.rs#L50-L60
# Opens src/main.rs at line 50 in your editor
```

### Git reference checking

When using a web URL that specifies a git reference (branch, tag, or commit), omni will check if your local repository is on the same reference. If there's a mismatch:

- In interactive mode, you'll see a warning and be prompted to continue
- In non-interactive mode (with `--locate` or `--edit`, or when not in an interactive shell), the operation will fail with an error

This helps ensure you're working with the correct version of the code when navigating from URLs.
