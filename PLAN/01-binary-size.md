# Phase A: binary size

**Goal:** 28 MB → ~12-15 MB, without introducing any dynamic linking.

**Status:** not started

Every item is measured **independently**. Bundled dependency changes hide regressions, and we need to know which lever delivered what (see the openssl/LTO interaction below).

## Measurement protocol

Before starting, record the baseline on the build machine:

```sh
cargo build --release
ls -l target/release/omni          # bytes
cargo bloat --release --crates -n 40 > PLAN/measurements/baseline-bloat.txt
```

After each change, re-measure and append a row to the table below. Keep the raw
`cargo bloat` output under `PLAN/measurements/` so deltas are attributable.

Sizes below are **local `aarch64-unknown-linux-gnu` host builds**, not the shipped musl target. Absolute numbers are not comparable to the 28 MB shipped binary; deltas are what matter. Re-measure on musl in CI.

| # | Change | Size | Delta | Notes |
|---|---|---|---|---|
| - | baseline (`--release`) | 25.09 MB | - | 26,304,032 bytes |
| 1 | `[profile.dist]` lto=fat, cgu=1, strip | **15.97 MB** | **-36.3%** | 16,748,376 bytes. Verified working. [`measurements/a1-profile-dist.txt`](measurements/a1-profile-dist.txt) |
| 2 | openssl removed | 15.97 MB | **0 B** | build time -22s (~10%), 5 crates dropped. [`measurements/a2-openssl-removal.txt`](measurements/a2-openssl-removal.txt) |
| 3 | reqwest features | | | |
| 4 | zip features | | | |
| 5 | tokio features | | | |
| 6 | base62 hand-roll | | | |
| 7 | drop `futures` facade | | | |
| 8 | shared tokio runtime | | | see [`06-runtime-speed.md`](06-runtime-speed.md) |

## 1. Add `[profile.dist]`

