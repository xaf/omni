# Findings and measured baseline

All measurements taken 2026-09-09 on the dev container (ARM aarch64, Linux), against commit `efe6129`.
Citations are `file:line` against that commit. **Re-verify before acting** if line numbers no longer match.

## Measured baseline

| Metric | Value | How measured |
|---|---|---|
| Binary size | **28 MB** (28,912,144 bytes) | `ls -la /usr/local/bin/omni` |
| Linking | fully static, `not a dynamic executable` | `file`, `ldd` |
| Dependency count | **382 unique packages** | `Cargo.lock` |
| Crates invoking a C/C++ compiler | **12** | `aws-lc-sys`, `blake3`, `cmake`, `libgit2-sys`, `liblzma-sys`, `libsqlite3-sys`, `libz-sys`, `openssl-src`, `openssl-sys`, `ring`, `sqlite-wasm-rs`, `zstd-sys` |
| Process floor (`omni hook uuid` x200) | **0.28 ms** | shell loop, wall time / 200 |
| `omni hook env bash` x50, plain dir | 5.8 ms | as above |
| `omni hook env bash` x50, with `OMNI_SHELL_PPID` | 7.2 ms | this is what the real shell hook does |
| **`omni hook env bash` x50, in `/app`** | **13.6 ms** | git repo + 5 KB `.omni.yaml` containing `up:` |
| `omni help` x10 | 15.7 ms | |
| `omni --complete ''` x20 | 12.3 ms | one TAB press |
| Source lines | 70,782 across 169 `.rs` files | ~23.4% is test code, correctly `#[cfg(test)]` gated |

Note the shell prompt hook costs **~25-50x the process floor**, and runs on every prompt.

## Build configuration

**`Cargo.toml` contains no `[profile.*]` section at all.** Verified: `grep -n "profile" Cargo.toml` matches only `[package.metadata.cargo-machete]` (`Cargo.toml:102`) and `[package.metadata.binstall]` (`Cargo.toml:105`).

So `--release` uses Cargo defaults: `opt-level=3`, `lto=false`, `codegen-units=16`, `panic=unwind`, no `strip`. **No LTO means no cross-crate dead-code elimination across 382 crates** - the single largest missed lever.

CI release build (`​.github/workflows/build-and-test-target.yaml:137-150`) passes only `--locked --release` plus `RUSTFLAGS: "-C prefer-dynamic=no"` and `strip: true`. No `lto`, no `codegen-units`, no section GC, no `upx`.

Targets (`build.yaml:50-65`): `{aarch64,x86_64}-unknown-linux-musl` (inherently static) and `{aarch64,x86_64}-apple-darwin`.

## Issue 1: `omni up` aborts everything when one tool fails

Reported symptom: `rust` installs successfully, `node` fails on `npm install`, and the two later steps (`bats`, `sqruff`) never run. Afterwards, rust is **not usable** either.

This is **two independent defects**.

### Defect A: fail-fast step loop

`src/internal/config/up/base.rs:87-126` is a plain sequential loop. Line 116:

```rust
step.up(options, environment, &progress_handler)?
```

The `?` returns on the first `Err`, abandoning all remaining steps. There is no error accumulator, no `Vec<UpError>` collection, no per-step tolerance.

Confirmed absent by exhaustive grep over `src/internal/config/up/` and `src/internal/commands/builtin/up.rs`: no `fail_fast`, `continue_on_error`, `ignore_errors`, `optional`, `allow_failure`, or `best_effort`. `UpCommandConfig` (`src/internal/config/parser/up_command.rs:29-59`) exposes no tolerance knob.

`num_steps = steps.len() + 2` (`base.rs:99`) - the `+2` are the synthetic trailing steps, which is why 4 configured tools display as `/6`.

### Defect B: all-or-nothing commit (the one that actually bites)

The environment is committed **only after the loop completes**, at `base.rs:120`:

