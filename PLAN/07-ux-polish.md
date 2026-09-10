# Phase E: UX polish and stability

**Goal:** fix the sharp edges - output that cannot be piped, panics on hot paths, missing affordances.

**Status:** not started

Independent items. Can be picked off individually; ordered roughly by severity.

## P0: correctness and crashes

### 1. `omni help` writes to stderr

**The principle everyone agrees on:** when a user explicitly asks for help, help *is* the requested output, so it belongs on stdout. Error-triggered usage is different and correctly belongs on stderr. That is also the GNU convention (`--help` → stdout, usage-after-error → stderr).

The code does not currently do that. Measured byte counts per stream:

| Invocation | stdout | stderr | Correct? |
|---|---|---|---|
| `omni help` | 0 | 1278 | no - should be stdout |
| `omni --help` | 0 | 1278 | no - should be stdout |
| `omni help status` | 0 | 820 | no - should be stdout |
| `omni help config` | 0 | 641 | no - should be stdout |
| `omni help --output json` | **2849** | 0 | **yes, already stdout** |
| unknown command | 0 | 41 | yes, correctly stderr |

The inconsistency is the clearest signal: **the same command with `--output json` goes to stdout, and without it goes to stderr.** Cause is `eprintln!` throughout `print_global_help` / `print_command_help` (`commands/builtin/help.rs:266-391`), while the JSON printer (`:708-790`) uses `println!`.

Consequence: `omni help | less`, `omni help | grep`, and `omni help > file` all silently produce nothing, while the JSON variant works fine.

Fix: human-readable help joins the JSON path on stdout. Error-triggered usage stays on stderr.

**Caution:** check whether anything (shell templates, `hook init`, tests) relies on help going to stderr. `tests/test_omni_help.bats` will need review - if it asserts on `stderr`, those assertions move to `stdout`.

### 2. Panic: `MakefileCommand::all_from_path` walks to `/`

`commands/frommakefile.rs:44-46`:

```rust
while let Some(parent) = path.parent() {
    for entry in fs::read_dir(path).unwrap() {   // ← PANICS on EACCES
        let entry = entry.unwrap();              // ← PANICS
```

Outside a workdir the loop walks all the way to `/`, reading every intermediate directory. Any unreadable directory hard-panics `omni <anything>`. Replace with graceful skips.

### 3. Panic: `COMP_CWORD` underflow

`main.rs:183-185` does `.parse().unwrap_or(0) - 1`. With `COMP_CWORD=0` this is `0usize - 1` - panic in debug, wraparound in release. Guard with `saturating_sub`.

### 4. Panic: cache layer on the prompt path

`CacheManager::get()` uses `.expect("Failed to create cache manager")` (`cache/database/manager.rs:19`), and `pool.rs:78-90` uses `.expect()` for `create_dir_all`, pool build, connection, and migration.

**A corrupt or unwritable cache DB therefore panics on every shell prompt** - a spectacularly bad failure mode for something in `PROMPT_COMMAND`. Degrade gracefully: a broken cache should mean "no dynamic env", not a crash in the user's shell.

Related: there is no downgrade guard. The live DB here reports `user_version = 4` while `cache/database/upgrade.rs:44-46` migrates to 5, i.e. the installed binary predates the source tree. An older omni opening a newer DB neither migrates nor detects the mismatch.

### 5. Unknown flags report as unknown commands

`omni --bogus-flag` prints `omni: command not found: --bogus-flag`, because the positional uses `allow_hyphen_values(true)` (`main.rs:82`) and swallows it; the message is emitted at `main.rs:293-298`. Distinguish the two cases.

Also note top-level errors are printed two inconsistent ways: the `omni_error!` macro (`user_interface/print.rs:92-109`, used at `main.rs:119`) versus hand-assembled `eprintln!` with inline colors (`main.rs:221-227,293-298`). Unify.

## P1: missing affordances

### 6. `omni completion {bash,zsh,fish}`

There is no completion command and no `clap_complete` dependency. Completion arrives **only** as a side effect of `omni hook init` (`templates/shell_integration.*.tmpl`), which registers `complete -F _omni_complete_bash omni` and friends. Users who do not want the prompt hook get no completions.

The bash implementation also `echo`s an error and skips completion entirely on bash < 4 - and bash 3.2 is still the system bash on macOS.

Add a standalone command emitting a static script.

### 7. Finish issue #599 - global flags in help

`GLOBAL_OPTIONS` (`help.rs:30-51`) is a hand-maintained list of **3** flags: `--update`, `--self-update`, `--exists`. `MainArgs::parse` (`main.rs:39-83`) parses about **9**. Missing: `-h/--help`, `--version`, `-e`, `-l/--local`, `-A/--askpass`, `--update-and-log-on-error`.

Generate from one source rather than duplicating, so it cannot drift again.

### 8. Finish issue #413 - mention `--unfold`

Global help prints `config ▶`, `hook ▶`, `test ▶` fold markers with no hint that `--unfold` exists. `cmd.num_folded()` is already computed at `help.rs:574`; add a one-line footer when it is non-zero.

### 9. Non-interactive fuzzy suggestions

