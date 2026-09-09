# Phase A: enforce no dynamic linking

**Goal:** turn "the binary must be self-sufficient" from a convention into an invariant CI enforces.

**Status:** not started

## Why this exists

Today the property is maintained by two conventions and nothing checks the result:

- `RUSTFLAGS: "-C prefer-dynamic=no"` (`.github/workflows/build-and-test-target.yaml:129,142`)
- musl targets for Linux (`build.yaml:50-65`), which are static by construction
- `openssl = { features = ["vendored"] }` and `libz-sys = { features = ["static"] }` (`Cargo.toml:53,63`), added specifically to force static linking

Nothing verifies the outcome. A transitive dependency picking up a system library, a feature flip, or a `vendored` feature being dropped during the Phase A dependency work would ship a binary that fails on a user's machine with a missing `.so`. This gate must land **before** the dependency changes in [`01-binary-size.md`](01-binary-size.md), because it is what makes those changes safe to attempt.

## Current state, measured

```
$ file /usr/local/bin/omni
ELF 64-bit LSB executable, ARM aarch64, version 1 (SYSV), statically linked, not stripped
$ ldd /usr/local/bin/omni
	not a dynamic executable
```

So Linux is genuinely fully static today. That is the property to lock in.

## The check must be per-platform

**Fully static is impossible on macOS.** `libSystem` is always dynamically linked, and Apple does not support static linking of the system libraries. So the invariant differs by target:

| Target family | Invariant |
|---|---|
| `*-unknown-linux-musl` | **zero** dynamic dependencies: no `DT_NEEDED` entries, no `PT_INTERP` |
| `*-apple-darwin` | **only** an allowlist: `/usr/lib/libSystem.B.dylib` and `/System/Library/Frameworks/*` |

Anything outside the macOS allowlist fails the build. In particular these must never appear:
`libssl`, `libcrypto`, `libgit2`, `libz`, `libsqlite3`, `liblzma`, `libcurl`, and anything under `/opt/homebrew` or `/usr/local/opt` (Homebrew paths - a real risk, since the build machine has brew installed and `pkg-config` can find its libraries).

## Implementation

A single script, `.github/scripts/check-static-linking.sh`, taking the binary path and the target triple.

**Linux (musl):** prefer `readelf` over `ldd`, because `ldd` output text varies across libc implementations while ELF headers do not.

```sh
# fail if any DT_NEEDED entry exists
readelf -d "$BIN" | grep -q 'NEEDED' && fail "dynamic dependency found"
# fail if a program interpreter is set
readelf -l "$BIN" | grep -q 'INTERP' && fail "interpreter found (not static)"
```

**macOS:** parse `otool -L` and reject anything not matching the allowlist.

```sh
otool -L "$BIN" | tail -n +2 | awk '{print $1}' | while read -r lib; do
  case "$lib" in
    /usr/lib/libSystem.B.dylib) ;;
    /System/Library/Frameworks/*) ;;
    *) fail "disallowed dynamic dependency: $lib" ;;
  esac
done
```

Note `otool -L` line 1 is the binary's own name, hence `tail -n +2`. Also check `otool -l | grep LC_RPATH` is empty - an `@rpath` entry would mean the binary expects to find libraries at runtime.

### Bonus assertion: no OpenSSL banner

If the openssl experiment in [`01-binary-size.md`](01-binary-size.md#2-the-openssl-experiment) concludes with removal, add:

```sh
strings "$BIN" | grep -q "OpenSSL [0-9]" && fail "OpenSSL re-entered the binary"
```

This prevents a transitive dependency silently reintroducing it later. **Only add this after V2 passes** - it would fail the build today, correctly, since OpenSSL 3.5.4 is currently present.

## CI wiring

Insert into `.github/workflows/build-and-test-target.yaml` immediately after the build step (`:137-150`) and **before** packaging (`:154`), signing (`:164-172`), and upload. A binary that fails the gate must never be packaged or released.

The existing `timeout-minutes: 30` on the build step is unaffected; the check takes under a second.

Also run it in PR CI, not only on release. A `--release` build on the PR path exercises the same link configuration, so regressions get caught before they reach a release. If the profile split from [`08-ci.md`](08-ci.md) means PR builds use `release` while releases use `dist`, run the gate in **both** - link configuration can in principle differ between profiles (e.g. `lto` changing symbol resolution).

## Verification

- [ ] Script fails correctly on a deliberately dynamic binary (test with a trivial `cargo build` without `prefer-dynamic=no`, or on a system `/bin/ls`)
- [ ] Script passes on the current `/usr/local/bin/omni`
- [ ] Wired into `build-and-test-target.yaml` before packaging
- [ ] Runs on all four targets: `{aarch64,x86_64}-unknown-linux-musl`, `{aarch64,x86_64}-apple-darwin`
- [ ] Runs on the PR path too
- [ ] Failure message names the offending library and the target, so the cause is obvious from the log

## Open question

macOS `aarch64` and `x86_64` are cross-built via `houseabsolute/actions-rust-cross@v1` (`build-and-test-target.yaml:99-101,137`). Confirm `otool` is available in that job, or use `llvm-otool` / `objdump -p` as a fallback. If neither is available in the cross container, the macOS half of the gate may need to run on a native `macos-latest` runner instead.
