# Phase B: error reporting and logs

**Goal:** one error renders once, and the log it points at is findable later.

**Status:** not started

Lands with [`03-up-lifecycles.md`](03-up-lifecycles.md) - the lifecycle summary needs clean single-render errors to be readable.

## Problem 1: errors render two or three times

Reported output:

```
[2/6] ✖ node (auto): failed to install packages: execution error: process exited with status 1; log is available at /var/.../omni-exec.20260908T234324Z.bt1Lr1
[2/6] ✖ node (auto): error: execution error: failed to install packages: execution error: process exited with status 1; log is available at /var/.../omni-exec.20260908T234324Z.bt1Lr1
omni: up command failed: issue while setting repo up: execution error: failed to install packages: ...
```

Three renders of one failure:

| # | Site | Shape |
|---|---|---|
| 1 | `src/internal/config/up/nodejs.rs:409` | `error_with_message(msg)` then `:410` returns `Err` |
| 2 | `src/internal/config/up/mise.rs:1716` | `error_with_message(format!("error: {err}"))` then `:1717` re-returns |
| 3 | `src/internal/commands/builtin/up.rs:1604` | `omni: up command failed: ...` |

Both handler calls receive the **same** step-level `UpProgressHandler` with `allow_ending == true`, so both take the rendering branch.

**Root cause:** `error_with_message` (`up/utils/up_progress_handler.rs:252-261`) has no already-ended guard and is not idempotent.

**Secondary cause:** nested `UpError` `Display` double-wraps. `UpError::Exec` formats as `"execution error: {0}"` (`up/error.rs:8-9`), so wrapping an `Exec` in an `Exec` yields `execution error: ... execution error: ...`.

This shape appears at **13 sites**: `mise.rs:1460,1594,1705,1716,2073`; `nodejs.rs:373,409`; `base.rs:155,160,169`; `nix.rs:543`; `custom.rs:95`.

Also note `mise.rs:1425,1435` call the message-less `progress_handler.error()`, which in `PrintProgressHandler::error()` (`up/utils/print_progress_handler.rs:72-74`) reprints the *last progress message* - another duplicate-shaped emission.

**This is not only a terminal artifact.** `error_with_message` also writes `SyncUpdateProgressAction::Error` to the lock file (`up_progress_handler.rs:255`), so an attached second `omni up` replays **both** records via `SyncUpdateListener::handle_line` (`:451-471`). The duplication is in the wire protocol.

### Approach

1. **Idempotent guard** on `UpProgressHandler`: track whether the handler has already ended (success or error) and make subsequent `error`/`error_with_message` calls no-ops. Mirrors the existing `allow_ending` mechanism at `:254`.
2. **Adopt the `StepFailed` pattern** at the 13 sites. `UpError::StepFailed(String, Option<(usize,usize)>)` (`up/error.rs:16-20`) already exists precisely to mean *"already rendered, carry only identity upward"* - and is used at exactly one site today, `custom.rs:97`:

   ```rust
   progress_handler.error_with_message(format!("{err}").light_red());
   return Err(UpError::StepFailed(name, progress_handler.step()));
   ```

   Convention to apply: **the innermost layer that has a useful message renders; outer layers convert to `StepFailed` and do not render.**
3. **Stop double-wrapping** in `UpError` `Display`, or stop re-wrapping `Exec` in `Exec` at the propagation sites.

### Verification

- [ ] A failing node step renders exactly **one** `✖` line
- [ ] The lock file contains exactly one `Error` record per failure
- [ ] An attached second `omni up` shows one error, not two
- [ ] Nested errors read as one clause, not `execution error: ... execution error: ...`
- [ ] The top-level `omni: ...` line remains (it is the exit summary, and is legitimate)

## Problem 2: log files leak permanently, and cannot be found

The `omni-exec.<ts>.<rand>` path in the error comes from `up/utils/progress_handler.rs:217-231` (created via `NamedTempFile::with_prefix`) and `:332-336` (kept via `log_file.keep()` on non-zero exit). It lands in `std::env::temp_dir()`.

`tmpdir_cleanup()` (`src/internal/env.rs:113-124`) only globs `TMPDIR_CLEANUP_PREFIX` = `omnitmp-<8-hex>` (`env.rs:74-83`). `omni-exec.*` and `omni-update.*` **do not match**, and `NamedTempFile::keep()` removes them from tempfile's Drop-based cleanup. So **every failed step leaks a permanent file**, forever. The shell templates' background `find "${tmpdir}/" -name "${OMNI_FILE_PREFIX}*"` only matches `omni_<uuid>*` and misses these too.

There is an in-source `TODO` acknowledging exactly this, `progress_handler.rs:330-331`:

```rust
// TODO: the log file should be prefixed by the tmpdir_cleanup_prefix
//       by default and renamed when deciding to keep it
```

A second generator exists for updates: `git/updater.rs:157-165` creates `omni-update.<ts>.`, `:208-212` keeps it, and the path is stored in the DB and surfaced on the next prompt by `report_update_error` (`:239-243`).

**And the pointer is destroyed as it is printed.** `report_update_error` clears `omnipath.update_error_log` inside the same transaction (`cache/omnipath.rs:74-88`), so a background-update error is shown **exactly once, ever**. In fish it is worse: the template evals hook output with `eval "$line" 2>/dev/null`, swallowing the message entirely.

There is **no `omni logs` command** (`commands/builtin/mod.rs` inventory).

### Approach

1. **Move logs to `${state_home}/logs/`** - `state_home()` already exists (`env.rs:377`). Stable, discoverable, not subject to `$TMPDIR` reaping.
2. **Implement the existing TODO**: prefix with `tmpdir_cleanup_prefix()` while running, rename on `keep()`. Aborted runs then self-clean.
3. **Add `omni logs`**:
   - `omni logs` - list recent, newest first
   - `omni logs --last` - print/tail the newest
   - `omni logs <lifecycle>` - the newest for a given lifecycle (pairs with the failure message in [`03-up-lifecycles.md`](03-up-lifecycles.md))
   - `omni logs --clean` - prune
4. **Retention** - keep last N or N days, pruned on successful `up`. There is precedent: the cache tables already have a retention concept (`cache/up_environments.rs:87-105`).
5. **Stop destroying the update-error pointer on read.** Keep an append-only list so `omni logs` can show past failures, or at minimum do not clear until the user has plausibly seen it.
6. **Stop swallowing hook stderr in the fish template** (`templates/shell_integration.fish.tmpl`) - that is where these messages appear.

### Verification

- [ ] A failed step writes to `${state_home}/logs/`, not `$TMPDIR`
- [ ] An interrupted run (Ctrl-C) leaves no orphan log
- [ ] `omni logs --last` shows the log the failure message pointed to
- [ ] Retention prunes old logs and is bounded
- [ ] A background-update error is still visible on the **second** prompt after it occurs
- [ ] fish shows the error rather than swallowing it
