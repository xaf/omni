---
description: Configuration of the `up` parameter
slug: /reference/configuration/parameters/up
---

# `up`

## Parameters

List of operations needed to set up or tear down a repository.

Each entry in the list can either be a single string (loads the operation with default parameters), or a map where the key is the type of operation and the value parameters to pass to the operation. These are the available operations:

| Operation | Type | Description                                                    |
|-----------|------|---------------------------------------------------------|
| `and` | [and](up/and) | Run multiple operations in sequence |
| `any` | [any](up/any) | Run the first operation that succeeds and skip the rest, while considering [configured preferred tools](up_command) |
| `apt` | [apt](up/apt) | Install packages with `apt` for ubuntu and debian-based systems |
| `bash` | [bash](up/bash) | Install bash |
| `bundler` | [bundler](up/bundler) | Install dependencies with bundler |
| `cargo-install` | [cargo-install](up/cargo-install) | Install a tool using `cargo install` |
| `custom` | [custom](up/custom) | A custom, user-defined operation |
| `dnf` | [dnf](up/dnf) | Install packages with `dnf` for fedora-based systems |
| `github-release` | [github-release](up/github-release) | Install a tool from a GitHub release |
| `go` | [go](up/go) | Install go |
| `go-install` | [go-install](up/go-install) | Install a tool using `go install` |
| `homebrew`  | [Homebrew](up/homebrew) | Install formulae and casks with homebrew |
| `nix` | [nix](up/nix) | Install packages with `nix` |
| `node` | [node](up/node) | Install node |
| `or` | [or](up/or) | Run the first available operation that succeeds and skip the rest |
| `pacman` | [pacman](up/pacman) | Install packages with `pacman` for arch-based systems |
| `python` | [python](up/python) | Install python |
| `ruby` | [ruby](up/ruby) | Install ruby |
| `rust` | [rust](up/rust) | Install rust |

## Example

```yaml
up:
  - rust
  - go: latest
  - homebrew:
      tap:
        - xaf/omni
      install:
        - omni
  - custom:
      meet: echo "Installing something"
      unmeet: echo "Uninstalling something"
      met?: |
        if [[ $((RANDOM % 2)) == 0 ]]; then
          echo "Already installed"
          true
        else
          echo "Not installed"
          false
        fi
```
