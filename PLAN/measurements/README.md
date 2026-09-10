# Measurements

Raw before/after data, kept so size and latency deltas are attributable to a specific change
rather than to "everything we did in Phase A".

## Conventions

- One file per measured variant, named `<phase>-<change>.txt` (e.g. `a1-profile-dist-bloat.txt`).
- Record the commit SHA and the exact command at the top of each file.
- `cargo bloat --release --crates -n 40` for size attribution.
- Prompt latency: many iterations, page cache warmed, `OMNI_SHELL_PPID` set.

## Baseline (commit efe6129, 2026-09-09, ARM aarch64 Linux dev container)

| Metric | Value |
|---|---|
| Binary size | 28,912,144 bytes (28 MB) |
| Linking | fully static (`ldd`: not a dynamic executable) |
| OpenSSL present | yes, `OpenSSL 3.5.4 30 Sep 2025` |
| Dependency count | 382 packages |
| `omni hook uuid` x200 | 0.28 ms/invocation |
| `omni hook env bash` x50 in /app | 13.6 ms/invocation |
| `omni help` x10 | 15.7 ms/invocation |
| `omni --complete ''` x20 | 12.3 ms/invocation |

Note: the baseline binary above is the installed `/usr/local/bin/omni` (version 2025.11.0),
which predates the source tree - its cache DB reports `user_version = 4` while
`cache/database/upgrade.rs` migrates to 5. Re-measure from a local
`cargo build --release` of `efe6129` before trusting size deltas.
