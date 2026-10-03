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

Out of scope: fetching mail (a webhook or push hands us the raw message), analytics, frontend, mobile.

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
- `extract` — owns "envelope ⇒ candidate fields"; interface: `trait Extractor: Send +
  Sync { fn extract<'a>(&'a self, &'a Envelope) -> BoxFuture<'a, Result<Extraction, Error>> }`
  (`dyn`-safe, async like `BillStore`),
  `Extraction` (pub-field record: `amount`, `issued`, `due`, `period`, `vendor`, each
  `Option<Field<T>>`; `is_complete(min_confidence: u8)` ⇔ amount at ≥ threshold plus vendor
  plus period), `Field<T> { value, confidence: Confidence (0..=100), span: Span
  { source: Source { Text, Html, Document(i) }, start, end } }`, `notes: BTreeSet<Note
  { NoTextLayer { document } }>`), `merge(Vec<Extraction>)` (highest confidence per
  field, notes unioned; ties by structural value order, then span — so it is
  order-insensitive and idempotent), `TextExtractor` (heuristic scanner over `text` and tag-stripped `html`; no regex),
  `PdfTextExtractor` (same scanner over the text layer of each `application/pdf`
  document via `text_layer(&[u8]) -> Result<Option<String>>`, pdf-extract under
  `catch_unwind`; image-only ⇒ `NoTextLayer` note, corrupt ⇒ `Error::Pdf`), `Chain`
  (`Chain::new(Vec<Box<dyn Extractor>>)`, itself an `Extractor`: runs each in order, first
  `Err` wins, else `merge`), `Escalate` (`Escalate::new(primary, secondary, min_confidence)`: runs
  `primary`, returns it when complete, else `merge`s it with `secondary`; either `Err`
  propagates). Binaries hold one `Chain`; an LLM-backed impl (#23) slots in as `Escalate`'s
  secondary.
- `store` — owns persistence; interface: `RawHash`, `InsertOutcome { Inserted, Duplicate }`,
  `trait BillStore` (`insert`, `get`, `find_by_hash`, `list`; async via `BoxFuture`, the
  crate-root alias re-exported here; `dyn`-safe), `SqliteStore` (sqlx, embedded migrations, WAL), `InMemoryStore` fake for
  other modules' tests.
- `llm` — owns the LLM-side system edges as injectable traits (the extractor itself is #26): `trait LlmClient { fn complete(&self, &LlmRequest) -> BoxFuture<Result<String,
  Error>> }` over `LlmRequest { instructions, parts: Vec<Part { Text, Png, Pdf }>, schema:
  schemars::Schema }`, `RigClient::new(Provider, model)` (the only code importing rig-core; `Provider { Ollama { base_url }, Anthropic { api_key, base_url }, OpenAi { .. } }`, keys redacted in `Debug`; a `Pdf` part on Ollama is `Error::Unsupported`),
  `trait Rasterizer { fn rasterize(&self, pdf, max_pages) -> Result<Vec<Vec<u8>>> }` with
  `Pdftoppm::new(dpi)` shelling out to poppler's `pdftoppm` in a temp dir, and
  `Config::from_env(get)` reading `HAUZ_LLM_PROVIDER` (`ollama|anthropic|openai`), `HAUZ_LLM_MODEL`, the provider's key or base URL; `Error { Client, Unsupported, Rasterizer, Config }`.
- `ingest` — owns the pipeline; interface: `async fn ingest(raw: &[u8], ex: &dyn Extractor,
  st: &dyn BillStore) -> Result<Outcome, Error>` (`Send` future), `Outcome { Created(BillId),
  Duplicate(BillId) }`, `raw_hash(&[u8]) -> RawHash` (SHA-256), `EXTRACTED_MIN_CONFIDENCE`
  (50), `Error { Email, Extract, Store, Bill }` (`#[from]` each). Order: hash →
  `find_by_hash` short-circuit → parse → extract → build `Bill` → insert. Bill id = lowercase
  hex of the hash. `Status::Extracted` iff `is_complete(EXTRACTED_MIN_CONFIDENCE)`, else
  `NeedsReview` keeping every present field. A parse or extractor `Err`
  stores nothing; `issued` and `notes` are not persisted.

## Entry points
- server (`crates/server`, lib + `main.rs`): `AppState::new(Arc<dyn BillStore>, Arc<dyn
  Extractor>)`, `pub fn router(state: AppState) -> axum::Router`, `MAX_BODY_BYTES` (25 MiB,
  413 beyond). `POST /v1/ingest/email` takes the raw RFC 5322 bytes as the body (any
  `Content-Type`): 201 created / 200 duplicate with `{"id"}`, 400 on `ingest::Error::{Email,
  Extract}`, 500 (`"internal error"`, nothing leaked) on anything else. `GET /v1/bills/{id}`:
  200 with the `Bill` JSON (`BillDraft` shape), 404 for an unknown or malformed id. `main.rs`
  reads `DATABASE_URL` (SQLite path, `sqlite://` prefix tolerated) and `BIND_ADDR` (default
  `127.0.0.1:8080`), runs `Chain([TextExtractor, PdfTextExtractor])`; untested by design.
- cli (`crates/cli`, bin-only, binary `hauz`): `hauz ingest <file.eml> [--db <sqlite path>]`
  (flag and positional in any order after `ingest`; `--db` defaults to `./hauz.db`) reads the
  file, then opens `SqliteStore` and runs `Chain([TextExtractor, PdfTextExtractor])` through
  `ingest`; stdout is exactly `Created <id>` or `Duplicate <id>`. Exit 0 on success, 1 on a
  runtime error (unreadable file, `ingest::Error`, store; anyhow message on stderr), 2 on a
  usage error (usage line on stderr); `-h|--help` prints usage on stdout. Args are parsed by
  hand (no clap); no env configuration.

## Storage
SQLite through sqlx (`sqlite` feature), one file, WAL mode. Durability and "SQLite in S3"
come from a Litestream sidecar replicating the WAL to a bucket and restoring on boot;
the application code never talks to S3. Turso stays possible as a second `BillStore` impl (sqlx has no libSQL driver).

## Risks / debt
- The real corpus (7 bills, 2026-10-03) all lands `NeedsReview`: vendor = sender domain,
  `$` read as `USD` where it meant `COP`, no period. The LLM pass (#26) targets exactly that;
  DIAN e-invoice zips (UBL XML) deserve an exact extractor (#27). A PDF with a slightly wrong xref reads as empty (pdf-extract loses data silently).
- Single-writer SQLite suits one ingest service; a second writer means Turso or Postgres.
