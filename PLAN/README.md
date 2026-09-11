# omni improvement plan

Working plan for making the omni binary **lighter**, **faster**, and **nicer to use**.

Created 2026-09-09. Baseline commit: `efe6129`.

## If you are a fresh session, read this first

1. Read [`00-findings.md`](00-findings.md) - the measured baseline and all evidence, with `file:line` citations.
2. Check the **status board** below to find the active phase.
3. Read that phase's file and continue from its `Status` section.

Every claim in these documents carries a `file:line` citation so you can **re-verify rather than trust**. If a citation no longer matches the code, the code moved: re-verify before acting. Several claims in the original analysis were wrong until checked empirically (see [`09-rejected.md`](09-rejected.md)), so prefer measuring over reasoning.

## Goals

| Goal | Baseline | Target |
|---|---|---|
| Lighter binary | 28 MB | ~12-15 MB · **host build now -37.3%: 25.09 → 15.72 MB** |
| Faster shell prompt | 13.6 ms/prompt | ~1-2 ms |
| Independent tool lifecycles | one failure aborts everything | a failure affects only its own lifecycle |
| Network resilience | no retries, no timeouts | retries with backoff, rate-limit aware |
| No dynamic linking | convention only | enforced by CI gate |

## Status board

| Phase | File | Status |
|---|---|---|
| A. Measurement harness & binary size | [`01-binary-size.md`](01-binary-size.md), [`02-static-linking.md`](02-static-linking.md), [`08-ci.md`](08-ci.md) | **done** - see below |
| B. Parallel up lifecycles | [`03-up-lifecycles.md`](03-up-lifecycles.md), [`04-error-reporting.md`](04-error-reporting.md) | not started |
| C. Network resilience | [`05-network-resilience.md`](05-network-resilience.md) | not started |
| D. Prompt latency | [`06-runtime-speed.md`](06-runtime-speed.md) | not started |
| E. UX polish | [`07-ux-polish.md`](07-ux-polish.md) | not started |

Status values: `not started` / `in progress` / `blocked` / `done`.

**Phase A goes first** because it builds the measurement harness (size gate, prompt-latency gate, static-linking gate) that tells us whether B-E actually helped.

### Phase A progress

Branch: `improve/phase-a-size-and-gates`

| Item | Status | Result |
|---|---|---|
| Self-sufficiency gate: dynamic deps ([`02`](02-static-linking.md)) | done | `check-static-linking.sh`, 6 cases verified, wired before packaging on PR + release paths |
| Self-sufficiency gate: **static** native libs ([`02`](02-static-linking.md)) | done | `check-linked-libraries.sh` + `.github/linked-libraries.allow`, 6 cases verified. Reads `cargo:rustc-link-lib=` directives, so it sees static linkage the binary cannot reveal |
| `[profile.dist]` ([`01`](01-binary-size.md) item 1) | done | **-36.3%**, 25.09 MB → 15.97 MB on a host build |
| CI uses `dist` for shipped artifacts ([`08`](08-ci.md)) | done | build step only; tests stay on `release`; timeout 30 → 45 min |
| Lockfile repair | done | pre-existing broken `--locked`, found incidentally |
| openssl removal ([`01`](01-binary-size.md) item 2) | done | removed; **0 B** size change, **-22s** build time. Settled by per-target `cargo tree`, no musl build needed |
| ~~OpenSSL-specific guard~~ | removed | wrong abstraction; superseded by the static-native-lib gate above |
| Dep narrowing: reqwest (item 3) | **not applied** | measured: saves 1 crate. Plan overestimated by reading `Cargo.lock` instead of the per-target graph |
| Dep narrowing: zip (item 4) | done | 14 crates + `zstd-sys` gone, -0.8% size, -27% build. All pure-Rust codecs retained; only zstd + AES dropped. Exposed latent dynamic-lzma bug |
| Dep narrowing: tokio + shared runtime (item 5) | done | 7 multi-threaded runtimes → 1 current-thread; `full` → 7 features; -65 KB |
| Dep narrowing: base62 (item 6) | done | 3 crates gone; 28 captured vectors prove byte-identical output |
| Dep narrowing: futures (item 7) | done | facade → `futures-util`; drops `futures-executor` + a proc-macro |
| Size CI report ([`08`](08-ci.md)) | done | `report-binary-size.sh`; a report, not a gate - baseline logic was tried and removed, see below |
| Prompt-latency CI gate ([`08`](08-ci.md)) | done, **report-only** | `bench-hook-env.sh`; needs a musl baseline before a threshold is meaningful |
| Repo hygiene (`config-value/` etc.) | done | removed by the maintainer |
| Shell pitfall audit | done | fixed 5 live sites; the bespoke linter written for it was removed as unwired dead code |
| Phase B: log lifecycle (`04` Problem 2, items 1-2) | done | kept logs renamed out of the cleanup prefix, staying in `$TMPDIR`; cleanup taught to remove files |
| Phase B: error dedup + message nesting (`04` Problem 1) | done | one failure renders once; `execution error:` no longer doubles |
| Merge main (zip 4 → 8) + drop liblzma | done | zip 8 uses pure-Rust lzma, so the C binding went entirely: native libs **6 → 5**. 4 new xz extraction tests |