```
[1/6] rust   ─┐
[2/6] node   ─┤  loop mutates `environment` IN MEMORY only
[3/6] bats   ─┤
[4/6] sqruff ─┘
[5/6] apply environment:   ← base.rs:120 assign_environment()  ← THE ONLY COMMIT POINT
[6/6] resources cleanup:   ← base.rs:123
```

The comment at `base.rs:150` states the intent explicitly: *"Assign the version id to the workdir now that we have successfully set it up"*.

So when node fails, after a successful rust step:

| Artifact | State |
|---|---|
| rust toolchain files on disk | persisted (mise installed them) |
| `mise_installed` row for rust | persisted (`mise.rs:1329`) |
| `environment.versions[]` entry for rust | **in-memory only** (`mise.rs:1341`) |
| `up_environments` workdir assignment | **not written** (needs `base.rs:120`) |
| `mise_installed_required_by` row | **not written** (needs `base.rs:168` → `mise.rs:1396`) |

The shell reads only the committed environment (`src/internal/dynenv.rs:376`), so **rust is installed but unusable**.

Worse: `commit()` (`base.rs:201`) is the only writer of `required_by`. With no `required_by` row, the working rust install matches the GC predicate in `src/internal/cache/database/sql/mise_operation_list_removable.sql` once `cleanup_after` elapses (default **604800 s / 1 week**, `src/internal/config/parser/cache/mise.rs:22,29`). So a successful install gets **uninstalled** if the failure persists.

Note this is a **non-commit, not a rollback**: `clear_cache()` (`base.rs:78-85`) is called only from `up.rs:1478` and `up.rs:1628` (the `down` path), never on failure. A previously-working environment survives intact.

### Existing seams to build on

- `Or` / `Any` composites already tolerate failure: `src/internal/config/up/tool.rs:290-301` and `:315-328`.
- `UpError::StepFailed(String, Option<(usize,usize)>)` (`src/internal/config/up/error.rs:16-20`) already models "error already rendered, carry only identity upward" - used at exactly one site, `custom.rs:97`.
- `is_available()` (`tool.rs:416-425`) already drops steps silently, but only `homebrew` and `nix` ever return false.

### Hard constraint found

`UpConfigMise` is **single-shot**. `mise.rs:1361-1363`:

```rust
if self.up_succeeded.get().is_some() {
    return Err(UpError::Exec("up operation already attempted".to_string()));
}
```

Any retry design must clone or rebuild the config, not re-invoke `up()` on the same instance.

## Issue 2: duplicated error line

Reported output shows `[2/6] ✖ node (auto):` twice. Root cause: **two `error_with_message()` calls on the same handler**.

1. `src/internal/config/up/nodejs.rs:409` renders, then `:410` returns `Err`
2. `src/internal/config/up/mise.rs:1716` renders again as `format!("error: {err}")`, then `:1717` re-returns
3. `src/internal/commands/builtin/up.rs:1604` prints a third time as `omni: up command failed: ...`

`error_with_message` (`src/internal/config/up/utils/up_progress_handler.rs:252-261`) has **no already-ended guard** and is not idempotent.

Nested `UpError` `Display` also double-wraps: `UpError::Exec` formats as `"execution error: {0}"` (`error.rs:8-9`), producing `execution error: failed to install packages: execution error: process exited with status 1`.

This render-then-propagate shape appears at **13 sites**: `mise.rs:1460,1594,1705,1716,2073`; `nodejs.rs:373,409`; `base.rs:155,160,169`; `nix.rs:543`; `custom.rs:95`.

Because `error_with_message` also writes `SyncUpdateProgressAction::Error` to the lock file (`up_progress_handler.rs:255`), the duplication is in the **wire protocol** too - an attached second `omni up` replays both records.

## Issue 3: no network resilience anywhere

Only **3 HTTP call sites** in the codebase, plus the self-updater:

