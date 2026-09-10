# Rejected ideas, and one corrected mistake

Recorded so nobody re-litigates these from the same bad premises.

---

## The openssl mistake, and what is actually true

### What was claimed, and why it was wrong

The first pass of this analysis asserted:

> "**Delete `openssl`** - 0 usages, verified. Nothing needs it. Pure win."

**That was wrong, and the reasoning was lazy.** The method was: `grep -rn "openssl::" src/` returns 0, therefore the dependency is unused. That conclusion does not follow. A `-sys` style dependency can be load-bearing without our source ever naming it, because it satisfies a *transitive* link requirement.

The shipped binary disproved the claim immediately:

```
$ strings /usr/local/bin/omni | grep -i "OpenSSL 3"
OpenSSL 3.5.4 30 Sep 2025
$ nm -a /usr/local/bin/omni | grep -c "SSL_CTX_new\|OPENSSL_init"
7
```

OpenSSL is compiled into the binary. **Lesson, now a ground rule in the README: never remove a dependency because our source does not reference it. Check `Cargo.lock` reverse dependencies and the built artifact.**

### What openssl was actually added for

It was **not** added for reqwest. The archaeology (all commands re-runnable):

**At `b8c5bee5` ("Moving to using rust for a faster omni"), where `openssl` was introduced:**

```
$ git show b8c5bee5:Cargo.toml | grep -n "openssl\|reqwest\|git2"
16:git2 = "0.17.2"
23:openssl = { version = "0.10", features = ["vendored"] }

$ git show b8c5bee5:Cargo.lock | grep -c 'name = "native-tls"'
0
```

`reqwest` **did not exist in the tree yet**, and `native-tls` was not in the lock. `git2 = "0.17.2"` carries default features, whose `https` feature makes `libgit2-sys` depend on `openssl-sys` on Linux. So `openssl = { vendored }` was the standard trick to force **libgit2's HTTPS transport** to link a vendored, static OpenSSL.

**`reqwest` arrived a year later**, at `2df18454` (2023-06-30), as `reqwest = { version = "0.11.18", features = ["blocking"] }`. At that commit both backends were present:

```
$ git show 2df18454:Cargo.lock | grep -c 'name = "native-tls"'   # 1
$ git show 2df18454:Cargo.lock | grep -c 'name = "openssl-sys"'  # 1
$ git show 2df18454:Cargo.lock | grep -A 12 'name = "libgit2-sys"'
  ... "libssh2-sys", "libz-sys", "openssl-sys", ...
```

So from mid-2023 openssl served **two** consumers: libgit2 (HTTPS + SSH) and reqwest (native-tls).

### When it became orphaned

Both consumer edges have since disappeared.

**libgit2 dropped it at `bc4273a` ("chore(deps): bump git2 from 0.20.4 to 0.21.0"), 2026-08-26:**

```
$ git show a7083e7:Cargo.lock | grep -A 12 'name = "libgit2-sys"' | grep -c openssl-sys   # 1
$ git show bc4273a:Cargo.lock | grep -A 12 'name = "libgit2-sys"' | grep -c openssl-sys   # 0
```

Current `libgit2-sys 0.18.8+1.9.7` deps are `[cc, libc, libz-sys, pkg-config]`.

**reqwest dropped it** at some point between 2025-12-29 (`04030d3`, `native-tls` still present) and HEAD, as it moved to 0.13.x. Current `reqwest 0.13.4` resolves to `hyper-rustls`, `rustls`, `tokio-rustls`, `rustls-platform-verifier`, and `native-tls` is **absent from `Cargo.lock` entirely**.

> **Residual uncertainty, stated plainly:** the exact commit where `native-tls` left the lock is not pinned. The earlier claim "vestigial since reqwest 0.13" was an **unverified guess** - 0.13 was picked only because it is the current version. It does not change the conclusion, because what matters is the present state, which is verified. Do not repeat the number as fact.

**Current state of the dependency graph:**

```
omnicli → openssl → openssl-sys → openssl-src     ← omnicli is the ONLY consumer
libgit2-sys → [cc, libc, libz-sys, pkg-config]    ← no openssl
reqwest 0.13.4 → hyper-rustls, rustls, tokio-rustls
native-tls → not in Cargo.lock
```

### Current hypothesis (NOT verified)

**openssl is now orphaned:** its last consumer edge vanished at `bc4273a`, roughly two weeks before HEAD, which plausibly explains why nobody has removed it.

Three things remain unverified, and they are precisely what the experiment in [`01-binary-size.md`](01-binary-size.md) exists to settle:

1. Whether HTTPS still works with openssl removed (rustls should carry it, but this is untested here)
2. Whether removal introduces any dynamic linking on any target
3. The size delta

