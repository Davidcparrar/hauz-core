# Architecture
<!-- ≤1000 words, verify-enforced. Interface-grained crate/module map — the one page every
     agent loads. Decisions go to docs/decisions.md, one line each. -->

## Purpose
- Turn a bill that arrived by email (body and/or attachments) into a structured `Bill`
  record: who charged what, how much, in which currency, for which period, due when.
- Persist every record durably and idempotently (the same email twice ⇒ one bill) so a
  later analysis service can query cost over time.
- Stay honest about uncertainty: what the extractor cannot read is stored as
  `needs_review`, never guessed and never dropped.

Out of scope here: fetching mail (a webhook, Gmail push, or SES hands us the raw
message), analytics, frontend, mobile.

## Crate map
```
┌──────────────┐   ┌──────────────┐
│ server (bin) │   │ cli (bin)    │   thin shells: raw email in, core calls out
│ axum         │   │ hauz ingest  │
└──────┬───────┘   └──────┬───────┘
       └────────┬─────────┘
                ▼
        ┌───────────────┐
        │ core (lib)    │   all domain logic; the only crate specs test
        └───────────────┘
   arrows point one way: binaries → core; core never imports a binary or a spike
```

## Modules in `core`
- `bill` — owns the domain vocabulary; interface: `Bill`, `BillDraft`, `BillId`, `Money`
  (minor units + `Currency`), `Vendor`, `BillingPeriod`, `Status { Extracted, NeedsReview }`.
  Parse, don't validate: fallible constructors; `Bill` only via `TryFrom<BillDraft>`, serde too.
- `email` — owns MIME decoding (mail-parser); interface: `Envelope::parse(&[u8]) ->
  Result<Envelope, Error>`. `Envelope` and `Document` are pub-field records: `subject`,
  `sender` (addr-spec, required), `date` (UTC), `text`, `html`, `documents: Vec<Document
  { mime: MimeType, filename, bytes }>` — one per attachment across nested multiparts,
  decoded; a `message/rfc822` part is one `Document`, not recursed. `MimeType` is a
  lowercase `type/subtype` newtype. `Error { Malformed, MissingSender, InvalidMimeType }`.
- `extract` — owns "document ⇒ candidate fields"; interface: `trait Extractor`,
  `Extraction` (partial fields + confidence + source span), `merge(Vec<Extraction>)`.
  First impl is heuristic text/HTML; PDF text and an LLM-backed impl slot in behind the
  same trait.
- `store` — owns persistence; interface: `RawHash`, `InsertOutcome { Inserted, Duplicate }`,
  `trait BillStore` (`insert`, `get`, `find_by_hash`, `list`; async via boxed futures,
  `dyn`-safe), `SqliteStore` (sqlx, embedded migrations, WAL), `InMemoryStore` fake for
  other modules' tests.
- `ingest` — owns the pipeline; interface: `async fn ingest(raw: &[u8], ex: &dyn Extractor,
  st: &dyn BillStore) -> Result<Outcome>`. Hashes the raw message for idempotency,
  parses, extracts, decides `Status`, persists.

## Entry points
- server: `pub fn router(state: AppState) -> axum::Router` (lib) + `main.rs` binds and
  serves. `POST /v1/ingest/email` takes the raw RFC 5322 message body; `GET /v1/bills/{id}`.
- cli: `hauz ingest <file.eml>` — same pipeline, local file, for dev and replay.

## Storage
SQLite through sqlx (`sqlite` feature), one file, WAL mode. Durability and "SQLite in S3"
come from a Litestream sidecar replicating the WAL to a bucket and restoring on boot;
the application code never talks to S3. Turso stays possible later behind `BillStore`
(sqlx has no libSQL driver, so it would be a second store impl, not a config switch).

## Risks / debt
- Extraction quality on real bills is unknown until we have a corpus; the heuristic
  extractor is a baseline.
- Scanned/image-only PDFs need OCR or a vision model — deferred until a real sample
  demands it.
- Single-writer SQLite suits one ingest service; a second writer means Turso or Postgres.