| Site | Purpose |
|---|---|
| `src/internal/config/up/github_release.rs:1178` | GitHub releases list (paginated) |
| `src/internal/config/up/github_release.rs:1522` | asset download |
| `src/internal/config/up/cargo_install.rs:952` | crates.io versions |
| `src/internal/self_updater.rs:155` | `reqwest::blocking::get`, bare |

For all of them:

- **No retry, no backoff.** The only `retry`-named code is FIFO/IPC (`up/utils/fifo_handler.rs:170,177,200`), unrelated.
- **No timeouts.** No `.timeout()`, `.connect_timeout()`, or `.read_timeout()` on any client (`github_release.rs:1132`, `cargo_install.rs:939`, `self_updater.rs:155`). reqwest blocking defaults to infinite, so a stalled socket wedges `omni up` forever. `RunConfig` timeouts apply to subprocesses only and never reach reqwest.
- **No rate-limit handling.** `grep -rn "429|rate.limit|403|Retry-After|X-RateLimit" --include=*.rs .` returns **zero hits repo-wide**, tests included. A rate-limit 403 is reported identically to a 404 (`github_release.rs:1203-1215`). Notably the client *does* read the `link` header for pagination (`:1191-1197`) but no rate-limit headers.
- **No `GITHUB_TOKEN` default.** Auth defaults to `gh auth token` (`src/internal/config/parser/github.rs:80-84`). `grep -rn "GITHUB_TOKEN"` matches only the memo-cache identifier at `github_release.rs:88,93,98`. In CI, where `GITHUB_TOKEN` exists but `gh` often does not, omni runs **unauthenticated at 60 req/hr** - and `list_releases_from_api` paginates at `per_page=100` with no pacing or page cap (`:1170-1230`).
- **Three ad-hoc clients**, one built *per asset* (`:1519`), so no connection pooling and a TLS handshake per download.
- **No resume**: `truncate(true)` (`:1549-1554`), no `Range` header, no `.part` file.
- **Checksum mismatch hard-fails** without re-download (`:1673-1681`) - exactly the case a retry fixes.

Three degradation behaviours that partially mask failures (useful, but not retries): fall back to stale cache (`:986-996`), re-fetch once on cache staleness (`:864-882`), fall back to an already-installed version (`:899-925`).

### Adjacent bugs found

- `cargo_install.rs:873` reads `config.cache.go_install.versions_expire` - copy-paste; should be `cargo_install`. Invisible today because both default to 86400.
- `self_updater.rs:161-164` `.expect("Failed to read response")` **panics** on a mid-stream network drop, and the code never checks `response.status()` (`:155-172`), so an error body is fed to `serde_json` and silently becomes "no update available".

## Issue 4: shell prompt latency

`omni hook env` runs on **every prompt** in all three shells (`templates/shell_integration.{bash,zsh,fish}.tmpl`); fish also fires on `--on-variable PWD`, so a `cd` triggers it twice.

Main costs, in order:

1. **clap parser construction per invocation** - `src/internal/commands/builtin/hook/env.rs:139-142` calls `exec_parse_args_typed`, which builds a fresh `clap::Command` (`config/parser/command_definition.rs:510-555`) for 3 flags.
2. **`check_workdir_config_updated` checks its cache too late** - `src/internal/dynenv.rs:56-168`. The `WD_CONFIG_MODTIME_VAR` short-circuit is at line 134, *after* a full config load (line 72) and a SQLite query (line 87). The hash it compares (line 129) depends only on `wdid` + `modtimes`, both obtainable first.
3. **`up_hash()` re-serialises config to YAML on every call** - `config/parser/omniconfig.rs:178-200`. This is why `/app` costs 13.6 ms versus 7.9 ms for a trivial repo.
4. **`config()` returns a full deep clone** - `config/parser/root.rs:19-32`. 54 `config(".")` and 65 `global_config()` call sites. Worst case `commands/loader.rs:455` clones the entire `OmniConfig` **once per candidate command** (228 clones) to read one `f64`.
5. **`report_update_error()` opens SQLite and runs a write transaction every prompt** - `git/updater.rs:228-246` → `cache/omnipath.rs:55-107`. Verified empirically: `~/.cache/omni/cache.db` is created only when `OMNI_SHELL_PPID` is set, which is exactly what the shell templates do.
6. **Duplicate identical query** - `cache.get_env(&wdid)` at `dynenv.rs:87` and again at `:147`.
7. **`GitRepoEnv::new` does an eager `peel_to_commit()`** - `env.rs:781-785`, an object-DB read the prompt path never consumes. `id` is already lazy via `OnceCell` at `:764`; `commit` is not.
8. **`handle_shims()` stats every `$PATH` entry** on every invocation - `config/up/utils/shims.rs:34-38`.
9. **`ensure_bootstrap()` does a second full config parse** on every interactive run - `config/bootstrap.rs:11-22` - where a `Path::exists()` check would do.

