# Phase B: parallel lifecycles in `omni up`

**Goal:** a failure in one tool's lifecycle affects only that lifecycle and its dependents. Everything independent still installs, commits, and works.

**Status:** not started

Ships as **one coherent change** (decision **D4**). Parser, composition, and reporting are entangled; partial states would be more confusing than the current behaviour.

> "Parallel" here means **independent failure and commit domains**, not concurrent execution. Steps still run sequentially. Concurrency is explicitly out of scope - see [`09-rejected.md`](09-rejected.md#rejected-parallel-step-execution-in-up).

## The problem

The reported symptom, from `.omni.yaml:1-6` (rust, node, bats, github-releases):

```
[1/6] ✔ rust (latest): rust 1.98.1 installed
[2/6] ✖ node (auto): failed to install packages: ...
omni: up command failed: issue while setting repo up: ...
```

`bats` and `sqruff` never ran, **and rust ended up unusable**. Two defects, fully evidenced in [`00-findings.md`](00-findings.md#issue-1-omni-up-aborts-everything-when-one-tool-fails):

- **Defect A** - fail-fast loop: the `?` at `src/internal/config/up/base.rs:116` abandons all remaining steps.
- **Defect B** - all-or-nothing commit: `assign_environment()` is at `base.rs:120`, *after* the loop, so rust's success is never committed. It is also never given a `required_by` row, so it becomes garbage-collectable after `cleanup_after` (default 1 week).

Defect B is the one that actually hurts. Fixing it alone would have left working rust.

## Design

### Lifecycles

A **lifecycle** is a maximal set of steps connected by dependency edges. Each lifecycle:

- fails fast **internally** (decision **D1** - current behaviour preserved)
- commits **independently** of every other lifecycle
- on failure, propagates *skipped* to its transitive dependents only

Default is one lifecycle per top-level step, then merged by inferred edges.

### Dependency inference, conservative by default

Independence cannot be proven in general, because `custom` / `bash` steps run arbitrary shell (decision **D2**). Rules:

| Step kind | Treatment |
|---|---|
| Pure tool steps (`mise` fallback, `github-release`, `nodejs`, `python`, `go`, ...) with no declared deps | independent lifecycle each |
| `cargo-install` | edge → rust |
| `go-install` | edge → go |
| `bundler` | edge → ruby |
| Steps sharing a `dir:` subtree | same lifecycle |
| `custom` / `bash` | **barrier** - depends on everything declared before it |
| `and` / `or` / `any` composites | one lifecycle (already atomic internally, `tool.rs:284-328`) |

Escape hatches: `depends_on: [<name>]` to add an edge, `lifecycle: <name>` to force grouping.

For the reported `.omni.yaml` all four steps are pure tool steps with no `custom`, so all four become independent and the desired outcome falls out of the default rules with **no configuration required**.

### Parser constraint

The step normalizer at `base.rs:380-435` currently assumes **a single-key object IS the tool tag**:

```rust
feuilletage::ContextValue::Object(values, _) if values.len() == 1 => ...
```

It wraps each step as `{"tool": <normalized>}` or `{"empty_operation": ...}`. Adding sibling keys like `depends_on:` or `lifecycle:` requires teaching it to **partition reserved keys from the tool tag** before wrapping. Reserved-key list must be explicit and documented, since a future tool named `lifecycle` would collide.

Extend `UpConfigStepWire` (`base.rs:356`) with the new optional fields, and thread them through `FromParsed` (`base.rs:440-485`).

### Env composition: the cheap keyed carry

Decision **D3** - no schema migration. Rationale and the rejected alternative are in [`09-rejected.md`](09-rejected.md#rejected-env-provenance-schema-migration).

Composition on partial failure:

```
committed_env = for each lifecycle L still declared in config:
    L succeeded       → new slice from this run
    L failed/skipped  → previously-committed slice for L
```

Field by field (`UpEnvironment`, `cache/up_environments.rs:339-355`):

| Field | Rule | Exactness |
|---|---|---|
| `versions` | carry by `(backend, normalized_name, dir)` - the same key `add_version` already dedupes on (`:507-510`) | **exact** |
| `UpVersion.env_vars` (`:588`) | rides along with its carried version | **exact** |
| `paths` | dedupe by value; a carried lifecycle's paths are identified by prefix from its `bin_path` / `data_path` | good |
| top-level `env_vars` | **recomputed wholesale** - these come from the workdir-global `env:` config section, not per-lifecycle | exact by construction |
| `config_modtimes`, `config_hash` | recomputed wholesale (already snapshotted once at `up.rs:1590`) | exact |

The unlock is `UpVersion.env_vars`: tool-specific env vars are already attached to the version record, which is keyable. So carrying a version carries its environment.

**Why top-level `env_vars` must not be merged per-name:** it is an **ordered operation log**, not a map. `add_env_var_operation` pushes (`:470`) with operations `Set | Prepend | Append | Remove | Prefix | Suffix` (`config/parser/env.rs:538-578`). A per-name "new wins" merge would be wrong for the accumulative ops. Recomputing the whole list from config sidesteps this entirely.

### Two accepted caveats

1. **PATH ordering may differ** from a fully-successful run (new-then-carried, versus config order). Mitigate by re-applying `add_path`'s existing rule that `data_home()`-prefixed paths get prepended (`:484-488`). Document rather than over-engineer.
2. **A lifecycle removed from config is not carried.** This is deliberate - it prevents stale paths accumulating forever.

### Critical: re-assert `required_by` for carried versions

`commit()` (`base.rs:185-205` → `tool.rs:372-376` → `mise.rs:1384-1403`) is the **only** writer of `mise_installed_required_by`. If a carried-over version does not get a `required_by` row for the new `env_version_id`, it matches the GC predicate in `cache/database/sql/mise_operation_list_removable.sql` and gets **uninstalled** after `cleanup_after`.

So composition must also re-assert `required_by` for every carried version, not just newly-installed ones. **Without this, the feature silently breaks the very thing it exists to preserve** - node would keep working for a week and then vanish.

Note also `commit()` is gated on `was_upped()` (`tool.rs:372-376`, `mise.rs:1405-1407`), which is false for a failed lifecycle. Carried versions therefore need a path that does not depend on `was_upped()`.

### Hard constraint: `UpConfigMise` is single-shot

`mise.rs:1361-1363` returns `"up operation already attempted"` if `up()` is called twice on the same instance. Any retry or re-run within a process must clone or rebuild the config.

### Reporting

Replace the single abort line with an end-of-run summary:

```
✔ rust     1.98.1 installed
✖ node     npm install exited 1 - kept previous 20.11.0   (omni logs node)
✔ bats     1.11.0 installed
✔ sqruff   0.21.0 installed

omni: up finished with 1 failed lifecycle (exit 1)
      updated: rust, bats, sqruff  ·  unchanged: node
      retry with: omni up node
```

Exit code stays non-zero. Error-rendering dedup is a prerequisite - see [`04-error-reporting.md`](04-error-reporting.md).

`[n/m]` numbering needs revisiting: `num_steps = steps.len() + 2` (`base.rs:99`) assumes linear advance. Skipped lifecycles mean the denominator no longer matches what runs.

### `omni up <lifecycle>`

Retrying one lifecycle follows naturally once the graph exists, and makes the failure message actionable. Scope it carefully: it must still commit a *composed* env (new slice for the retried lifecycle, carried slices for the rest), not just that lifecycle's slice in isolation.

## Implementation order

1. Lifecycle graph construction + inference, with unit tests on the graph alone (no behaviour change yet)
2. `UpConfigStepWire` + normalizer changes for `depends_on:` / `lifecycle:`
3. Replace the `?` at `base.rs:116` with per-lifecycle fail-fast + cross-lifecycle continuation, accumulating `Vec<(lifecycle, UpError)>`
4. Env composition + `required_by` re-assertion
5. Summary reporting (needs [`04-error-reporting.md`](04-error-reporting.md) landed first or together)
6. `omni up <lifecycle>`
7. Docs under `website/contents/reference/configuration/parameters/up/`

## Verification

Extend `tests/test_omni_up_custom.bats`. Note two existing tests encode the **current** fail-fast behaviour and will need updating with intent:

- `:69-88` - "omni up custom only executes first command if failing" (intra-step; should still pass, since fail-fast within a lifecycle is preserved)
- `:691-706` - "omni up fails after running valid operations when another operation is invalid" (this is the config-error channel, `UpConfig::errors`, not the runtime channel - confirm which behaviour is being asserted)

New cases needed:

- [ ] Independent tool steps: one fails, others install **and are usable in the shell afterwards**
- [ ] Exit code is non-zero when any lifecycle fails
- [ ] Failed lifecycle keeps its **previously-committed** version (the core scenario)
- [ ] Carried version survives a `cleanup` run - i.e. `required_by` was re-asserted. **This is the test most likely to be forgotten and the most damaging to miss.**
- [ ] `custom` step acts as a barrier: steps after it are skipped when it fails
- [ ] `cargo-install` is skipped when rust fails (inferred edge)
- [ ] Explicit `depends_on:` propagates skip
- [ ] A lifecycle removed from config is not carried forward
- [ ] `omni up <lifecycle>` retries one and composes correctly
- [ ] First-ever run with a failure: no previous slice exists to carry, must not panic or write a partial version record
- [ ] `omni down` still tears down correctly after a partial commit (`base.rs:207-234` iterates in reverse with the same `?`)
