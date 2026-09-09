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
| Lighter binary | 28 MB | ~12-15 MB |
| Faster shell prompt | 13.6 ms/prompt | ~1-2 ms |
| Independent tool lifecycles | one failure aborts everything | a failure affects only its own lifecycle |
| Network resilience | no retries, no timeouts | retries with backoff, rate-limit aware |
| No dynamic linking | convention only | enforced by CI gate |

## Status board

| Phase | File | Status |
|---|---|---|
| A. Measurement harness & binary size | [`01-binary-size.md`](01-binary-size.md), [`02-static-linking.md`](02-static-linking.md), [`08-ci.md`](08-ci.md) | not started |
| B. Parallel up lifecycles | [`03-up-lifecycles.md`](03-up-lifecycles.md), [`04-error-reporting.md`](04-error-reporting.md) | not started |
| C. Network resilience | [`05-network-resilience.md`](05-network-resilience.md) | not started |
| D. Prompt latency | [`06-runtime-speed.md`](06-runtime-speed.md) | not started |
| E. UX polish | [`07-ux-polish.md`](07-ux-polish.md) | not started |

Status values: `not started` / `in progress` / `blocked` / `done`.

**Phase A goes first** because it builds the measurement harness (size gate, prompt-latency gate, static-linking gate) that tells us whether B-E actually helped.

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
| D8 | openssl removal is **gated on an experiment**, not assumed | The original "it is unused" claim was wrong. It was added for **libgit2's HTTPS**, and was orphaned only recently at `bc4273a`. See [`09-rejected.md`](09-rejected.md#the-openssl-mistake-and-what-is-actually-true) |
| D9 | Wholesale async migration **rejected** | The hot path is CPU-bound, not I/O-concurrency-bound; async would add `Runtime::new()` to every invocation and grow the binary. Instead consolidate **7** ad-hoc multi-thread runtimes into one current-thread runtime. See [`09-rejected.md`](09-rejected.md#rejected-wholesale-async-migration) |

## Open questions

None blocking. Questions that surface during implementation should be added here rather than resolved silently.

## Ground rules

- **Measure before and after.** Every size or latency claim needs a number from this machine, not an estimate.
- **One dependency change at a time**, each measured independently. Bundled changes hide regressions.
- **Do not remove a dependency because our source does not reference it.** Check `Cargo.lock` reverse dependencies and the built binary. This mistake was made once already.
- **The binary must stay self-sufficient.** No new dynamic dependencies on any platform. See [`02-static-linking.md`](02-static-linking.md).
