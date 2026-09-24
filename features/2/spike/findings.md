# Feature #2 spike findings — sqlx 0.9 + BillStore trait shape

Proof crate: `features/2/spike/ref/` (`cargo test` — 5/5 pass; `cargo clippy --all-targets --
-D warnings` — clean, workspace lint table copied verbatim).

## Q1 — build + open + migrate
Builds with exactly `sqlx = { version = "0.9", default-features = false, features =
["runtime-tokio", "sqlite", "macros", "migrate", "time"] }` — no system SQLite lib needed
(bundled via `libsqlite3-sys`), no TLS feature required for a sqlite-only pool. Snippet:
```rust
let options = SqliteConnectOptions::new()
    .filename(path)
    .create_if_missing(true)
    .journal_mode(SqliteJournalMode::Wal);
let pool = SqlitePoolOptions::new().connect_with(options).await?;
sqlx::migrate!("./migrations").run(&pool).await?;
```
Difference from 0.8: the feature is `runtime-tokio` (not `runtime-tokio-native-tls`); TLS
feature suffixes are gone since 0.8. Otherwise API is unchanged.
Verdict: **works as specified**. Proof: `ref/src/lib.rs::open`, test `q1_opens_and_migrates`.

## Q2 — type round-trips
`time::Date`, `Option<time::Date>`, `i64` bind/decode directly via `.bind(x)` /
`row.try_get::<T,_>(..)`, stored as SQLite `TEXT`/`INTEGER`. A `[u8; 32]` has **no**
direct `Encode`/`Decode` in sqlx 0.9 — bind as `hash.as_slice()` (`BLOB`), decode as
`Vec<u8>` then `.try_into::<[u8; 32]>()` (infallible here since the column is always
32 bytes by construction).
Verdict: **round-trips, with one indirection for the fixed array**. Proof:
`ref/src/lib.rs`, test `q2_round_trips_types`.

## Q3 — dyn-compatible async trait, no async-trait
The `Pin<Box<dyn Future<...> + Send + 'a>>` shape compiles and is dyn-compatible; both
`SqliteBillStore` (pool-backed) and `MemoryBillStore` (`std::sync::Mutex<Vec<_>>`) are
called through `&dyn BillStore` in `#[tokio::test]`s. `UNIQUE INDEX ON bills(hash)` +
`INSERT ... ON CONFLICT(hash) DO NOTHING` followed by `SELECT id WHERE hash = ?` returns
the pre-existing id on a second insert (asserted `id1 == id2`). No pedantic lint fired
(`must_use_candidate`, `needless_pass_by_value` etc. all clean) — the boxed-future
signature and `&self` receivers avoid them naturally.
Verdict: **compiles clean under `-D warnings`, dedup confirmed**. Proof:
`ref/src/lib.rs`, tests `q3_sqlite_backend_through_dyn`, `q3_memory_backend_through_dyn`.

## Q4 — concurrency
A default `SqlitePool` (unset `max_connections`, i.e. sqlx's default pool size) survives
8 concurrent `tokio::spawn` inserts against one WAL-mode file, run 5x with no flakiness —
sqlx's default `busy_timeout` (5s) absorbs the brief writer contention that WAL still
serializes internally. `max_connections(1)` is not needed for this feature's scale (one
ingest service, low concurrency); revisit only if a heavier write burst appears later.
Verdict: **default pool is fine**. Proof: `ref/src/lib.rs`, test `q4_concurrent_inserts`.

<!-- STATUS: COMPLETE -->
