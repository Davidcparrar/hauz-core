# Review log: #2 BillStore trait + SqliteStore

## Cycle 1 — 2026-09-23
VERDICT: APPROVE
verify.sh: `verify: ALL GREEN`

REQUIRED CHANGES
none

NOTES
- AC1–AC6 present in both `crates/core/tests/unit_store.rs` and `crates/core/tests/integration_store.rs` via shared `&dyn BillStore` scenarios in `tests/common/mod.rs`; AC7–AC9 integration-only. All fns named `acN_*` at the tagged level. Tests use only `hauz_core::{bill,store}` pub items; `InMemoryStore` is a fake at the persistence edge, not a mock of an own type.
- Pub surface matches the spec delta exactly (`RawHash`, `Error` `#[non_exhaustive]` with all four variants, `InsertOutcome`, `BoxFuture`, `BillStore`, `SqliteStore`, `InMemoryStore`); no extras. No `unwrap`/`expect`/`panic` in `crates/core/src`; poisoned lock recovered via `unwrap_or_else(PoisonError::into_inner)`. No `query!` macros. Only `sqlx` dep + `tokio` dev-dep, both `{ workspace = true }`; root `Cargo.toml` untouched; no `features/` path dep.
- AC9 is a real observation: sqlx 0.9 leaves `journal_mode` unset unless requested (`sqlx-sqlite-0.9.0/src/options/mod.rs:183`), so the separate connection reads the persisted mode.
- Insert order is hash → id → insert inside one tx (rollback on `DuplicateId` by drop); `list` uses `ORDER BY rowid ASC`.
- Debt, not blocking: (1) `integration_store.rs` ac1–ac6 never remove their tmp `.sqlite3`; ac7/ac9 remove only the main file, leaving `-wal`/`-shm`, and skip cleanup on early `?`. (2) `row_to_bill` coerces any unknown `status` string to `NeedsReview` instead of `Error::Corrupt`. (3) `decode_id` reports a bad `id` column as `Error::Backend(Decode)`, not `Corrupt`. (4) The deferred read-then-write tx may hit SQLITE_BUSY under the multi-connection pool; worth an `ingest`-time check.

## Cycle 2 — 2026-09-23 (after spec amendment: Corrupt shape, AC10, BEGIN IMMEDIATE, tmp cleanup)
VERDICT: APPROVE
verify.sh: `verify: ALL GREEN`

REQUIRED CHANGES
none

NOTES
- Delta checks all pass. `Error::Corrupt { id: String, reason: String }` is raised for a bad `id` column (`decode_id`, and `build_draft`'s `BillId::new`), an unknown `status` (`decode_status` returns `Err`, no coercion to `NeedsReview`), and any `bill::Error` on `Bill::try_from` — all funneled through `to_corrupt` in `row_to_bill`, carrying the raw stored id.
- AC10 is integration-only and real: `ac10_corrupt_status_reports_error` (get + find_by_hash + list) and `ac10_corrupt_id_reports_error` (find_by_hash + list, `get` correctly omitted) write the bad row through a separate `SqliteConnection`, not through the store.
- `insert` opens `begin_with("BEGIN IMMEDIATE")` before the hash check, so hash → id → write share the write lock; the only early returns are `?`/`Duplicate`/`DuplicateId`, all of which drop `tx` (sqlx queues ROLLBACK on connection return) — no path leaves a transaction open or commits partially.
- `TmpDbFile` with `Drop` removing the file plus `-wal`/`-shm` is used by every `SqliteStore` test (ac1–ac7, ac9, both ac10); no ad-hoc `remove_file` remains. ac8 creates no file, so it needs none.
- Cycle-1 findings still hold: AC1–AC6 at both levels, AC7–AC9 integration-only, all `acN_*`; pub surface matches the amended spec exactly; no `unwrap`/`expect`/`panic` in `src`; no `query!`; only the cycle-1 `sqlx` dep + `tokio` dev-dep; no `features/` path dep. The single `#[allow(clippy::too_many_arguments)]` carries an inline reason.
- Docs delta correct: architecture `store` line lists `RawHash`, `InsertOutcome`, `BillStore`, `SqliteStore`, `InMemoryStore`; one `2026-09-23 #2:` decisions line for the boxed-future dyn trait.
- Non-blocking: `decode_id` and `build_draft`'s id branch duplicate the same mapping; cycle-1 debt item 4 (SQLITE_BUSY under the pool) is now more likely to surface as `Backend` under contention — an `ingest`-time concern, outside this spec.
