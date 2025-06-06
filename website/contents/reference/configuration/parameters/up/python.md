---
description: Configuration of the `python` kind of `up` parameter
---

# `python` operation

Installs python to be used by the current repository.

:::note
Omni uses [`mise`](https://mise.jdx.dev/) in the background to manage that tool. The `mise` installation of omni is in its own directory, and won't conflict with any installation of `mise` you might have on your system.
:::

## Parameters

The following parameters can be used:

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `dir` | path | Relative path (or list of relative paths) to the directory in the project for which to use the python version; each specified directory will have its own virtual environment. |
| `pip` | path/boolean | Controls dependency installation. If set to `true` or `auto` (default), omni will try to install dependencies from well-known requirement files in each specified `dir` (or in each discovered directory with `version: auto`) if they exist. If set to `false`, no dependency installation will be performed. Can also be a relative path (or list of paths) to specific requirements files to be used with `pip install -r`. |
| `url` | string | The URL to download the tool from, in case the tool is not registered in [the `mise` registry](https://github.com/jdx/mise/blob/main/registry.toml) or if you want to use a custom version. |
| `version` | string | The version of the tool to install; see [version handling](#version-handling) below for more details. |
| `upgrade` | boolean | whether or not to always upgrade to the most up to date matching version, even if an already-installed version matches the requirements *(default: false)* |

### Version handling

The following strings can be used to specify the version:

| Version | Meaning |
|---------|---------|
| `1.2`     | Accepts `1.2` and any version prefixed by `1.2.*` |
| `1.2.3`   | Accepts `1.2.3` and any version prefixed by `1.2.3.*` |
| `~1.2.3`  | Accepts `1.2.3` and higher patch versions (`1.2.4`, `1.2.5`, etc. but not `1.3.0`) |
| `^1.2.3`  | Accepts `1.2.3` and higher minor and patch versions (`1.2.4`, `1.3.1`, `1.4.7`, etc. but not `2.0.0`) |
| `>1.2.3`  | Must be greater than `1.2.3` |
| `>=1.2.3` | Must be greater or equal to `1.2.3` |
| `<1.2.3`  | Must be lower than `1.2.3` |
| `<=1.2.3` | Must be lower or equal to `1.2.3` |
| `1.2.x`   | Accepts `1.2.0`, `1.2.1`, etc. but will not accept `1.3.0` |
| `*`       | Matches any version (same as `latest`, except that when `upgrade` is `false`, will match any installed version) |
| `latest`  | Latest release (when `upgrade` is set to `false`, will only match with installed versions of the latest major) |
| `auto`    | Lookup for any version files in the project directory (`.tool-versions` or `.python-version`) and apply version parsing |

The version also supports the `||` operator to specify ranges. This operator is not compatible with the `latest` and `auto` keywords. For instance, `1.2.x || >1.3.5 <=1.4.0` will match any version between `1.2.0` included and `1.3.0` excluded, or between `1.3.5` excluded and `1.4.0` included.

The latest version satisfying the requirements will be installed.

### Dependencies management

Omni uses [`uv`](https://github.com/astral-sh/uv) for package installation, a fast Python package installer and resolver, written in Rust. It will be used automatically to install user-specified dependencies through the `pip` parameter.

When using the `pip: auto` parameter, omni will automatically detect, parse and install packages from the dependency files in the following order:
1. `poetry.lock`
2. `Pipfile.lock`
3. `pyproject.toml`
4. `requirements.txt`
5. `Pipfile`

:::info
The version of `uv` used can be configured through the [`uv_version`](/reference/configuration/parameters/up_command) parameter in the omni configuration file.
:::

## Examples

```yaml
up:
  # Will install the latest version of python
  - python

  # And also
  - python: latest

  # Let omni lookup for version files in the project
  - python: auto

  # Will install any version starting with 3.11, and containing
  # only dots and numbers after
  - python: 3.11

  # Will install any version starting with 3, and containing only
  # dots and numbers after
  - python: 3

  # Full specification of the parameter to identify the version;
  # this will install any version starting with 3.11.4, and
  # containing only dots and numbers after
  - python:
      version: 3.11.4

  # Use that version but only in the some/sub/dir directory
  - python:
      version: 3.11.4
      dir: some/sub/dir

  # This will install python 3.11.4 and run
  # pip install -r req.txt
  - python:
      version: 3.11.4
      pip: req.txt

  # This will install python 3.11.4 and run
  # pip install -r requirements.txt for each of dir1 and dir2
  - python:
      version: 3.11.4
      dir:
        - dir1
        - dir2
      pip: auto

  # Disable dependency installation
  - python:
      version: 3.11.4
      pip: false

  # Let omni lookup for version files in the project,
  # and run `pip install -r requirements.txt` in each
  # of the directories identified with a version file
  - python:
      version: auto
      pip: auto
```

## Dynamic environment

The following variables will be set as part of the [dynamic environment](/reference/dynamic-environment).

| Environment variable | Operation | Description |
|----------------------|-----------|-------------|
| `PATH` | prepend | The `bin` directory for the loaded version of python |
| `POETRY_CACHE_DIR` | set | Set to an environment-specific directory for isolation |
| `POETRY_CONFIG_DIR` | set | Set to an environment-specific directory for isolation |
| `POETRY_DATA_DIR` | set | Set to an environment-specific directory for isolation |
| `PYTHONHOME` | unset | |
| `VIRTUAL_ENV` | set | The path to the python virtual environment |
