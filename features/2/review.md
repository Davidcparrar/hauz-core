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
