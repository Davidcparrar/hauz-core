# Spec: tui: read-only bill browser (ratatui) (#41)

## Problem
Stored bills are visible only via `sqlite3` or `GET /v1/bills/{id}`. `hauz-tui --db <path>`
opens the SQLite file read-only: a bill list, a `needs_review` filter toggle and a detail
pane with every field. It never writes, so it runs beside the server.

## Non-goals
- No editing, re-ingest, HTTP/remote mode, live reload, search or sort.
- No corrupt-row tolerance: `list` fails on one, so the TUI exits 1 with the store error.
- No migrations: an unmigrated DB fails (exit 1).
- No process-level happy-path test (needs a terminal).

## Assumptions
- no spike: sqlx `read_only(true)` is `SQLITE_OPEN_READONLY`, which reads a WAL database
  with or without a live writer (SQLite WAL docs); `TestBackend` exposes its `Buffer`.
- Design call: pins `ratatui = "0.30"`, `crossterm = "0.29"` (its version); human yes in decision #41.
- Design call: `crates/tui` is lib + bin (the server's shape); `main.rs` is an untested
  thin shell: args → `load` → terminal loop (`ratatui::init`/`restore`). The DB opens
  **before** the terminal.
- Design call (test levels, like GPUI): TUI tests are `[e2e]` only — `TestBackend` renders
  through the pub API in `crates/tui/tests/e2e_render.rs`, `assert_cmd` in
  `crates/tui/tests/e2e_cli.rs`. The constitution gets this line.
- Design call: newest-ingested row first (reverse `list` order).
- Design call: amounts = minor units at two decimals + code (`123456` COP → `1234.56 COP`,
  `-5` USD → `-0.05 USD`), the extractors' own assumption. Absent fields render `-`; id
  prefix = 8 chars.
- Design call: `Down`/`j`, `Up`/`k` move, clamped; `r` toggles the filter and selects the
  first row; `q`/`Esc` quit; other keys ignored.
- Design call: args parsed by hand like `hauz`: `--db <path>` (default `./hauz.db`),
  `-h`/`--help` → usage on stdout, exit 0; usage error exit 2, runtime error exit 1.

## Architecture delta
- `store` (core): `SqliteStore::open_read_only(path: &Path) -> Result<SqliteStore, Error>`:
  read-only, never creates the file, no journal-mode change, no migrations.
- New crate `crates/tui` (package `hauz-tui`, lib `hauz_tui`, bin `hauz-tui`); deps
  core, ratatui, crossterm, tokio, anyhow; dev assert_cmd, time. Pub:
  - `App::new(Vec<Bill>)` (`list` order), `App::on_key(&mut self, crossterm::event::KeyCode)
    -> Flow`, `App::selected() -> Option<&Bill>`, `App::needs_review_only() -> bool`;
    `enum Flow { Continue, Quit }`.
  - `draw(frame: &mut ratatui::Frame<'_>, app: &App)`: list left, detail right (id, vendor,
    amount, period start–end, issued, due, status); list title says `needs_review only`
    while filtered; an empty view shows `no bills`.
  - `async fn load(db: &Path) -> anyhow::Result<App>` = `open_read_only` + `list` + `App::new`.
- `ratatui`, `crossterm` join `[workspace.dependencies]`.
- `PROMOTES: store, crates/tui` → architecture, decisions, constitution TUI line.

## Test plan
Fixtures: `Bill`s via `BillDraft` (`tests/common/mod.rs`); temp-dir DBs via `SqliteStore::open`.
- AC1 [unit] WHEN `open_read_only` gets a nonexistent path THE SYSTEM SHALL return
  `Err(Error::Backend(_))` and the path SHALL still not exist.
- AC2 [unit] WHEN a DB holds two bills and its writer is still open THE SYSTEM SHALL return
  both, in `list` order, from `open_read_only(..).list()`.
- AC3 [unit] WHEN a read-only handle gets `insert` THE SYSTEM SHALL return
  `Err(Error::Backend(_))` and the writer's `list` SHALL be unchanged.
- AC4 [e2e] WHEN `draw` renders two bills (an `Extracted` one, an amountless
  `NeedsReview` one) THE SYSTEM SHALL show both rows newest first with id prefix, vendor,
  `1234.56 COP`, issued, due, status, and `-` for the absent amount.
- AC5 [e2e] WHEN `Down`/`j` moves the selection THE SYSTEM SHALL show every field of the
  selected bill in the detail pane; `Up`/`k` on the first row SHALL keep it selected.
- AC6 [e2e] WHEN `r` is pressed THE SYSTEM SHALL list only `NeedsReview` bills, select the
  first, title the list `needs_review only` and report `needs_review_only()`; again restores all.
- AC7 [e2e] WHEN the visible list is empty (none stored or filtered to none) THE SYSTEM SHALL
  render `no bills`, `selected()` SHALL be `None` and moving SHALL not panic.
- AC8 [e2e] WHEN `q` or `Esc` is pressed THE SYSTEM SHALL return `Flow::Quit`, else `Continue`.
- AC9 [e2e] WHEN `load` gets a DB whose writer handle was dropped THE SYSTEM SHALL return an
  `App` whose `draw` shows every stored bill's vendor.
- AC10 [e2e] WHEN `hauz-tui --db <missing path>` runs THE SYSTEM SHALL exit 1, stderr naming
  the path, creating no file.
- AC11 [e2e] WHEN `hauz-tui` gets an unknown argument or a valueless `--db` THE SYSTEM SHALL
  exit 2 with usage on stderr; `--help` SHALL print usage on stdout and exit 0.

<!-- GATE 1 CHECKLIST (Leader self-check, before labelling `approved`):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [x] no [unverified] assumption is load-bearing; spike questions answered or carried
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens (else split into two issues)
     [x] ≤800 words -->
