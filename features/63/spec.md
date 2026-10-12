# Spec: web: read-only admin bill browser on Topcoat (#63)

## Problem
`hauz-tui` is the only way to browse stored bills. A new `crates/web` (lib + bin `hauz-web`)
serves the same view as server-rendered HTML with Topcoat. It is the first step of
`docs/prd.md` and lays down the CSS-token convention that later branding swaps.

## Non-goals
- No auth, users, scopes, OTP or SSO. It binds `127.0.0.1` only.
- No writes, upload, chat, file browser, S3, i18n, JavaScript, Tailwind or Topcoat `asset`/CLI.
- No pagination or search beyond the `needs_review` filter.

## Assumptions
- [spike-verified] Topcoat `=0.10.0` with `default-features = false` and features
  `router, serve, view, discover` adds 46 lock packages; tokio/hyper/tower unify, only
  `hkdf`/`hmac`/`matchit`/`rand_chacha` gain a second version (lock-verified on the branch). MSRV 1.98. Owner approved (#63).
- [spike-verified] Pages render in-process via `Router::handle(Request)` with
  `http_body_util` to read the body. State comes in via `.app_context(store)` and
  `app_context::<SqliteStore>(cx)`. Dynamic segments use `module_param!`.
- [spike-verified] The `tailwind` feature downloads a binary in `build.rs`, and `asset!` needs
  a CLI bundle step (missing ⇒ panic). Both break offline `verify.sh`.
- Design call — **CSS**: `crates/web/src/theme.css` holds every token (colors, font stacks,
  radii, spacing) as `:root` custom properties, with a `prefers-color-scheme: dark` override.
  `app.css` uses only `var(--…)` and contains no color literals. Both are embedded with
  `include_str!` and served together at `GET /style.css` (`text/css`). Branding is a
  `theme.css` swap.
- Design call — **routes**: `/admin` (all), `/admin/needs-review` (filter as its own path,
  since query-string handling is unspiked) and `/admin/bills/{id}`. Unknown or malformed id ⇒ 404.
- Design call — **formatting** matches the TUI: id prefix, `1234.56 COP`, `-` when absent. The
  TUI's helpers are private to a binary crate, so `web` keeps its own copy. That is debt to
  move into core if a third view appears.
- Design call — the root `Cargo.toml` (Leader, in the spec commit) sets `rust-version = "1.98"`
  and adds `topcoat` to `[workspace.dependencies]`.

## Reference implementation
`features/63/spike/ref/src/lib.rs` (router, `module_param!`, CSS route) and `tests/e2e.rs`
(`Router::handle` helper). Illustrative only.

## Architecture delta
PROMOTES: web (new crate boundary). New crate `crates/web` (lib + bin `hauz-web`), depending on `hauz-core`, `topcoat`, `tokio`,
`anyhow`, plus dev `http-body-util`, `assert_cmd` and `time`:
- `pub fn router(store: SqliteStore) -> topcoat::router::Router`: every page reads through
  `BillStore` (`list`, `get`).
- `main.rs`: `hauz-web [--db <path>] [--port <n>]` (defaults `./hauz.db` and `3000`). It calls
  `SqliteStore::open_read_only` before binding `127.0.0.1:<port>` and serves through
  `topcoat::serve`. Exit codes: 1 runtime, 2 usage (hand-parsed like the TUI).
- No `core` change. Architecture gains a `web` entry point, and decisions gain the Topcoat/MSRV line.

## Test plan
`[e2e]` only, like the TUI. In-process tests go in `crates/web/tests/e2e_router.rs`
(`Router::handle` over a store seeded in a tmp dir via `SqliteStore::open` + `insert`, then
reopened read-only). The process tests go in `crates/web/tests/e2e_cli.rs` (`assert_cmd`).
- AC1 [e2e] WHEN `GET /admin` with two bills stored THE SYSTEM SHALL answer 200 HTML listing
  both, newest first. Each row shows the id prefix, vendor, amount (`1234.56 COP`), issued,
  due and status, and links to `/admin/bills/<full id>`.
- AC2 [e2e] WHEN `GET /admin/needs-review` THE SYSTEM SHALL list only `needs_review` bills.
- AC3 [e2e] WHEN `GET /admin/bills/<id>` for a stored bill THE SYSTEM SHALL answer 200 with
  the full id, vendor, amount, period, issued, due and status (`-` for absent fields).
- AC4 [e2e] WHEN `GET /admin/bills/<id>` with an unknown or malformed id THE SYSTEM SHALL
  answer 404.
- AC5 [e2e] WHEN a stored vendor name contains `<script>` THE SYSTEM SHALL render it
  HTML-escaped in list and detail.
- AC6 [e2e] WHEN `GET /style.css` THE SYSTEM SHALL answer 200 `text/css` containing the
  `:root` tokens (`--color-bg`, `--color-fg`, `--color-accent`, `--font-body`, `--radius`) and
  a `prefers-color-scheme: dark` block, and every page SHALL link `/style.css`.
- AC7 [e2e] WHEN the database is empty THE SYSTEM SHALL render `/admin` with 200 and a
  "no bills" message.
- AC8 [e2e] WHEN `hauz-web --db <seeded db> --port <free port>` runs THE SYSTEM SHALL serve
  `GET http://127.0.0.1:<port>/admin` with 200 (the test kills the child after).
- AC9 [e2e] WHEN `hauz-web --db <missing file>` runs THE SYSTEM SHALL exit 1 without creating
  the file, and WHEN `hauz-web --bogus` runs THE SYSTEM SHALL exit 2 with usage.

<!-- GATE 1 CHECKLIST (Leader self-check, before labelling `approved`):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [x] no [unverified] assumption is load-bearing; spike questions answered or carried
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens (else split into two issues)
     [x] ≤800 words -->