The "did you mean?" machinery (`commands/loader.rs:349-500`) is entirely gated on `shell_is_interactive()`. In a pipe or script the user gets no suggestion at all. Print the top candidate as a plain hint even when non-interactive.

### 10. `--dry-run`

None exists - `grep -rn "dry.run|dry_run" src/` finds one comment at `git/updater.rs:390`.

`omni up` installs toolchains, writes shims, mutates PATH, and clones repos; `omni tidy` moves git repos around; `omni clone` writes to disk. For a tool whose flagship command reorganises the filesystem and installs software, this is the biggest missing safety affordance.

`omni up` already has `--no-cache`, `--bootstrap`, `--clone-suggested`, `--trust`, `--update-repository`, `--update-user-config`, `--upgrade` (`commands/builtin/up.rs:1194-1323`), so the flag plumbing exists.

**Interacts with [`03-up-lifecycles.md`](03-up-lifecycles.md):** a dry run becomes much more useful once the lifecycle graph exists, because it can print the resolved graph and what each lifecycle would do. Consider sequencing after Phase B.

### 11. Colors: respect `TERM=dumb` and `CLICOLOR=0`

`enable_colors()` (`user_interface/colors.rs:72-84`) honours `NO_COLOR` (`:73`) and `CLICOLOR_FORCE` (`:77`), but:

- **`TERM=dumb` is not checked** anywhere. Emacs `shell-mode`, `M-x compile`, and many CI runners set it.
- **`CLICOLOR=0` is not honoured** (only `CLICOLOR_FORCE`).
- The condition is `stdout().is_terminal() || stderr().is_terminal()` (`:83`, with an acknowledged `TODO` at `:81-82`), so colors turn **on** when stdout is a pipe as long as stderr is a TTY - meaning `omni cmd | tool` gets ANSI codes in the piped stream.
- The `force_*` API (`:101-147`) **bypasses the check entirely** and is used throughout `dynenv.rs:152-155,250,253`, so dynamic-env prompt messages ignore `NO_COLOR`.

### 12. Logging and verbosity

**There is no logging framework at all.** `grep -rn "RUST_LOG|env_logger|tracing::|log::" src/` returns **zero**. No `log`/`tracing`/`env_logger` in `Cargo.toml`.

Consequences:

- **`CLAUDE.md` is wrong**: it documents `Run with debug: RUST_LOG=debug cargo run -- <command>`. `RUST_LOG` is read nowhere. Fix the doc regardless of whether logging is added.
- No global `--verbose`, `--quiet`, or `--log-level`. The only `--quiet` is on `hook env` (`hook/env.rs:90`).
- A stray `dbg!` sits in production at `self_updater.rs:167`, and does not format its arguments correctly (`dbg!` takes no format args).

Adding a logging façade is a larger change; **fixing `CLAUDE.md` and removing the `dbg!` are immediate.**

### 13. Machine-readable output

`--output json` exists in exactly two places: `omni help` (`help.rs:202-207`) and `omni config check` (`config/check.rs:279-281`). Nothing for `status`, `up`, `tidy`, `cd`, `clone`. No global `--output`.

Lower priority, but worth considering for `omni status` and `omni up` (the lifecycle summary from Phase B is naturally structured).

## P2: smaller items

- **Do not `exit(1)` on narrow terminals.** `print_categorized_command_help` exits (`help.rs:518-521`) and `print_syntax_column_help` errors (`:410-412`) when the terminal is under ~30 columns. Degrade instead.
- **Deduplicate config file discovery.** The logic is duplicated verbatim between `config/loader.rs:61-100` and `config/feuilletage_loader.rs:33-81`, including `WORKDIR_CONFIG_FILES` (`loader.rs:12` and `feuilletage_loader.rs:23`). Two sources of truth for which files omni reads.
- **Replace the per-`omni`-call background `find $TMPDIR`** in the shell templates with a targeted `rm` of known filenames. Every `omni` invocation currently spawns a full `find` over `$TMPDIR`.
- **fish swallows hook errors** with `eval "$line" 2>/dev/null` (`templates/shell_integration.fish.tmpl`) - see [`04-error-reporting.md`](04-error-reporting.md).

## Verification

- [ ] `omni help | grep <something>` works; `omni help > f` is non-empty
- [ ] `test_omni_help.bats` updated for the stdout move
- [ ] `omni` from an unreadable directory does not panic
- [ ] `COMP_CWORD=0 omni --complete ''` does not panic
- [ ] A deliberately corrupted cache DB does not panic the shell prompt
- [ ] `omni --bogus-flag` reports an unknown *flag*
- [ ] `omni completion bash` emits a usable script, independent of `hook init`
- [ ] Global help lists every flag `MainArgs::parse` accepts
- [ ] `--unfold` is discoverable from global help
- [ ] `NO_COLOR=1 omni ...` produces no ANSI codes **including** dynamic-env messages
- [ ] `TERM=dumb omni ...` produces no ANSI codes
- [ ] `omni cmd | cat` produces no ANSI codes
- [ ] `CLAUDE.md` no longer claims `RUST_LOG` works
