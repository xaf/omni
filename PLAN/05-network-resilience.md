# Phase C: network resilience

**Goal:** transient network failures and rate limits retry instead of aborting `omni up`; a stalled connection cannot hang forever.

**Status:** not started

## Current state

The whole codebase has **3 HTTP call sites** plus the self-updater. Every other "network" operation (git, brew, mise, nix, go, cargo) is delegated to a subprocess.

| Site | Purpose | Client built at |
|---|---|---|
| `src/internal/config/up/github_release.rs:1178` | GitHub releases list, paginated | `:1132` via `get_github_client` (`:1093-1145`) |
| `src/internal/config/up/github_release.rs:1522` | asset download | `:1519` - **per asset** |
| `src/internal/config/up/cargo_install.rs:952` | crates.io versions | `:939`, hand-duplicated |
| `src/internal/self_updater.rs:155` | `raw.githubusercontent.com` | bare `reqwest::blocking::get`, no builder |

What is missing, all verified in [`00-findings.md`](00-findings.md#issue-3-no-network-resilience-anywhere):

- **No retry, no backoff.** Anywhere. The only `retry`-named code is FIFO/IPC (`up/utils/fifo_handler.rs:170,177,200`).
- **No timeouts.** No `.timeout()`, `.connect_timeout()`, or `.read_timeout()` on any client. reqwest blocking defaults to **infinite**, so a stalled socket wedges `omni up` indefinitely. `RunConfig` timeouts cover subprocesses only and never reach reqwest.
- **No rate-limit awareness.** `grep -rn "429|rate.limit|403|Retry-After|X-RateLimit" --include=*.rs .` → **zero hits repo-wide**. A rate-limit 403 is reported identically to a 404 (`github_release.rs:1203-1215`). The client reads the `link` header for pagination (`:1191-1197`) but no rate-limit headers.
- **No `GITHUB_TOKEN` default.** Auth defaults to `gh auth token` (`config/parser/github.rs:80-84`); `grep -rn "GITHUB_TOKEN"` matches only a memo-cache key (`github_release.rs:88,93,98`).
- **No connection reuse.** A fresh `Client` per asset means a TLS handshake per download. `download_and_extract_to_temp` loops assets at `:1774` calling `download_asset` at `:1793`, plus a nested checksum fetch at `:1591` - roughly 2N clients for N assets.
- **No resume.** `truncate(true)` at `:1549-1554`, no `Range`, no `.part` file.
- **Checksum mismatch hard-fails** at `:1673-1681` without re-downloading - exactly the case a retry fixes.

Three existing degradation behaviours (keep these; they are good, just not retries): stale-cache fallback (`:986-996`), one re-fetch on cache staleness (`:864-882`), fallback to an already-installed version (`:899-925`).

## Approach

### 1. Shared HTTP client with timeouts

New `src/internal/config/up/utils/http.rs`. Note `up/utils/mod.rs:1-57` currently exports progress handlers, run config, shims, and version parsing - **no HTTP utility exists**, which is why the three sites diverged.

A lazily-built, pooled client with:

- `connect_timeout` ~10 s
- request timeout for API calls (bounded, they return small JSON)
- **a separate, generous streaming policy for asset downloads** - a large release asset on a slow link must not be killed by a short total-request timeout. Prefer a read/idle timeout over a total-duration timeout here.
- consistent `user-agent: omni <version>` (the self-updater currently sends none)

Refactor all four sites onto it. Keep `get_github_client`'s header and auth logic (`:1093-1145`) - that part is sound - but have it reuse the shared client rather than building one.

### 2. Retry with exponential backoff and jitter

Retry **only idempotent GETs**, on:

- connection/transport errors
- 5xx
- 429
- GitHub secondary rate-limit 403 (distinguishable by body message via `GithubApiError::from_json`, `github_release.rs:2626`)

Do **not** retry 404 or other 4xx. Jitter matters because several `github-release` steps in one `up` would otherwise retry in lockstep.

### 3. Explicit rate-limit handling

- Match `StatusCode::TOO_MANY_REQUESTS` and `FORBIDDEN` at `github_release.rs:1203`
- Honour `Retry-After` when present
- Read `X-RateLimit-Remaining` / `X-RateLimit-Reset` for the message
- Add `UpError::RateLimited { retry_after }` to `up/error.rs` so the message can say *"rate limited; authenticate to raise your limit from 60/hr"* instead of a generic exec error

An unauthenticated user gets **60 req/hr**, and `list_releases_from_api` paginates at `per_page=100` with **no pacing and no page cap** (`:1170-1230`). This is very easy to trip.

### 4. Fall back to `GITHUB_TOKEN` / `GH_TOKEN`

At `github_release.rs:1037`, when `which::which("gh").is_err()`, check `GITHUB_TOKEN` then `GH_TOKEN` before giving up.

**Biggest single CI win.** In CI, `GITHUB_TOKEN` is conventionally present and `gh` often is not, so omni currently runs unauthenticated - the highest rate-limit-risk configuration.

Keep the existing explicit config modes (`Skip`, `Token`, `TokenEnvVar`, `GhCli` - `config/parser/github.rs:61-84`) taking precedence. This is a fallback, not an override.

Also worth fixing while here: if `HeaderValue::from_str` fails on the token (e.g. a stray newline), `:1121-1129` logs a progress message and **silently proceeds unauthenticated**. That should be louder.

### 5. Retry once on checksum mismatch

`:1673-1681` currently hard-fails. Re-download once, then fail. A truncated download is the common cause and is exactly what a retry fixes.

### 6. Pace and cap pagination

`:1229` advances `cur_page` with no delay and no maximum. Add a small inter-page delay and a sane page cap.

## Adjacent bugs to fix in this phase

- **`cargo_install.rs:873`** reads `config.cache.go_install.versions_expire` - copy-paste error; should be `config.cache.cargo_install.versions_expire`. `CargoInstallCacheConfig::versions_expire` (`config/parser/cache/cargo_install.rs:10,28`) is defined but **never read**. Invisible today because both default to 86400; becomes a real bug the moment a user overrides one.
- **`self_updater.rs:155-172`** - three problems in one function:
  - never checks `response.status()`, so an error body is fed to `serde_json` and silently becomes "no update available"
  - `.expect("Failed to read response")` at `:161-164` **panics** on a mid-stream network drop
  - no `user-agent`
- **`self_updater.rs:167`** - a stray `dbg!` in production, which also does not format its arguments correctly (`dbg!` takes no format args).

## Verification

Test with `mockito` (already a dev-dependency, `Cargo.toml:30`).

- [ ] 429 with `Retry-After` retries after the indicated delay and eventually succeeds
- [ ] 429 exhausting retries yields `UpError::RateLimited` with an actionable message
- [ ] GitHub secondary 403 is retried; a genuine permission 403 is not
- [ ] 404 is not retried
- [ ] connect timeout fires on a black-holed connection instead of hanging
- [ ] a large download is **not** killed by a total-request timeout on a slow link
- [ ] `GITHUB_TOKEN` is used when `gh` is absent; explicit config still wins
- [ ] checksum mismatch triggers exactly one re-download, then fails
- [ ] connection reuse: N assets no longer build N clients
- [ ] self-updater returns `None` rather than panicking on a truncated body
- [ ] `cargo_install` honours `cache.cargo_install.versions_expire`
