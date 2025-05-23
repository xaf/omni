---
description: Builtin command `cd`
---

# `cd`

Change directory to the git directory of the specified work directory.

If no repository or path is specified, change to the first organization's worktree, or defaults to the default worktree.

The `...` special path can also be used to change to the root of the current work directory.

## Parameters

| Parameter       | Required | Value type | Description                                         |
|-----------------|----------|------------|-----------------------------------------------------|
| `--locate` | no | `null` | If provided, will only return the path to the repository instead of switching directory to it. When this flag is passed, interactions are also disabled, as it is assumed to be used for command line purposes. This will exit with 0 if the repository is found, 1 otherwise. |
| `--[no-]include-packages` | no | `null` | If provided, overrides the default behavior of considering or not packages when calling the command. When using `--locate`, packages will by default be included, otherwise they won't. |
| `repo` | no | string | The name of the repo to change directory to; this can be in the format of a full git URL, or `<org>/<repo>`, or just `<repo>`, in which case the repo will be searched for in all the organizations in the order in which they are defined, and then trying all the other repositories in the configured worktrees. |

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
```