### SQLite configuration

Verified on the live DB: `journal_mode=delete`, `synchronous=2 (FULL)`, **no `busy_timeout`**. `grep -rn "PRAGMA"` in `src/internal/cache/` finds only `user_version`. Consequences: multiple fsyncs per write, and concurrent omni processes get immediate `SQLITE_BUSY`, silently swallowed by `.unwrap_or_default()` (`cache/omnipath.rs:44,107`).

`r2d2` pool with `max_size(10)` (`cache/database/pool.rs:83-86`) is pure overhead for a single-shot CLI. The pool itself is correctly lazy (`pool.rs:70-94`).

`CacheManager::get()` uses `.expect("Failed to create cache manager")` (`cache/database/manager.rs:19`), and `pool.rs:78-90` uses `.expect()` for `create_dir_all`, pool build, connection, and migration. **A corrupt or unwritable cache DB therefore panics on every shell prompt.**

### Command discovery is uncached

`commands/frompath.rs:174` does a full recursive `WalkDir` with `follow_links(true)` and **no `max_depth`** over every omnipath entry, `is_executable()` stat per file (`:182`), then `canonicalize` per command (`:286-293`). There is **no `path_commands` table** - verified across all 86 files in `cache/database/sql/`. This runs on every invocation and every TAB press.

## Issue 5: log files leak permanently

The `omni-exec.<timestamp>.<rand>` path in the reported error comes from `config/up/utils/progress_handler.rs:217-231` (created) and `:332-336` (kept on failure). It lands in `std::env::temp_dir()`.

`tmpdir_cleanup()` (`src/internal/env.rs:113-124`) only globs `TMPDIR_CLEANUP_PREFIX` = `omnitmp-<8-hex>` (`env.rs:74-83`). `omni-exec.*` and `omni-update.*` do not match, and `NamedTempFile::keep()` removes them from Drop-based cleanup. **Every failed step leaks a permanent file into `$TMPDIR`.** There is an in-source `TODO` acknowledging exactly this at `progress_handler.rs:330-331`.

There is **no `omni logs` command** (`commands/builtin/mod.rs` inventory). And `report_update_error` **deletes the DB pointer as it prints** (`cache/omnipath.rs:74-88`), so a background-update error is shown exactly once, ever.

## Issue 6: CLI ergonomics

