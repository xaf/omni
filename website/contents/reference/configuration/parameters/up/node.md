---
description: Configuration of the `node` kind of `up` parameter
---

# `node` operation

Installs node to be used by the current repository.

:::note
Omni uses [`mise`](https://mise.jdx.dev/) in the background to manage that tool. The `mise` installation of omni is in its own directory, and won't conflict with any installation of `mise` you might have on your system.
:::

## Alternative names

- `nodejs`

## Parameters

The following parameters can be used:

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `dir` | path | Relative path (or list of relative paths) to the directory in the project for which to use the node version |
| `install_engines` | boolean | If set to `true`, the engines specified in the `package.json` file will be installed. *(default: `true`)* |
| `install_packages` | boolean | If set to `true`, the packages specified in the `package.json` file will be installed, using the preferred package manager between `npm`, `yarn` and `pnpm`. The preferred package manager is determined from any lock file found in the project directory, as well as from the `engines` section of the `package.json` file. *(default: `true`)* |
| `url` | string | The URL to download the tool from, in case the tool is not registered in [the `mise` registry](https://github.com/jdx/mise/blob/main/registry.toml) or if you want to use a custom version. |
| `version` | string | The version of the tool to install; see [version handling](#version-handling) for more details. |
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
| `auto`    | Lookup for any version files in the project directory (`.tool-versions`, `.node-version`, `.nodejs-version`, `package.json` or `.nvmrc`) and apply version parsing |

The version also supports the `||` operator to specify ranges. This operator is not compatible with the `latest` and `auto` keywords. For instance, `1.2.x || >1.3.5 <=1.4.0` will match any version between `1.2.0` included and `1.3.0` excluded, or between `1.3.5` excluded and `1.4.0` included.

The latest version satisfying the requirements will be installed.

## Examples

```yaml
up:
  # Will install the latest version of node
  - node

  # Will also install the latest version of node
  - nodejs

  # And also
  - node: latest

  # Let omni lookup for version files in the project
  - node: auto

  # Will install any version starting with 20.3, and containing
  # only dots and numbers after
  - node: 20.3

  # Will install any version starting with 20, and containing only
  # dots and numbers after
  - node: 20

  # Full specification of the parameter to identify the version;
  # this will install any version starting with 20.3.1, and
  # containing only dots and numbers after
  - node:
      version: 20.3.1

  # Use that version but only in the some/sub/dir directory
  - node:
      version: 20.3.1
      dir: some/sub/dir

  # Disable the installation of engines and packages
  - node:
      version: auto
      install_engines: false
      install_packages: false
```

## Dynamic environment

The following variables will be set as part of the [dynamic environment](/reference/dynamic-environment).

| Environment variable | Operation | Description |
|----------------------|-----------|-------------|
| `PATH` | prepend | The `bin` directory for the loaded version of node |
