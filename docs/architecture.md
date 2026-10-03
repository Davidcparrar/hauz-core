# Architecture
<!-- ≤1000 words, verify-enforced. Interface-grained crate/module map; decisions go to
     docs/decisions.md, one line each. -->

## Purpose
- Turn an emailed bill (body and/or attachments) into a structured `Bill`: vendor, amount,
  currency, period, due date.
- Persist every record durably and idempotently (the same email twice ⇒ one bill).
- Stay honest: what the extractor cannot read is stored `needs_review`, never guessed.

Out of scope: fetching mail (a webhook hands us the raw message), analytics, frontend, mobile.

## Crate map
```
┌──────────────┐   ┌──────────────┐
│ server (bin) │   │ cli (bin)    │   thin shells: email in, core calls out
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
- `bill` — owns the domain vocabulary: `Bill`, `BillDraft`, `BillId`, `Money`
  (minor units + `Currency`), `Vendor`, `BillingPeriod`, `Status { Extracted, NeedsReview }`.
  Parse, don't validate: fallible constructors; `Bill` only via `TryFrom<BillDraft>`, serde too.
- `email` — owns MIME decoding (mail-parser): `Envelope::parse(&[u8]) ->
  Result<Envelope, Error>`. `Envelope` and `Document` are pub-field records: `subject`,
  `sender` (addr-spec, required), `date` (UTC), `text`, `html`, `documents: Vec<Document
  { mime: MimeType, filename, bytes }>` — one per attachment, decoded, `message/rfc822` not recursed. `MimeType` is a
  lowercase `type/subtype` newtype. `Error { Malformed, MissingSender, InvalidMimeType }`.
- `extract` — owns "envelope ⇒ candidate fields": `trait Extractor: Send +
  Sync { fn extract<'a>(&'a self, &'a Envelope) -> BoxFuture<'a, Result<Extraction, Error>> }`
  (`dyn`-safe, async like `BillStore`),
  `Extraction` (pub-field record: `amount`, `issued`, `due`, `period`, `vendor`, each
  `Option<Field<T>>`; `is_complete(min_confidence: u8)` ⇔ amount at ≥ threshold plus vendor
  plus period), `Field<T> { value, confidence: Confidence (0..=100), span: Span
  { source: Source { Text, Html, Document(i), Model }, start, end } }`, `notes: BTreeSet<Note
  { NoTextLayer { document }, LlmUnavailable, LlmMalformed }>`, `Error { InvalidConfidence, Pdf,
  Llm(llm::Error) }`), `merge(Vec<Extraction>)` (highest confidence per
  field, notes unioned; ties by value then span: order-insensitive, idempotent), `TextExtractor` (heuristic scanner over `text` and tag-stripped `html`; no regex; a bare `$` is no currency, `COP$`/`US$` are),
  `PdfTextExtractor` (same scanner over the text layer of each `application/pdf`
  document via `text_layer(&[u8]) -> Result<Option<String>>`, pdf-extract under
  `catch_unwind`; image-only ⇒ `NoTextLayer` note, corrupt ⇒ `Error::Pdf`), `Chain`
  (`Chain::new(Vec<Box<dyn Extractor>>)`, itself an `Extractor`: runs each in order, first
  `Err` wins, else `merge`), `Escalate` (`Escalate::new(primary, secondary, min_confidence)`: runs
  `primary`, returns it when complete, else `merge`s it with `secondary`; either `Err`
  propagates). Binaries hold one `Chain`, wrapped in `Escalate` with `llm::LlmExtractor` when configured.
- `store` — owns persistence: `RawHash`, `InsertOutcome { Inserted, Duplicate }`,
  `trait BillStore` (`insert`, `get`, `find_by_hash`, `list`; async via the crate-root
  `BoxFuture`, re-exported; `dyn`-safe), `SqliteStore` (sqlx, embedded migrations, WAL), `InMemoryStore` fake for
  tests.
- `llm` — owns the LLM-side system edges as injectable traits and the extractor over them: `trait LlmClient { fn complete(&self, &LlmRequest) -> BoxFuture<Result<String,
  Error>> }` over `LlmRequest { instructions, parts: Vec<Part { Text, Png, Pdf }>, schema:
  schemars::Schema }`, `RigClient::new(Provider, model)` (sole rig-core importer; `Provider { Ollama { base_url }, Anthropic { api_key, base_url }, OpenAi { .. } }`, keys redacted in `Debug`; `Pdf` on Ollama ⇒ `Unsupported`),
  `trait Rasterizer { fn rasterize(&self, pdf, max_pages) -> Result<Vec<Vec<u8>>> }` with
  `Pdftoppm::new(dpi)` shelling out to poppler's `pdftoppm`, and
  `Config::from_env(get)` reading `HAUZ_LLM_PROVIDER` (`ollama|anthropic|openai`), `HAUZ_LLM_MODEL`, the provider's key or base URL; `Error { Client, Unsupported, Rasterizer, Config }`.
  `LlmExtractor::new(client, rasterizer, LlmOptions { delivery: PdfDelivery { RasterizedPages | Native }, max_pages, max_confidence, max_body_chars })` (`Default`: pages, 4, 70, 20k) is an `Extractor`: sends the body, each PDF's pages (or the PDF) and text layer; maps the reply's required-but-nullable JSON fields through `bill`'s constructors, caps confidence, span `Source::Model`; a `Client` error or bad JSON ⇒ empty extraction + `Note::LlmUnavailable`/`LlmMalformed` (bill lands `NeedsReview`); `Unsupported`/`Rasterizer` ⇒ `extract::Error::Llm`.
- `ingest` — owns the pipeline: `async fn ingest(raw: &[u8], ex: &dyn Extractor,
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
  413 beyond). `POST /v1/ingest/email` takes raw RFC 5322 bytes (any
  `Content-Type`): 201 created / 200 duplicate with `{"id"}`, 400 on `ingest::Error::{Email,
  Extract}`, 500 (`"internal error"`, nothing leaked) otherwise. `GET /v1/bills/{id}`:
  200 with the `Bill` JSON (`BillDraft` shape), 404 if unknown or malformed. `main.rs`
  reads `DATABASE_URL` (SQLite path, `sqlite://` prefix tolerated) and `BIND_ADDR` (default
  `127.0.0.1:8080`), runs `Chain([TextExtractor, PdfTextExtractor])`, wrapped in `Escalate(chain,
  LlmExtractor(RigClient, Pdftoppm::new(150)), EXTRACTED_MIN_CONFIDENCE)` when `llm::Config::from_env`
  is `Some`; a config error aborts startup. Untested by design.
