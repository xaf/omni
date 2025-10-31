---
description: Builtin command `sandbox`
---

# `sandbox`

Create a sandbox directory pre-configured for omni.

Unless a specific path is provided, sandboxes are created under the directory configured by `sandbox` in the omni configuration (by default `~/sandbox`). The command writes a fresh `.omni.yaml` file populated with the dependencies you pass on the command line, initializes the work directory metadata, trusts the new sandbox, and (when shell integration is enabled) emits a `cd` into the sandbox so you are dropped inside it even if the automatic `omni up` fails.

The command aborts when the target already contains a git repository, an omni work directory, or an `.omni.yaml` file.

## Parameters

| Parameter | Required | Value type | Description |
|-----------|----------|------------|-------------|
| `--path`, `-p` | no | directory path | Create the sandbox at an explicit path. The directory (and any missing parents) are created automatically if they do not exist. The destination must not already contain an omni work directory, git repository, or `.omni.yaml` file. |
| `--name`, `-n` | no | string | Name of the sandbox directory to create under the sandbox root. |
| `--allow-empty` | no | `null` | Permit creating a sandbox without any dependencies. When omitted and no dependencies are provided, the command still succeeds but writes placeholder comments in `.omni.yaml`. |
| `dependencies…` | no | string list | Dependencies to place under the `up:` section of the generated `.omni.yaml`. Individual entries can pin versions (for example `go@1.21.1`). |

## Examples

```bash
# Create a sandbox under the configured sandbox root using an auto-generated name.
# The generated .omni.yaml contains the listed dependencies under the `up:` section.
omni sandbox node python

# Create a sandbox with a fixed name in the sandbox root.
omni sandbox --name hackday go@1.22.0 terraform@1.6.1

# Create a sandbox in an explicit directory. The directory must exist and be empty.
omni sandbox --path ~/tmp/scratchpad rust@1.75.0

# Creating a sandbox also runs `omni up` automatically and trusts the new workdir.
# If the shell integration is active you will end up in the sandbox directory even
# when `omni up` fails, allowing you to investigate or rerun the command manually.
```