**One important nuance.** The 7 symbols and the version banner prove openssl is **linked**, not that it is **reachable**. Without LTO, cross-crate dead-code elimination is weak: the `openssl` crate's own Rust code is compiled and its initialisers referenced, dragging the archive in. **Adding `lto = "fat"` may strip most of it by itself**, which is why the experiment measures `baseline` / `+lto` / `+openssl-removed` as three separate variants. The LTO win may partly subsume the removal win, and we should know which lever did what.

**`libz-sys` stays regardless.** `Cargo.toml:53` exists to force `features = ["static"]`, and `libgit2-sys` genuinely needs zlib. Of the two entries in the `cargo-machete` ignore list (`Cargo.toml:102-103`), this is the legitimate one.

### Status

Undecided. Gated on the Phase A experiment. **If HTTPS breaks or a dynamic dependency appears, openssl stays and this section gets updated with that result.**

---

## Rejected: `panic = "abort"`

**Idea.** Set `panic = "abort"` in the release/dist profile to drop landing pads and unwind tables across 382 crates.

**Why rejected.** `src/internal/env.rs:1154` uses `catch_unwind` around `gethostname`:

```rust
Err(_) => match catch_unwind(gethostname) {
```

Per the maintainer, this guard is **load-bearing**: the `gethostname` crate offers no fallible API and panics outright on platforms where the call is unsupported, so `catch_unwind` is the only way to avoid crashing. Under `panic = "abort"` the guard becomes inert and those platforms would hard-abort.

This is the **only** `catch_unwind` in non-test code (`grep -rn "catch_unwind|panic::set_hook" src/`), so the change would be tempting. It is still wrong.

**Revisit only if** `gethostname` gains a `try_gethostname`-style API, or the call is replaced with a direct `nix`/libc invocation that returns `Result`. `nix` is already a direct dependency (`Cargo.toml:56`).

---

## Rejected: removing `git2` / vendored libgit2

**Idea.** `git2` is used in only 9 production places across 4 files, for 5 read-only operations, while the codebase already shells out to `git` in 16 places. Dropping it would remove a large C library build and possibly the direct `libz-sys` need.

Production usage:

| Operation | Site |
|---|---|
| `Repository::discover` | `src/internal/git/utils.rs:251` |
| `status.contains(Status::IGNORED)` | `src/internal/git/utils.rs:272-275` |
| `find_branch(name, BranchType::Local)` | `src/internal/env.rs:822` |
| `head().peel_to_commit()` | `src/internal/env.rs:781-785` |
| `Repository` + `Oid::from_str` + commit lookup | `src/internal/commands/builtin/cd.rs:383-392` |

**Why rejected.** `GitRepoEnv::new` (`env.rs:765-846`) is **on the shell-prompt hot path**. Replacing in-process libgit2 calls with `git rev-parse` / `git check-ignore` subprocesses adds 1-3 `fork+exec` per prompt at roughly 2-5 ms each - against a **measured 13.6 ms** prompt we are trying to reduce to 1-2 ms. That trades a few MB of binary for a regression in the single most visible interaction in the tool.

Secondary reason: `discover` and `check-ignore` semantics are genuinely fiddly (worktrees, submodules, `.git` files, ignore precedence). Not worth reimplementing for a size win that LTO may largely deliver anyway.

**Do instead** (tracked in [`06-runtime-speed.md`](06-runtime-speed.md)): make the existing git2 usage cheaper by dropping the eager `peel_to_commit()` at `env.rs:781-785` behind a `OnceCell`, matching how `id` is already lazy at `:764`. The prompt path never reads `commit`.

**Revisit only if** the size measurements after Phase A show libgit2 is a dominant contributor *and* the prompt path has been restructured so it no longer needs git at all.

---

## Rejected: parallel step execution in `up`

**Idea.** Run independent `up` steps concurrently to cut wall-clock time.

**Why rejected for now.** Phase B deliberately separates two things that sound alike: **independent failure/commit domains** (what we are building) and **concurrent execution** (what we are not). The first solves the reported problem; the second adds risk without being asked for.

