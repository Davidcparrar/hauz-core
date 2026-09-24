# Spec: BillStore trait + SqliteStore (sqlx, embedded migrations) (#2)
<!-- ≤800 words, verify-enforced. Drafted by the Leader WITH the human; approved at Gate 1. -->

## Problem
`hauz-core` can build a `Bill` but cannot keep one. This adds the `store` module: a
`BillStore` trait (the persistence edge), `SqliteStore` on sqlx with embedded migrations
and WAL, and an `InMemoryStore` fake for other modules' tests. Insert is idempotent on
the raw-message hash: the same email twice yields one row and the same id.

## Non-goals
- No hashing: the caller (`ingest`) computes the `RawHash`; `store` only keys on it.
- No update/delete; no pagination, filtering, or sort options on `list`.
- No timestamps on `Bill`; `inserted_at` is a store column, not a domain field.
- No pool tuning, backups, Litestream, or Turso.
- No `sqlx::query!` macros (need `DATABASE_URL` at build); runtime `sqlx::query` only.
- No new dependencies: `sqlx`, `tokio`, `time`, `thiserror` are workspace-pinned.

## Assumptions
- [spike-verified] sqlx 0.9.0 with the pinned features builds with bundled SQLite;
  `create_if_missing` + `journal_mode(Wal)` + `sqlx::migrate!("./migrations")` work.
- [spike-verified] `time::Date`, `Option<time::Date>`, `i64` bind/decode directly;
  `[u8; 32]` does not: bind `as_slice()`, decode `Vec<u8>` then `try_into`.
- [spike-verified] boxed-future dyn trait (no `async-trait`) is clippy-clean under the
  workspace lints; `UNIQUE(hash)` + `ON CONFLICT DO NOTHING` + select returns the prior id.
- [spike-verified] a default `SqlitePool` handles concurrent inserts under WAL.

## Reference implementation
`features/2/spike/ref/src/lib.rs` (trait shape, `open`, bind/decode); illustrative only.

## Architecture delta
- New `pub mod store`; `crates/core/migrations/0001_bills.sql` embedded via `migrate!`.
- `crates/core/Cargo.toml`: `sqlx = { workspace = true }`; dev-dep `tokio`.
- Public surface of `store`:
  - `RawHash([u8; 32])`: `new([u8; 32])`, `as_bytes()`; `Debug, Clone, Copy, PartialEq,
    Eq, Hash`.
  - `Error` (`thiserror`, `#[non_exhaustive]`): `DuplicateId(BillId)`, `Corrupt { id:
    BillId, source: bill::Error }`, `Backend(#[from] sqlx::Error)`, `Migrate(#[from]
    sqlx::migrate::MigrateError)`.
  - `InsertOutcome { Inserted(BillId), Duplicate(BillId) }`; `Duplicate` carries the id
    of the row already holding that hash.
  - `type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>`.
  - `trait BillStore: Send + Sync` (dyn-compatible): `insert(&self, &RawHash, &Bill) ->
    BoxFuture<Result<InsertOutcome, Error>>`, `get(&self, &BillId) ->
    BoxFuture<Result<Option<Bill>, Error>>`, `find_by_hash(&self, &RawHash) ->
    BoxFuture<Result<Option<Bill>, Error>>`, `list(&self) -> BoxFuture<Result<Vec<Bill>,
    Error>>` (insertion order, oldest first).
  - `SqliteStore`: `async fn open(path: &Path) -> Result<Self, Error>` (creates file, sets
    WAL, runs migrations); `impl BillStore`; `Debug + Clone`.
  - `InMemoryStore`: `new()`, `Default`, `Debug`; `impl BillStore` with identical
    semantics over a `std::sync::Mutex` (recover a poisoned lock, never panic).
- Table `bills`, one column per field (queryable, no JSON blob): `id TEXT PK`, `hash BLOB
  NOT NULL UNIQUE`, `vendor`, `amount_minor INTEGER`, `currency`, `period_start`,
  `period_end`, `due`, `status NOT NULL`, `inserted_at NOT NULL`. Rows rebuild a `Bill`
  via `BillDraft` → `TryFrom` (`Error::Corrupt` on failure).
- Insert check order: hash known → `Duplicate(existing id)`; else id known →
  `Error::DuplicateId`; else insert → `Inserted(id)`.
- `PROMOTES: store` → rewrite the `store` line in `docs/architecture.md` (`RawHash`,
  `InsertOutcome`, `InMemoryStore`, boxed-future async trait) and add one
  `docs/decisions.md` line: boxed-future dyn trait over `async-trait` or generics.

## Test plan
Files: `tests/unit_store.rs` (`InMemoryStore`), `tests/integration_store.rs`
(`SqliteStore`, unique tmp file). Shared scenario fns over `&dyn BillStore` in
`tests/common/mod.rs`; one `acN_…` fn per tag per file. `#[tokio::test]`.
- AC1 [unit][integration] WHEN a `Bill` is inserted under a fresh hash THE SYSTEM SHALL
  return `Inserted(id)` and `get(id)` SHALL return an equal `Bill`.
- AC2 [unit][integration] WHEN `get` or `find_by_hash` receives an unknown key THE SYSTEM
  SHALL return `Ok(None)`.
- AC3 [unit][integration] WHEN a second `Bill` (any id or content) is inserted under a
  stored hash THE SYSTEM SHALL return `Duplicate(first id)` and store nothing (`list` length
  unchanged, `find_by_hash` yields the first bill).
- AC4 [unit][integration] WHEN a `Bill` is inserted whose id exists under another hash
  THE SYSTEM SHALL return `Error::DuplicateId` and store nothing.
- AC5 [unit][integration] WHEN three bills are inserted THE SYSTEM SHALL `list` them in
  insertion order regardless of id lexical order.
- AC6 [unit][integration] WHEN a `NeedsReview` bill with every optional field `None` is
  inserted THE SYSTEM SHALL round-trip it unchanged through `get`.
- AC7 [integration] WHEN `SqliteStore::open` runs twice on one path THE SYSTEM SHALL
  succeed both times and the second handle SHALL see rows inserted through the first.
- AC8 [integration] WHEN `open` targets a path whose directory does not exist THE SYSTEM
  SHALL return `Err(Error::Backend(_))`.
- AC9 [integration] WHEN the file is opened THE SYSTEM SHALL leave `PRAGMA journal_mode`
  at `wal`, observed through a separate raw sqlx connection to the same file.

<!-- GATE 1 CHECKLIST (human + Leader, before "approved"):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration: store + real SQLite file; no entry point ⇒ no e2e)
     [x] no [unverified] assumption; all four spike questions answered
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present (new pub module)
     [x] fits an implementer context of ~15k tokens
     [x] ≤800 words -->