- **`omni help` writes to stderr.** Verified: `omni help > f` produces **0 bytes** on stdout; stderr gets 1258. All of `print_global_help`/`print_command_help` use `eprintln!` (`commands/builtin/help.rs:266-391`). Meanwhile `omni help --output json` correctly uses stdout. So `omni help | less` and `omni help | grep` silently fail.
- **Global flags list is incomplete and hand-maintained.** `GLOBAL_OPTIONS` (`help.rs:30-51`) lists 3 flags; `MainArgs::parse` (`main.rs:39-83`) parses about 9. Missing: `-h/--help`, `--version`, `-e`, `-l/--local`, `-A/--askpass`, `--update-and-log-on-error`. (issue #599)
- **`--unfold` is not advertised** in global help despite `cmd.num_folded()` already being computed at `help.rs:574`. (issue #413)
- **Unknown flags report as unknown commands.** `omni --bogus-flag` prints `omni: command not found: --bogus-flag` because the positional uses `allow_hyphen_values(true)` (`main.rs:82`), handled at `main.rs:293-298`.
- **Fuzzy suggestions are gated on `shell_is_interactive()`** (`commands/loader.rs:349`), so scripts and pipes get no hint.
- **No shell completion command.** No `clap_complete` dependency. Completion arrives only as a side effect of `omni hook init`, and the bash implementation no-ops on bash < 4 (still stock on macOS).
- **`COMP_CWORD` underflow**: `main.rs:183-185` does `.parse().unwrap_or(0) - 1`, which underflows when `COMP_CWORD=0`.
- **No `--dry-run`** anywhere. `grep -rn "dry.run|dry_run" src/` finds one comment at `git/updater.rs:390`. `omni up` installs toolchains and `omni tidy` moves git repos.
- **No logging framework.** `grep -rn "RUST_LOG|env_logger|tracing::|log::" src/` returns **zero**. So **`CLAUDE.md`'s "Run with debug: `RUST_LOG=debug`" is false.** No global `--verbose`/`--quiet`. A stray `dbg!` sits in production at `self_updater.rs:167`.
- **Colors**: `NO_COLOR` and `CLICOLOR_FORCE` are respected (`user_interface/colors.rs:72-84`), but **`TERM=dumb` and `CLICOLOR=0` are not**. The `force_*` API (`colors.rs:101-147`) bypasses the check entirely and is used throughout `dynenv.rs:152-155,250,253`, so dynamic-env messages ignore `NO_COLOR`.
- **Per-call regex compiles on hot paths**: `filter_control_characters` (`user_interface/print.rs:184-187`) is called **once per output line** during `omni up`; `strip_colors` (`colors.rs:66-69`) likewise. Compare `strip_ansi_codes` (`print.rs:176-182`), which correctly uses `lazy_static`.
- **Panic risk walking to `/`**: `commands/frommakefile.rs:44-46` `unwrap()`s on `read_dir` and each entry while walking parents; outside a workdir it walks to `/`, so an unreadable intermediate directory hard-panics `omni <anything>`.

## Dead weight and hygiene

- **Untracked debris**: `config-value/` (**104 MB**, contains only `target/`, no `Cargo.toml`), `petname/` (empty), `wt/` (only `.DS_Store`). None are path dependencies - `Cargo.toml` has zero `path =` deps. `config-value` is dead since the feuilletage migration (`be19f36`); it appears in neither `Cargo.toml` nor `Cargo.lock`.
- **`cargo machete` runs in CI** (`tests.yaml:185-209`, `--with-metadata` at `:209`) and is a required gate (`:279,291`), but `Cargo.toml:102-103` suppresses `libz-sys` and `openssl` via `ignored`.
- **23 crates compile at two or three versions**, including `syn` (2.x and 3.x), and a RustCrypto 0.10/0.11 split duplicating `digest`, `block-buffer`, `crypto-common`, `sha1` - caused by `zip` pinning `sha1 0.10` while omni uses `sha1 0.11`.
- **`sqlite-wasm-rs`** (a `cc` crate) is pulled by `rusqlite` and compiled on native targets.
- **`r2d2_sqlite` and `rusqlite` both declare `bundled`** (`Cargo.toml:70,74`). Harmless - feature unification means SQLite compiles once - but redundant.
- **`build.rs`** has a `[build-dependencies] time` with `serde-well-known` (`Cargo.toml:27`) for a fallback timestamp; `std::time` would remove the build-dep entirely. It also emits `rerun-if-env-changed` (`build.rs:7`) but **no `rerun-if-changed` for git state**, which disables Cargo's default directory heuristic and makes the embedded version go stale unpredictably.

## Dependency usage census

Counts are usages in `src/`, excluding `*_test.rs`.

| Crate | Usages | Assessment |
|---|---|---|
| `requestty` | 132 / 15 files | essential |
| `time` | 74 / 27 files | essential (but see `chrono` below) |
| `rusqlite` | 57 / 15 files | essential |
| `clap` | 52 / 3 files | essential; `"string"` needed for runtime-built commands |
| `tokio` (`features=["full"]`) | 46 / 20 files | **over-featured**; only `process,time,rt,io-util,net,sync,macros` used. No `#[tokio::main]`/`#[tokio::test]` exist |
| `tera` | 38 / 8 files | keep; v2 is lean (single dep: `serde`) |
| `reqwest` (`["blocking"]`) | 19 / 3 files | **over-featured**; pulls `quinn` (QUIC), `h2`, `wasm-bindgen`/`js-sys`/`web-sys`, `rustls-platform-verifier`, and **both** `aws-lc-rs` and `ring` |
| `indicatif` | 13 / 6 files | keep |
| `git2` (`vendored-libgit2`) | **9 / 4 files** | see [`09-rejected.md`](09-rejected.md) - keeping it |
| `blake3` | 9 / 6 files | keep |
| `uuid` | 9 / 8 files | keep |
| `zip` | **1** (`github_release.rs:1852`) | default features drag `bzip2`, `zstd`, `zopfli`, `ppmd-rust`, `deflate64`, `aes`, `pbkdf2`, `hmac`, duplicate `sha1 0.10` |
| `flate2` / `liblzma` / `tar` | 1 / 1 / 2 | all in the same extraction block, `github_release.rs:1898-1907` |
| `futures` | 3 / 2 files | facade crate; `askpass.rs:9` is just `std::future::Future`, `listener_manager.rs:6,145` need `select_all`/`join_all` |
| `num-bigint` + `num-integer` + `num-traits` | 3 crates, **1 file** | `utils/base62.rs` (33 lines) on a fixed 32-byte BLAKE3 digest. Callers: `up/utils/directory.rs:29`, `env.rs:991` |
| `md-5` / `sha1` / `sha2` | checksum algos | all reachable; algorithm inferred from hex length (`github_release.rs:2308-2314`) |
| `duct` | **1** | audit for removal |
| `machine-uid` / `gethostname` / `whoami` | 1 each | `env.rs:1152`, `env.rs:15`, `env.rs:476`. Overlapping; `nix` is already a dep |
| `chrono` | transitive via `feuilletage` | **a second full date/time library compiles** alongside `time`. Not locally fixable |
| `openssl` (`vendored`) | 0 direct | see [`09-rejected.md`](09-rejected.md) for the real story |

## Parallelism in `up`

There is none. `base.rs:100` is a plain synchronous `for`; `up()` is not `async`. No `rayon`, no `par_iter`, no `tokio::spawn` of steps. Tokio is used only *inside* a single command execution (`up/utils/progress_handler.rs:40-41` builds a fresh `Runtime` per invocation and `block_on`s it) to `select!` over stdout/stderr.

Structural blockers to step parallelism, if ever wanted:

1. `std::env::set_current_dir` per step (`base.rs:103`) - process-global mutable state inside the loop
2. `environment: &mut UpEnvironment` - exclusive borrow shared by every step
3. Genuine sequential dependency - `tool.rs:276-280` reloads the dynamic env from accumulated state before each step
4. `Runtime::new()` per command
5. Progress rendering assumes linear `[n/m]` advance. Note `SpinnerProgressHandler::new_with_multi` (`up/utils/spinner_progress_handler.rs:30-36`) accepts an `indicatif::MultiProgress` and is **never called** - a dormant seam for concurrent multi-line progress

Phase B does **not** introduce parallel execution. "Parallel lifecycles" means independent commit and failure domains, still executed sequentially.
