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
- 2026-09-23 #2: `BillStore` is an async trait returning boxed `Send` futures (`dyn`-compatible, no `async-trait` crate) over generics or a sync trait — because sqlx is async, `ingest` and the server want `&dyn BillStore` so the in-memory fake can stand in, and it adds no dependency
- 2026-09-24 #3: `MimeType` is a normalised `type/subtype` newtype over an enum, and `Envelope`/`Document` are pub-field records over accessor types — because the mime set is open (`extract` owns what a mime means) and downstream modules' tests build envelopes by hand
- 2026-09-24 #4: `Extractor::extract` takes `&Envelope` (not `&Document`), `TextExtractor` is a hand-written scanner (no `regex` dependency), and `merge` breaks confidence ties by a structural order on the value then the span — because the vendor fallback needs the sender, the token grammar is small enough that a dependency is not worth a decision, and a total order is what makes merge order-insensitive by construction