`Cargo.toml` currently has **no `[profile.*]` section at all** ([`00-findings.md`](00-findings.md#build-configuration)), so `--release` runs with `lto=false` and `codegen-units=16` - no cross-crate dead-code elimination across 382 crates. This is the single largest lever.

Per decision **D7**, the expensive settings go in a *separate* profile so PR CI and local dev stay fast:

```toml
[profile.dist]
inherits = "release"
lto = "fat"
codegen-units = 1
strip = "symbols"
```

`panic = "abort"` is **excluded** - see [`09-rejected.md`](09-rejected.md#rejected-panic--abort).

**Done.** Measured **-36.3%** (25.09 MB → 15.97 MB) on a local gnu build, with `--version`, `help` and `status` all verified working. Cold `dist` build took 3m44s, which is the cost being isolated away from PR CI.

Consider testing `opt-level = "s"` as a separate measured variant. It trades runtime speed for size, and given goal 2 is *faster*, only adopt it if the size win is large and the prompt-latency gate ([`06-runtime-speed.md`](06-runtime-speed.md)) shows no regression.

CI wiring, packaging path changes (`target/dist/` not `target/release/`), and the double-compile fix live in [`08-ci.md`](08-ci.md).

## 2. The openssl experiment

**DONE - removed.** Full evidence in [`measurements/a2-openssl-removal.txt`](measurements/a2-openssl-removal.txt). Summary:

The decisive check turned out to need **no build at all**. `cargo tree --locked --target <triple> -i openssl-sys` resolves the graph per target, and on all three shipped targets the only path is `openssl-sys → openssl → omnicli`. Nothing else - not libgit2-sys, not reqwest, and `native-tls` is absent from the lock entirely.

Corroborated by the link behaviour: on gnu, `vendored` **did** engage and built `libcrypto.a` (15 MB) + `libssl.a` (2.6 MB) with `rustc-link-lib=static`, yet the binary contained **zero** openssl symbols and no banner. Static archives only contribute members that resolve an undefined symbol, so 17.6 MB was built and wholly discarded - a direct demonstration that no reference exists.

**Correction to the original plan:** this saves **build time, not size.** Measured size delta was exactly **0 bytes**, because the linker was already dropping it. The 36.3% reduction came entirely from LTO (item 1). The win here is not compiling a vendored OpenSSL 3.6.0 on every clean build: **-22s (~10%)** on this many-core host, likely more on a CI runner.

The shipped binary's `OpenSSL 3.5.4` banner is explained by it being an **older build**, from before `bc4273a` removed the libgit2 edge - the version mismatch against today's 3.6.0 is the tell.

`openssl-probe` correctly remains: it is pure Rust that only locates the system CA store for `rustls-native-certs`.

**Three variants, measured separately**, because LTO may strip most of openssl on its own and we need to know which lever did the work:

| Variant | Build | Must verify |
|---|---|---|
| V0 | baseline | size, `ldd`/`otool`, HTTPS smoke |
| V1 | `+ [profile.dist]` (item 1) | does the OpenSSL text drop out unaided? `strings \| grep "OpenSSL 3"` |
| V2 | `+ openssl removed from Cargo.toml` | all of the below |

**V2 acceptance criteria - all must pass, or openssl stays:**

1. HTTPS works against every real endpoint omni uses:
   - `api.github.com` (releases list, `github_release.rs:1178`)
   - GitHub asset download (`github_release.rs:1522`)
   - `crates.io` (`cargo_install.rs:952`)
   - `raw.githubusercontent.com` (self-updater, `self_updater.rs:155`)
2. **Zero new dynamic dependencies** on every target - enforced by the gate in [`02-static-linking.md`](02-static-linking.md)
3. Local git operations still work: `Repository::discover`, `find_branch`, ignore-status checks (the 5 git2 call sites in [`00-findings.md`](00-findings.md#dependency-usage-census))
4. Size delta recorded

Also remove `"openssl"` from the `cargo-machete` ignore list (`Cargo.toml:102-103`) if it goes. **Keep `"libz-sys"`** - that entry is legitimate, since `Cargo.toml:53` exists solely to force `features = ["static"]` for `libgit2-sys`'s zlib.

If V2 fails, record the failure mode in [`09-rejected.md`](09-rejected.md) and move on. The LTO win from V1 stands either way.

## 3. Narrow `reqwest` features

`Cargo.toml:73` is `features = ["blocking"]` with **default features on**. Only 19 usages across 3 files, all blocking ([`00-findings.md`](00-findings.md#issue-3-no-network-resilience-anywhere)), yet the resolved graph pulls:

- `quinn` + `quinn-proto` + `quinn-udp` - QUIC/HTTP3, for a tool making 3 blocking GETs
- `h2` - HTTP/2
- `wasm-bindgen`, `js-sys`, `web-sys`, `wasm-bindgen-futures` - browser targets
- `rustls-platform-verifier` → `jni`, `security-framework`, `core-foundation`, `windows-sys`
- **both** `aws-lc-rs` (→ `aws-lc-sys`, a large C/asm build) **and** `ring`

Proposed:

```toml
reqwest = { version = "0.13.4", default-features = false, features = ["blocking", "rustls-tls"] }
```

Consider `rustls-tls-webpki-roots` instead if dropping `rustls-platform-verifier` is acceptable. **Trade-off to decide explicitly:** platform-verifier uses the OS trust store (respects corporate/custom CAs); webpki-roots bundles a fixed root set (smaller, but ignores system CAs). For a dev tool behind corporate TLS interception, platform-verifier is the safer default. Measure both, prefer platform-verifier unless the delta is large.

**Verify:** all four HTTPS endpoints from item 2 still work, and only one crypto backend remains in `Cargo.lock`.

## 4. Narrow `zip` features

**One usage** - `github_release.rs:1852`. Default features drag in `bzip2`, `zstd` (+`zstd-sys`, a C build), `zopfli`, `ppmd-rust`, `deflate64`, `aes`, `pbkdf2`, `hmac`, and a duplicate `sha1 0.10` that forces a second copy of `digest`/`block-buffer`/`crypto-common` to compile alongside omni's `sha1 0.11`.

```toml
zip = { version = "4.6.1", default-features = false, features = ["deflate"] }
```

**Verify:** extracting a real `.zip` release asset still works. Check whether any tracked `github-release` config in the wild uses a non-deflate zip - if `.zip` assets with bzip2/zstd entries exist, this breaks them. Deflate is overwhelmingly standard for release archives, but confirm against the `github_release` bats fixtures before landing.

Note `flate2` (gzip), `liblzma` (xz), and `tar` are used in the same extraction block (`github_release.rs:1898-1907`) and all stay.

## 5. Narrow `tokio` features

`Cargo.toml:93` is `features = ["full"]`. The complete set of tokio APIs actually used (census in [`00-findings.md`](00-findings.md#dependency-usage-census)):

| API | Feature |
|---|---|
| `tokio::process::Command` (19 uses) | `process` |
| `tokio::select!` (4) | `macros` |
| `tokio::time::{sleep,Duration,timeout}` (5) | `time` |
| `tokio::runtime::{Runtime,Handle}` (5) | `rt` |
| `tokio::io::{BufReader,AsyncReadExt,AsyncBufReadExt,AsyncWriteExt}` (7) | `io-util` |
| `tokio::net::{UnixStream,UnixListener}` (2) | `net` |
| `tokio::sync::Mutex` (1) | `sync` |

```toml
tokio = { version = "1.53.1", features = ["process", "time", "rt", "io-util", "net", "sync", "macros"] }
```

No `#[tokio::main]` or `#[tokio::test]` exists anywhere (`grep -rn "#\[tokio::"` → 0 hits).

**Interacts with item 8**: if the shared-runtime change in [`06-runtime-speed.md`](06-runtime-speed.md) moves to a current-thread runtime, `rt-multi-thread` is not needed either - and it is not in the list above, so this narrowing already assumes that. Land item 8 first or together, otherwise `Runtime::new()` (which is multi-thread) will fail to compile. **That is a useful forcing function, not a problem.**

## 6. Hand-roll base62

`num-bigint` + `num-integer` + `num-traits` (`Cargo.toml:59-61`) exist for **one 33-line file**, `src/internal/utils/base62.rs`, with two callers: `up/utils/directory.rs:29` and `env.rs:991`. Both encode a **fixed 32-byte BLAKE3 digest** and take `[..20]` of the result.

A fixed-width 32-byte → base62 conversion needs no bignum library. Implement with a simple repeated-division over a `[u8; 32]` buffer.

**Verify:** this is a **cache-key function** - `directory.rs` and `env.rs` use it to derive on-disk paths. The new implementation **must produce byte-identical output** for the same input, or every existing user's cached environments and data paths are orphaned. Add a test with known input/output pairs captured from the current implementation *before* replacing it.

## 7. Drop the `futures` facade

Three usages ([`00-findings.md`](00-findings.md#dependency-usage-census)):

- `up/utils/askpass.rs:9` - `use futures::Future`, which is just `std::future::Future`
- `up/utils/listener_manager.rs:6` - `futures::future::select_all`
- `up/utils/listener_manager.rs:145` - `futures::future::join_all`

`futures` is the facade crate, pulling `futures-executor`, `futures-sink`, `futures-task`, `futures-io`, `futures-channel`, and the `futures-macro` proc-macro. Replace with `futures-util` (`default-features = false`) or `tokio::task::JoinSet`, and change the first to `std::future::Future`.

## 8. Repo hygiene

- `rm -rf config-value/ petname/ wt/` - untracked debris, **104 MB** of dev disk, all confirmed non-dependencies ([`00-findings.md`](00-findings.md#dead-weight-and-hygiene)). No binary-size effect; removes misleading structure.
- Drop the redundant `bundled` on `r2d2_sqlite` (`Cargo.toml:70`) - harmless today thanks to feature unification, but it implies SQLite compiles twice, which it does not.
- Drop the `[build-dependencies] time` (`Cargo.toml:27`) in favour of `std::time` in `build.rs`, and add `cargo:rerun-if-changed=.git/HEAD` - see [`08-ci.md`](08-ci.md).

## Deferred, needs upstream

- **`chrono` and `time` both compile.** `chrono` arrives only via `feuilletage` (`Cargo.lock`: `chrono: <- ['feuilletage']`). Not fixable locally; check whether `feuilletage` can make it optional.
- **`sqlite-wasm-rs`** (a `cc` crate) is pulled by `rusqlite` and built on native targets. Upstream concern.
- **23 crates at multiple versions**, including `syn` 2.x + 3.x. Mostly transitive; item 4 fixes the RustCrypto split.

## Verification checklist

Before declaring Phase A done:

- [ ] `cargo build --profile dist` succeeds on all four targets
- [ ] Size gate in CI passing, baseline recorded ([`08-ci.md`](08-ci.md))
- [ ] Static-linking gate passing on all four targets ([`02-static-linking.md`](02-static-linking.md))
- [ ] Prompt-latency gate recorded, no regression ([`06-runtime-speed.md`](06-runtime-speed.md))
- [ ] All four HTTPS endpoints verified working
- [ ] base62 output byte-identical to pre-change (test committed)
- [ ] `cargo test` green; bats suite green
- [ ] `cargo machete` green with `openssl` removed from the ignore list, or openssl retained with the reason recorded
