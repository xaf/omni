# Phase D: runtime speed

**Goal:** shell prompt 13.6 ms → ~1-2 ms. Secondary: cheaper `omni help` and TAB completion.

**Status:** not started

## Measured baseline

| Scenario | Per invocation |
|---|---|
| `omni hook uuid` x200 (process floor) | **0.28 ms** |
| `omni hook env bash` x50, plain dir | 5.8 ms |
| `omni hook env bash` x50, with `OMNI_SHELL_PPID` (real hook) | 7.2 ms |
| **`omni hook env bash` x50, in `/app`** (git repo + `.omni.yaml` with `up:`) | **13.6 ms** |
| `omni help` x10 | 15.7 ms |
| `omni --complete ''` x20 (one TAB) | 12.3 ms |

The hook runs on **every prompt** in bash (`PROMPT_COMMAND`), zsh (`precmd_functions`), and fish (`--on-event fish_prompt --on-variable PWD` - so `cd` fires it twice). At 13.6 ms it is ~50x the process floor.

Everything below the floor is avoidable work, and **none of it is I/O-concurrency-bound** - it is CPU, allocation, and a handful of local syscalls. That is why async does not help here (see [`09-rejected.md`](09-rejected.md#rejected-wholesale-async-migration)).

## Estimated budget

| Change | Est. saving |
|---|---|
| Reorder `check_workdir_config_updated` env-var short-circuit | 3-5 ms in-repo |
| Memoize `up_hash()` | 2-4 ms in repos with `up:` |
| Skip clap construction for `hook env` | 3-4 ms |
| Gate `report_update_error()` behind a `stat()` | ~1.2 ms |
| `Arc<OmniConfig>` instead of deep clone | varies, large off-prompt |
| Lazy `peel_to_commit()` | 0.5-1 ms |
| SQLite WAL + `busy_timeout`, drop r2d2 | ~0.5 ms + removes fsync/BUSY tail latency |

## P0: the prompt path

### 1. Reorder `check_workdir_config_updated`

`src/internal/dynenv.rs:56-168`. Current order:

```
 62: workdir(&wdpath).id()                    // git discover
 72: let config = config(&wdpath);            // FULL config load + deep clone
 82: get_config_mod_times(&wdpath)            // + another workdir() inside (directory.rs:99)
 87: cache.get_env(&wdid)                     // SQLite query #1
122-129: flatten modtimes, blake3 hash
134: if env_var == hashed { return; }         // ← THE SHORT-CIRCUIT, 70 lines too late
147: cache.get_env(&wdid)                     // SQLite query #2 (identical!)
148: config.up_hash()                         // re-serializes to YAML
```

The hash compared at line 134 depends only on `wdid` + `modtimes`. Reorder to `wdid → modtimes → hash → compare → return` so the steady-state prompt (the overwhelmingly common case) skips the config parse **and** both DB queries.

### 2. Memoize `up_hash()`

`config/parser/omniconfig.rs:178-200` calls `feuilletage::to_yaml` on `up`, `suggest_config`, `suggest_clone`, and `env` **on every call**. This is the measured difference between 13.6 ms in `/app` and 7.9 ms in a trivial repo. Wrap in a `OnceCell` on `OmniConfig`.

### 3. Dedupe the identical `get_env` calls

`dynenv.rs:87` and `:147` issue the same `cache.get_env(&wdid)` query.

### 4. Gate `report_update_error()`

`git/updater.rs:228-246` → `cache/omnipath.rs:55-107` opens SQLite and runs a **write transaction** (SELECT + DELETE + COMMIT) on every prompt.

Verified empirically: with `OMNI_SHELL_PPID` unset the cache DB is not created; with it set - exactly what the shell templates do - the 290 KB DB appears. So the real hook pays this every prompt.

It is a "was there a background update error" flag. Gate behind a one-`stat()` sentinel (a marker file), or only check when the dynamic env actually changed.

**Coordinate with [`04-error-reporting.md`](04-error-reporting.md)**, which changes how update errors are stored (stop destroying the pointer on read). Do these together.

### 5. Skip clap for `hook env`

`commands/builtin/hook/env.rs:139-142` calls `exec_parse_args_typed`, which builds a fresh `clap::Command` (`config/parser/command_definition.rs:510-555`) including a `SyntaxOptArgType::Enum(Shell::all())`, for **3 flags**. Hand-parse them, as `hook uuid` effectively does.

Keep the declared syntax for `omni help hook env` - only the runtime parse changes.

### 6. `Arc<OmniConfig>` from `config()` / `global_config()`

`config/parser/root.rs:19-32` memoizes per path but **returns a full deep clone every call**, plus a `canonicalize()` syscall (`:23`) and a `workdir()` lookup (`:73`). `OmniConfig` (`config/parser/omniconfig.rs:66-130+`) has ~22 fields including `HashMap<String, CommandDefinition>`, `Vec<OrgConfig>`, and the whole `UpConfig`.

54 `config(".")` and 65 `global_config()` call sites. Worst offender, `commands/loader.rs:455`:

```rust
.filter(|command| command.score > config(".").command_match_min_score)
```

One full clone **per candidate command** - 228 clones to read a single `f64`. Three more at `:465-468`.

Return `Arc<OmniConfig>` (or `&'static` via `OnceLock`). Broad payoff, mechanical change, also removes the per-call mutex and canonicalize.

### 7. Lazy `peel_to_commit()`

`env.rs:781-785` does `head().peel_to_commit()` in `GitRepoEnv::new` - an object-DB read the prompt path never consumes. `id` is already lazy via `OnceCell` at `:764`; make `commit` match.

This is also the reason git2 stays rather than being replaced by subprocesses - see [`09-rejected.md`](09-rejected.md#rejected-removing-git2--vendored-libgit2).

### 8. SQLite pragmas, drop r2d2

Verified on the live DB: `journal_mode=delete`, `synchronous=2 (FULL)`, **no `busy_timeout`**. `grep -rn "PRAGMA"` in `cache/` finds only `user_version`.

Set at pool init (`cache/database/pool.rs:70-94`):

```
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA busy_timeout=3000;
```

Without `busy_timeout`, concurrent omni processes (several terminals hooking at once) get immediate `SQLITE_BUSY`, silently swallowed by `.unwrap_or_default()` (`cache/omnipath.rs:44,107`) - so lock contention degrades into "the update check never runs" with no diagnostic.

Also replace `r2d2` (`max_size(10)`, `pool.rs:83-86`) with a `OnceLock<Connection>`. Connection pooling buys nothing in a single-shot CLI that exits milliseconds later.

**Caution:** WAL creates `-wal` and `-shm` sidecar files. Confirm nothing assumes a single-file cache (backup, `omni tidy`, cleanup paths), and that WAL works on network filesystems where a home directory might live. Consider keeping `delete` mode as a fallback if `WAL` cannot be set.

## P1: off-prompt startup

### 9. Cache omnipath command discovery

`commands/frompath.rs:174` does a full recursive `WalkDir` with `follow_links(true)` and **no `max_depth`** per omnipath entry, an `is_executable()` stat per file (`:182`), a sort (`:190`), then `canonicalize` per command (`:286-293`). **No `path_commands` table exists** - verified across all 86 files in `cache/database/sql/`.

Runs on every invocation and every TAB press. Cache keyed on directory mtimes.

`follow_links(true)` with no depth limit is also a symlink-cycle risk - add `max_depth`.

### 10. `ensure_bootstrap` should not parse config

`config/bootstrap.rs:11-22` calls `OmniConfigLoader::new_global()`, loading and parsing **all** global config files - a second full parse on every interactive invocation - only to ask `has_user_config()`. Replace with `Path::exists()` checks over `user_config_files()`.

### 11. Cheapen `handle_shims()`

`config/up/utils/shims.rs:23-75` runs unconditionally from `main.rs:320`. When `argv[0]` has no `/` (the normal PATH case) it stats **every `$PATH` entry** (`:34-38`) to detect shim invocation. Compare against `current_exe()` (already cached in `CURRENT_EXE`, `env.rs:60-67`) and `shims_dir()` (`env.rs:362-364`, env-var only), or short-circuit when `argv0 == "omni"`.

### 12. Fix the self-update early return

`git/updater.rs:371` intends to skip when the omnipath is empty and self-update checking is off, but `PathRepoUpdatesSelfUpdateEnum::default()` is `Ask` when the `self-update-check` feature is on (default, `Cargo.toml:22-25`), so `do_not_check()` is false and the early return never fires. Every interactive invocation therefore reaches `should_update()` → `try_exclusive_update()` → SQLite transaction.

Fix the condition, and move the check into the existing background mechanism (`trigger_background_update`, `:248-278`) so it never blocks a foreground command. Add a timeout to the self-updater HTTP client ([`05-network-resilience.md`](05-network-resilience.md)).

### 13. Hoist per-call regex compiles

- `filter_control_characters` (`user_interface/print.rs:184-187`) - called **once per output line** during `omni up` streaming (guarded by `run_config.strip_ctrl_chars`), so a long build pays a regex compile per line
- `strip_colors` (`user_interface/colors.rs:66-69`)

Both should use `lazy_static`, as `strip_ansi_codes` already correctly does (`print.rs:176-182`). Also `frommakefile.rs:92`, `bootstrap.rs:552`, `status.rs:272`.

### 14. One shared tokio runtime

**7 `Runtime::new()` sites**, each building a fresh **multi-threaded** runtime (`Runtime::new()` is `new_multi_thread().enable_all()`), spawning worker threads per call:

`up/utils/askpass.rs:78,154`; `up/utils/progress_handler.rs:40,69,79`; `git/utils.rs:51,74`

`progress_handler.rs:40` is inside `run_progress`, called **once per command execution** during `up` - so a build with many commands churns many runtimes.

Replace with a single lazily-built **current-thread** runtime. The async surface is only 13 `async fn` / 42 `.await`, all of it subprocess-I/O multiplexing and FIFO listeners, none of it needing multiple worker threads.

**Interacts with [`01-binary-size.md`](01-binary-size.md#5-narrow-tokio-features)**: the narrowed tokio feature list omits `rt-multi-thread`, so this change is a prerequisite for that one. Land them together.

## Verification

- [ ] Prompt-latency benchmark added to CI as a regression gate ([`08-ci.md`](08-ci.md))
- [ ] `omni hook env bash` in `/app` measurably under ~3 ms
- [ ] Steady-state prompt performs **zero** SQLite queries (verify by instrumenting or by checking the DB is untouched)
- [ ] base62 / cache-key derivation unchanged - config and env hashes must stay stable, or every user's cached environment is orphaned
- [ ] Concurrent prompts in several terminals do not produce `SQLITE_BUSY`
- [ ] WAL sidecar files do not break backup/cleanup paths
- [ ] `omni --complete ''` measurably faster with the discovery cache; correct after adding/removing an omnipath command

## Measured: musl is not slower than glibc here. It is faster.

First local musl build (musl-tools + musl-dev now available; CI's recipe is
`musl-tools` plus `RUSTFLAGS=-C prefer-dynamic=no`). Four paired runs of
`bench-hook-env.sh` on the same machine, same conditions:

| run | gnu total | musl total | gnu floor | musl floor |
|-----|----------:|-----------:|----------:|-----------:|
| a   | 2.61 ms   | 2.24 ms    | 1.57 ms   | 0.46 ms    |
| b   | 3.38 ms   | 1.97 ms    | 0.84 ms   | 0.43 ms    |
| c   | 2.46 ms   | 2.02 ms    | 0.82 ms   | 0.70 ms    |
| d   | 2.36 ms   | 2.19 ms    | 0.83 ms   | 0.57 ms    |

musl wins all four. It is also far more consistent: 1.97-2.24 ms against
gnu's 2.36-3.38 ms.

Static linking is why. musl's startup floor is roughly half gnu's (~0.5 ms
vs ~0.83 ms) because there is no dynamic loader and no relocation work. For
a process the shell spawns on every prompt, startup dominates.

And the "musl work is slower" part does not survive repetition either.
Overhead above the floor was gnu 1.53-2.54 ms against musl 1.32-1.62 ms --
overlapping ranges. The single-run figures that suggested otherwise (gnu
1.04, musl 1.78) were noise.

### The premise behind this investigation was invalid

The 4.7x gap that motivated looking at musl allocators compared **local gnu
(1.61 ms) against CI musl (7.51 ms)**. Those are different machines. It
measured GitHub runner speed, not libc.

Consequences:

- **No case for mimalloc or jemalloc.** There is no gap to close. It would
  add a native C library -- which `check-linked-libraries.sh` would flag,
  correctly -- reversing part of the 7 -> 5 reduction, for a benefit that
  does not exist on this workload. See [`09-rejected.md`](09-rejected.md).
- **No case for `-C target-cpu` tuning** on this basis. That evidence was
  x86_64-specific anyway, while the gap was claimed on aarch64.
- **The latency check must stay report-only**, but for a better reason than
  "no musl baseline exists": the benchmark's own variance is larger than any
  effect worth gating on. gnu ranged 1.61-3.38 ms for one unchanged binary,
  a >2x spread. Any threshold tight enough to catch a regression would fire
  on noise.

### If runtime speed is revisited

Measure paired, same-machine, repeated -- never one run, and never across
machines. The floor/overhead split in `bench-hook-env.sh` is the useful part;
it separates startup cost from work and would have caught this immediately
had it been read across repeats rather than once.