Blockers documented in [`00-findings.md`](00-findings.md#parallelism-in-up): process-global `set_current_dir` per step, an exclusively-borrowed `&mut UpEnvironment`, genuine sequential env dependency between steps, a fresh tokio `Runtime` per command, and `[n/m]` progress rendering that assumes linear advance.

**Revisit after** Phase B lands. The lifecycle graph it produces is exactly the input a scheduler would need, and `SpinnerProgressHandler::new_with_multi` (`up/utils/spinner_progress_handler.rs:30-36`) is already written and unused, ready for concurrent multi-line progress.

---

## Rejected: env provenance schema migration

**Idea.** Add a provenance column so each `paths` / `env_vars` entry records which lifecycle produced it, enabling exact per-lifecycle carry-forward.

**Why rejected.** Not necessary. `UpVersion` already has its own `env_vars` field (`src/internal/cache/up_environments.rs:588`), so carrying a version carries its env vars, and `versions` are already keyed by `(backend, normalized_name, dir)` - the same key `add_version` dedupes on (`:507-510`). Top-level `env_vars` come from the workdir-global `env:` config section and are recomputed wholesale every run, so they never need carrying.

That leaves only `paths`, which can be attributed by prefix from the carried version's `bin_path` / `data_path`. Full design in [`03-up-lifecycles.md`](03-up-lifecycles.md).

**Revisit if** the cheap approach produces wrong PATH ordering in practice, or if a future step type contributes env state that is not reachable from a `UpVersion`.

---

## Rejected: wholesale async migration

**Idea.** Move most of the codebase to async Tokio, on the theory that it would help performance or make the code cleaner.

**Why rejected.** The measurements point the other way on both counts.

### It would not help performance

The async surface today is **13 `async fn` and 42 `.await`** across ~54k lines of production code. All of it is subprocess stdout/stderr multiplexing (`up/utils/progress_handler.rs`) and askpass FIFO/socket listeners (`up/utils/askpass.rs`, `up/utils/listener_manager.rs`). That is genuinely the right shape for async, and it is already async.

Everything else is not I/O-concurrency-bound:

- **The prompt hot path** (13.6 ms, the biggest perf problem) is CPU and allocation: clap `Command` construction, YAML re-serialisation in `up_hash()`, a full `OmniConfig` deep clone, one local SQLite read, one libgit2 `discover`. See [`06-runtime-speed.md`](06-runtime-speed.md). Async adds poll/waker machinery to all of it and makes none of it faster.
- **`Runtime::new()` is itself a cost**, and async `main` would move it onto *every* invocation - including `hook env`, which `main.rs:194-228` currently fast-paths precisely to avoid heavy setup. `Runtime::new()` is `new_multi_thread`, so it spawns worker threads per construction.
- **`rusqlite` is synchronous** and there is no async SQLite in this stack. Going async means either blocking inside async (wrong) or `spawn_blocking` (thread-pool overhead) for every cache access.
- **The `up` step loop is deliberately sequential.** Steps depend on earlier steps' PATH (`tool.rs:276-280`), `set_current_dir` is process-global (`base.rs:103`), and `&mut UpEnvironment` is an exclusive borrow. Those are the blockers to concurrency - not sync-vs-async. Async would not unlock parallelism without solving them first, and Phase B deliberately does not (see above).
- **Binary size would grow**, against goal 1. Async state machines are large, and [`01-binary-size.md`](01-binary-size.md#5-narrow-tokio-features) is actively *narrowing* tokio from `"full"` to 7 features.
- **`requestty` (132 usages, the most-used non-std crate) is sync**, as is the whole interactive/terminal layer. Rewriting prompts async is large churn for no gain.

### It would not make the code cleaner

The current shape - sync business logic with async confined to the few places that genuinely multiplex I/O - is a reasonable separation. Spreading `.await` through 54k lines that have no concurrency requirement adds ceremony, not clarity, and colours every call site with an async requirement it does not need.

### But the instinct was right about something

There *is* real mess here, and the fix is the opposite direction: **less async surface, better managed.**

There are **7 separate `Runtime::new()` call sites**, each building a fresh **multi-threaded** runtime:

```
src/internal/config/up/utils/askpass.rs:78,154
src/internal/config/up/utils/progress_handler.rs:40,69,79
src/internal/git/utils.rs:51,74
```

`progress_handler.rs:40` sits inside `run_progress`, called **once per command execution** during `omni up` - so a build with many commands churns many multi-thread runtimes, spawning worker threads each time, for work that is a single `select!` over two pipes.

Replacing all seven with one lazily-built **current-thread** runtime is cleaner, faster, and smaller (it also permits dropping `rt-multi-thread` from the narrowed tokio feature list). Tracked as item 14 in [`06-runtime-speed.md`](06-runtime-speed.md), and it is a **prerequisite** for the tokio feature narrowing in [`01-binary-size.md`](01-binary-size.md#5-narrow-tokio-features).

**Revisit the broader question if** a future feature genuinely needs concurrent I/O across many endpoints - the most plausible candidate is fetching release metadata and assets for several lifecycles at once, once Phase B's lifecycle graph exists. Even then, prefer targeted concurrency at those call sites (`futures::join_all` over the existing 3 HTTP sites, or a small thread pool) over an async rewrite of the whole tool.
