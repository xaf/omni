---
description: Configuration of the `github-release` kind of `up` parameter
---

# `github-release` operation

Install a tool from a GitHub release.

For this to work properly for a GitHub release, it will need to:
- Be provided as a `.tar.gz` or `.zip` archive, or as a binary file (no extension)
- Have a file name that contains hints about the OS it was built for (e.g. `linux`, `darwin`, ...)
- Have a file name that contains hints about the architecture it was built for (e.g. `amd64`, `arm64`, ...)

Omni will download all the assets matching the current OS and architecture, verify checksums, extract them and move all the found binary files to a known location to be loaded in the repository environment.

:::info
If using a ARM Mac (M1, M2, etc.) with Rosetta installed, omni will try to download the `amd64` version of the asset if the `arm64` version is not available.
:::

:::note
This supports authenticated requests using [the `gh` command line interface](https://cli.github.com/) if it is installed and authenticated, which allows for a higher rate limit and access to private repositories, as well as GitHub Enterprise instances. See the `auth` parameter to override the default behavior.
:::

## Alternative names

- `ghrelease`
- `github_release`
- `githubrelease`
- `github-releases`
- `ghreleases`
- `github_releases`
- `githubreleases`

## Parameters

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `dir` | path | Relative path (or list of relative paths) to the directory in the project for which to use this tool |
| `repository` | string | The name of the repository to download the release from, in the `<owner>/<name>` format; can also be provided as an object with the `owner` and `name` keys |
| `version` | string | The version of the tool to install; see [version handling](#version-handling) below for more details. |
| `upgrade` | boolean | whether or not to always upgrade to the most up to date matching release, even if an already-installed version matches the requirements *(default: false)* |
| `prerelease` | boolean | Whether to download a prerelease version or only match stable releases; this will also apply to versions with prerelease specification, e.g. `1.2.3-alpha` *(default: `false`)* |
| `build` | boolean | Whether to download a version with build specification, e.g. `1.2.3+build` *(default: `false`)* |
| `binary` | boolean | Whether to download an asset that is not archived and consider it a binary file *(default: `true`)* |
| `immutable` | boolean | Whether to only match releases marked as immutable by GitHub. Immutable releases provide enhanced supply chain security by preventing modifications after publication. When set to `true`, only releases marked as immutable will be considered; when set to `false`, both immutable and non-immutable releases are accepted *(default: `false`)* |
| `asset_name` | string | The name of the asset to download from the release. All assets matching this pattern _and_ the current platform and architecture (unless skipped) will be downloaded. It can take glob patterns, e.g. `*.tar.gz` or `special-asset-*`. It can take multiple patterns at once, one per line, and accepts positive and negative (starting by `!`) patterns. The first matching pattern returns (whether negative or positive). If not set, will be similar as being set to `*` |
| `skip_os_matching` | boolean | Whether to skip the OS matching when downloading assets. If set to `true`, this will download all assets regardless of the OS *(default: `false`)* |
| `skip_arch_matching` | boolean | Whether to skip the architecture matching when downloading assets. If set to `true`, this will download all assets regardless of the architecture *(default: `false`)* |
| `prefer_dist` | boolean | Whether to prefer downloading assets with a `dist` tag in the name, if available; when set to `false`, will prefer downloading assets without `dist` in the name *(default: `false`)* |
| `api_url` | string | The URL of the GitHub API to use, useful to use GitHub Enterprise (e.g. `https://github.example.com/api/v3`); defaults to `https://api.github.com` |
| `checksum` | object | The configuration to verify the checksum of the downloaded asset; see [checksum configuration](#checksum-configuration) below |
| `auth` | [`Auth`](../github#auth-object) object | The configuration to authenticate the GitHub API requests for this release; if specified, will override the global configuration |
| `env` | object | Environment variables to set when using this release; see the [`env` parameter documentation](../env) for syntax details, and the [environment variables context](#environment-variables-context) section below for special context variables |

### Checksum configuration

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `enabled` | boolean | Whether to verify the checksum of the downloaded asset; if set to `true`, the checksum will be verified and the operation will fail if the checksum is not valid *(default: `true`)* |
| `required` | boolean | Whether the checksum verification is required; if set to `true`, the operation will fail if the checksum cannot be verified *(default: `false`)* |
| `algorithm` | string | The algorithm to use to verify the checksum; can be `md5`, `sha1`, `sha256`, `sha384`, or `sha512`; if not set, will try to automatically detect the algorithm based on the checksum length |
| `value` | string | The value of the checksum to verify the downloaded asset against; if not set, will try to automatically find the asset containing the checksum in the GitHub release |
| `asset_name` | string | The name of the asset containing the checksum to verify the downloaded asset against. It can take glob patterns, e.g. `*.md5` or `checksum-*`. It can take multiple patterns at once, one per line, and accepts positive and negative (starting by `!`) patterns. The first matching pattern returns (whether negative or positive). If not set, will be similar as being set to `*` |

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

The version also supports the `||` operator to specify ranges. This operator is not compatible with the `latest` keywords. For instance, `1.2.x || >1.3.5 <=1.4.0` will match any version between `1.2.0` included and `1.3.0` excluded, or between `1.3.5` excluded and `1.4.0` included.

The latest version satisfying the requirements will be installed.

### Environment variables context

In order to simplify setting environment variables for tools installed via GitHub releases, the environment variables values support some context variables.

| Variable | Description | Example |
|----------|-------------|---------|
| `install_dir` | The installation directory path for this release | `/Users/user/.local/share/omni/ghreleases/owner/repo/v1.2.3` |

## Supply chain verification

Omni provides multiple layers of verification to ensure the integrity and authenticity of downloaded GitHub releases:

### Checksum verification

Omni can verify the integrity of downloaded assets by computing and comparing checksums. When enabled (which is the default), omni will automatically look for checksum files in the release assets (such as `checksums.txt`, `SHA256SUMS`, etc.) and use them to verify downloaded files.

For enhanced security, you can provide a checksum value directly in the configuration using the `checksum.value` parameter. This is more secure than relying on checksum files from the release itself, as those files could theoretically be altered by an attacker who compromised the release. By providing the checksum in your configuration, you ensure it comes from a trusted source.

See the [checksum configuration](#checksum-configuration) section for details on configuration options.

### Immutable release verification

When a GitHub release is marked as [immutable](https://github.blog/news-insights/product-news/github-immutable-releases-are-generally-available/), omni will automatically verify the cryptographic signature of downloaded assets using `gh release verify-asset` (if [the `gh` command line interface](https://cli.github.com/) is available).

The cryptographic verification ensures:
- The asset was published by the repository owner
- The asset has not been modified since publication
- The asset's signature is valid

If the `gh` CLI is not available, a warning will be displayed but the installation will continue. If verification fails when `gh` is available, the installation will be aborted to protect against tampered releases.

:::info
The `immutable` parameter controls which releases are considered during version matching (when `true`, only immutable releases), while the verification described here applies to any release that GitHub marks as immutable. To take full advantage of immutable release verification, it is recommended to set `immutable` to `true` whenever a repository provides immutable releases.
:::

## Examples

```yaml
up:
  # Will error out since no repository is provided
  - github-release

  # Will install the latest release of the `omni` tool
  # from the `xaf/omni` repository
  - github-release: xaf/omni

  # We can call it with any of the alternative names too
  - ghrelease: xaf/omni
  - github_release: xaf/omni
  - githubrelease: xaf/omni
  - github-releases: xaf/omni
  - ghreleases: xaf/omni
  - github_releases: xaf/omni
  - githubreleases: xaf/omni

  # Will also install the latest version
  - github-release:
      repository: xaf/omni
      version: latest

  # Will install any version starting with 1.20
  - github-release:
      repository: xaf/omni
      version: 1.2

  # Will install any version starting with 1
  - github-release:
      repository: xaf/omni
      version: 1

  # Full specification of the parameter to identify the version;
  # this will install any version starting with 1.2.3
  - github-release:
      repository: xaf/omni
      version: 1.2.3

  # Will install any version starting with 1, including
  # any pre-release versions
  - github-release:
      repository: xaf/omni
      version: 1
      prerelease: true

  # Will only install releases marked as immutable by GitHub
  # for enhanced supply chain security
  - github-release:
      repository: xaf/omni
      version: latest
      immutable: true

  # Will install all the specified releases
  - github-release:
      xaf/omni: 1.2.3
      omnicli/omni:
        version: 4.5.6
        prerelease: true

  # Will install all the listed releases
  - github-release:
      - xaf/omni: 1.2.3
      - repository: omnicli/omni
        version: 4.5.6

  # Will only download *.tar.gz assets, even if other assets
  # are matching the current OS and arch
  - github-release:
      repository: xaf/omni
      asset_name: "*.tar.gz"

  # Will download assets even if OS and arch are not matching
  - github-release:
      repository: xaf/omni
      asset_name: "cross-platform-binary"
      skip_os_matching: true
      skip_arch_matching: true

  # Use this tool only in the specified directory
  - github-release:
      repository: xaf/omni
      version: 1.2.3
      dir: some/specific/dir

  # Use this tool in multiple directories
  - github-release:
      repository: xaf/omni
      version: 1.2.3
      dir:
        - some/specific/dir
        - another/dir

  # Set custom environment variables for an SDK-like tool
  - github-release:
      repository: xaf/omni
      version: 1.2.3
      env:
        ROOT_DIR: "{{ install_dir }}"
        SPECIAL_PATH:
          prepend: "{{ install_dir }}/bin"
```

## Dynamic environment

The following variables will be set as part of the [dynamic environment](/reference/dynamic-environment).

| Environment variable | Operation | Description |
|----------------------|-----------|-------------|
| `CPLUS_INCLUDE_PATH` | prepend | C++ header files directory (when `include/` directory detected) |
| `C_INCLUDE_PATH` | prepend | C header files directory (when `include/` directory detected) |
| `DYLD_LIBRARY_PATH` | prepend | Dynamic library path for shared libraries (macOS only, when `lib/` directory detected) |
| `LD_LIBRARY_PATH` | prepend | Dynamic library path for shared libraries (Linux only, when `lib/` directory detected) |
| `MANPATH` | prepend | Manual pages directory (when `man/` directory detected) |
| `PATH` | prepend | Injects the path to the binaries of the installed tool |
