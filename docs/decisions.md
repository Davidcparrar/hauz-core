# Decisions
<!-- Append-only, one line each, newest last. No word budget. Format:
     - YYYY-MM-DD #<issue>: chose X over Y — because Z -->
- 2026-09-21 #0: workspace with a `core` lib + thin binaries — so specs test one crate's pub API and shells stay untested-by-design
- 2026-09-21 #0: sqlx + SQLite (one file, WAL) with a Litestream sidecar to S3 over Turso — because sqlx has no libSQL driver, the data is ~0.3 GB/month at 10K users, and cost is a wash (<$10/mo either way); Turso stays possible as a second `BillStore` impl
- 2026-09-21 #0: raw emails and attachments are S3 objects, only structured rows go in the DB — because attachments are ~100× the row data (~30 GB/month vs ~0.3 GB)
- 2026-09-21 #0: money = integer minor units + ISO currency code, no decimal crate — because it is exact, sortable, and trivially stored as INTEGER
- 2026-09-21 #0: `time` over `jiff`/`chrono` for dates — because sqlx ships a native `time` feature, so no glue types
- 2026-09-21 #0: allowed deps += sqlx, mail-parser, pdf-extract, serde, serde_json, sha2, time — pinned in the workspace so implementers only add `{ workspace = true }` in their crate
- 2026-09-21 #1: `Bill` is built via `TryFrom<BillDraft>` (plain struct, pub fields) with `#[serde(try_from, into)]` over a many-argument constructor — because one entry point enforces the status/field invariants for both code and JSON