### Unplanned findings from Phase A

1. **CI was broken on `main`.** `Cargo.lock` at `efe6129` referenced `syn 3.0.4` with no matching `[[package]]` entry, so `cargo metadata --locked` failed - and both CI build steps pass `--locked`. Fixed in `f9c8bcd`.
2. **The openssl question cannot be answered on a glibc host.** On gnu, `openssl-sys` finds the *system* library so `vendored` never engages, and the linker discards openssl entirely (0 symbols, no banner, no `libssl` in `ldd`) while the binary still works. The shipped musl binary *does* contain `OpenSSL 3.5.4`. The experiment has to run on musl. Details in [`measurements/a1-profile-dist.txt`](measurements/a1-profile-dist.txt).
3. **`grep -q` in a pipeline under `set -o pipefail` silently inverts.** Bit the first version of the linking gate. Noted in [`02-static-linking.md`](02-static-linking.md#bug-found-while-testing-the-script).
4. **`liblzma` was only statically linked by accident.** Its `static` feature is off by default; `zip`'s `lzma-static` was enabling it transitively. Trimming zip's features silently produced a binary linked against `liblzma.so.5`. Now declared explicitly. Caught by the new gate on its first real use.
5. **Size estimates from `Cargo.lock` are wrong.** `Cargo.lock` is the union across all targets and feature combinations; only `cargo tree --target` shows what compiles. `quinn`, `ring` and `wasm-bindgen` were never being built despite appearing in the lock.
6. **A glibc host benchmark does not represent shipped performance.** Both a pre-change and a post-change local build measure ~1.8 ms on `hook env`, while the shipped musl binary measures **7.51 ms**. Phase A changed prompt latency not at all. Leading hypothesis is musl's allocator against a prompt path that does 228 `OmniConfig` deep clones and re-serialises YAML per call - which means Phase D should pay off **more** on musl than a local benchmark suggests. The latency gate is therefore report-only until CI records a musl baseline. See [`measurements/a5-a7-and-gates.txt`](measurements/a5-a7-and-gates.txt).
7. **Knowing about a footgun in prose did not stop me repeating it - twice.** The `grep -q` + `pipefail` + `SIGPIPE` inversion was written up in [`02-static-linking.md`](02-static-linking.md), then reintroduced hours later in the benchmark script. A later audit found it had **never actually been fixed**: two live instances survived in `check-static-linking.sh` itself, including the darwin `LC_RPATH` check where `otool -l`'s large output makes the false pass most likely. The five live sites were fixed and each now carries a comment *at the call site*, which is the only form of the lesson that travels with the code.

A bespoke linter was written to enforce this, then removed: it was never wired into a workflow, and an unwired checker is worse than none - it looks like protection while providing zero. If this needs to be a control rather than a convention, the right move is `shellcheck` in CI (already used ad hoc via a `disable=` directive in `get-changed-files.sh`, but not run anywhere), not a one-pattern script. **Comments and docs are not controls - but neither is a checker nothing invokes.**
8. **"musl is slower" was measured across two different machines.** The 4.7x gap driving an allocator investigation compared local gnu (1.61 ms) with CI musl (7.51 ms). On one machine, paired and repeated, **musl is faster** (1.97-2.24 ms vs gnu 2.36-3.38 ms) and more consistent, because its static-linked startup floor is half gnu's and startup dominates a per-prompt process. Two hours of research into musl allocators, mimalloc and `-C target-cpu` rested on a comparison that measured runner speed. **A number from a different machine is not a baseline.**
9. **A size threshold could not be set to a useful value, so it was removed.** Any tolerance loose enough not to trip on ordinary feature work also lets growth ratchet through unnoticed - 5% per PR compounds to +28% over five PRs with CI green throughout, which is the drift the gate existed to stop. And the routine response to a tripped threshold is to bump the baseline, making it a rubber stamp with maintenance attached. The sharp edge - a new native C dependency - is caught precisely by `check-linked-libraries.sh`, which has a real yes/no answer. Size is now reported (and surfaced on the run summary) rather than enforced. **A check whose only realistic failure response is to raise the limit is not a check.**
10. **`$TMPDIR` is shared between users on Linux.** Kept logs are `0600`, so contents are safe, but anything that *lists* `$TMPDIR` by pattern enumerates other users' files. This is what ruled out a cheap `omni logs`; making it correct would need a per-user store, which reintroduces retention. Per-user `$TMPDIR` on macOS hides this, so it would not have shown up in local testing.
11. **`remove_dir_all` does not remove a file.** Its "all" means *the directory and its contents, recursively* - handed a plain path it returns `NotADirectory` and removes nothing (verified directly). `force_remove_dir_all` wraps it and is documented as *"remove the given directory"*, so this is correct behaviour, not a bug in that function. It became a bug only where a glob could match **both** kinds of entry, and `tmpdir_cleanup` discarded the error with `let _ =`. The other 6 callers were checked and all pass genuine directories. The fix went at that one call site rather than widening `force_remove_dir_all`, because 4 of its callers check the error and a file where a directory is expected is a signal worth keeping.
12. **Do not run `cargo fmt` on stable in this repo.** `.rustfmt.toml` sets `unstable_features`, `group_imports` and `imports_granularity`, which stable rustfmt ignores silently, so a stable `cargo fmt` reformatted 30 unrelated files. CI only runs fmt from the `auto-pr-cargo-maintenance` workflow, not as a per-PR gate, so the tree is not stable-fmt-clean. Caught only because a `| tail -3` had hidden the real `diff --stat` summary - **truncating verification output is how unrelated changes get committed.**
13. **`liblzma` was never meant to be a C dependency at all.** main's zip 8 bump switched zip to a pure-Rust lzma decoder, which removed the transitive feature that had been statically linking `liblzma-sys`, leaving main linking `liblzma.so.5` dynamically. Since a pure-Rust xz decoder was now already in the graph, the C binding was dropped outright: native libraries **6 → 5**.

## Decisions made

| # | Decision | Rationale |
|---|---|---|
| D1 | Fail-fast stays the rule **within** a lifecycle; continue **across** lifecycles by default | Preserves current behaviour for anything whose independence we cannot prove |
| D2 | Dependency inference is **conservative**: `custom`/`bash` steps are barriers | Arbitrary shell cannot be analysed, so assume dependence |
| D3 | Env composition uses the **cheap keyed-carry** approach, no schema migration | `UpVersion` already carries its own `env_vars`, so carrying a version carries its env. See [`03-up-lifecycles.md`](03-up-lifecycles.md) |
| D4 | Phase B ships as **one coherent change**, not staged | Parser + composition + reporting are entangled; partial states are confusing |
| D5 | `panic = "abort"` **rejected** | Breaks the load-bearing `catch_unwind(gethostname)` at `src/internal/env.rs:1154` |
| D6 | git2 removal **rejected** | Would add `fork+exec` to the prompt hot path, trading visible latency for a few MB |
| D7 | Expensive optimisation (`lto`, `codegen-units=1`) lives in a separate `[profile.dist]`, not `[profile.release]` | Keeps PR CI and local dev builds fast |
| D8 | openssl **removed** in `f7a3faf`, after an experiment rather than on assumption | Original "it is unused" claim was wrong in method. Settled by per-target `cargo tree` plus link evidence. Saves build time, **not** size. See [`09-rejected.md`](09-rejected.md#the-openssl-mistake-and-what-is-actually-true) |
| D9 | Wholesale async migration **rejected** | The hot path is CPU-bound, not I/O-concurrency-bound; async would add `Runtime::new()` to every invocation and grow the binary. Instead consolidate **7** ad-hoc multi-thread runtimes into one current-thread runtime. See [`09-rejected.md`](09-rejected.md#rejected-wholesale-async-migration) |

## Open questions

None blocking. Questions that surface during implementation should be added here rather than resolved silently.

## Ground rules

- **Measure before and after.** Every size or latency claim needs a number from this machine, not an estimate.
- **One dependency change at a time**, each measured independently. Bundled changes hide regressions.
- **Do not remove a dependency because our source does not reference it.** Check `Cargo.lock` reverse dependencies and the built binary. This mistake was made once already.
- **The binary must stay self-sufficient.** No new dynamic dependencies on any platform. See [`02-static-linking.md`](02-static-linking.md).