- cli (`crates/cli`, bin-only, binary `hauz`): `hauz ingest <file.eml> [--db <sqlite path>]`
  (any order after `ingest`; `--db` defaults to `./hauz.db`) reads the
  file, reads the LLM env as the server (config error ⇒ exit 1 before the DB opens), opens `SqliteStore` and runs the same extractor through
  `ingest`; stdout is exactly `Created <id>` or `Duplicate <id>`. Exit 0, 1 on a runtime
  error (unreadable file, `ingest::Error`, store; message on stderr), 2 on a usage error
  (usage on stderr); `-h|--help` prints usage. Args parsed by hand (no clap).

## Storage
SQLite through sqlx (`sqlite` feature), one file, WAL mode; a Litestream sidecar replicates the WAL to a bucket, the
application never talks to S3. Turso stays possible as a second `BillStore` impl.

## Risks / debt
- The real corpus (7 bills, 2026-10-03) all lands `NeedsReview` heuristically: vendor = sender domain,
  no amount for the four bare-`$` Colombian bills (#28), no period. The LLM pass (gemma4, before #28)
  made 2 `Extracted` and named 5 vendors. DIAN e-invoice zips (UBL XML) deserve an exact extractor
  (#27). A slightly wrong xref reads as empty.
- Single-writer SQLite suits one ingest service; a second writer means Turso/Postgres.
