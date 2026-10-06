# Architecture
<!-- ≤1000 words, verify-enforced. Interface-grained crate/module map; decisions go to
     docs/decisions.md. -->

## Purpose
- Turn an emailed bill (body, attachments) into a structured `Bill`.
- Persist every record idempotently (same email ⇒ one bill).
- Stay honest: unreadable ⇒ `needs_review`, never guessed.

Mail arrives by webhook POST or Gmail (planned). Out of scope: analytics, web UI.

## Crate map
```
┌──────────────┐ ┌─────────────┐ ┌──────────────┐
│ server (bin) │ │ cli (bin)   │ │ tui (bin)    │  thin shells
│ axum         │ │ hauz ingest │ │ read-only    │
└──────┬───────┘ └──────┬──────┘ └──────┬───────┘
       └────────────────┼───────────────┘
                        ▼
        ┌───────────────┐
        │ core (lib)    │   all domain logic; the only crate specs test
        └───────────────┘
   binaries → core only; core never imports a binary or a spike
```

## Modules in `core`
- `bill` — owns the domain vocabulary: `Bill`, `BillDraft`, `BillId`, `Money`
  (minor units + `Currency`: active ISO 4217 only), `Vendor`, `BillingPeriod`, `Status { Extracted, NeedsReview }`.
  Parse, don't validate: fallible constructors; `Bill` only via `TryFrom<BillDraft>`, serde too.
  `Extracted` needs vendor + amount + (period or `issued`); other fields optional.
- `email` — owns MIME decoding (mail-parser): `Envelope::parse(&[u8]) ->
  Result<Envelope, Error>`. `Envelope` and `Document` are pub-field records: `subject`,
  `sender` (addr-spec, required), `sender_name` (trimmed display name, `None` if blank or = `sender`), `date` (UTC), `text`, `html`, `documents: Vec<Document
  { mime: MimeType, filename, bytes }>` per attachment, `message/rfc822` not recursed. `MimeType`:
  lowercase `type/subtype`. `Error { Malformed, MissingSender, InvalidMimeType }`.
- `extract` — owns "envelope ⇒ candidate fields": `trait Extractor: Send +
  Sync { fn extract<'a>(&'a self, &'a Envelope) -> BoxFuture<'a, Result<Extraction, Error>> }`
  (`dyn`-safe), `Extraction` (pub-field record: `amount`, `issued`, `due`, `period`, `vendor`, each
  `Option<Field<T>>`; `is_complete(min_confidence: u8)` ⇔ amount at ≥ threshold plus vendor
  plus (period or issued)), `Field<T> { value, confidence: Confidence (0..=100), span: Span
  { source: Source { Text, Html, Document(i), Model }, start, end } }`, `notes: BTreeSet<Note
  { NoTextLayer { document }, UnreadableArchive { document }, LlmUnavailable, LlmMalformed }>`, `Error { InvalidConfidence, Pdf,
  Llm(llm::Error), Zip { document, source } }`), `merge(Vec<Extraction>)` (highest confidence per
  field, notes unioned, deterministic ties: order-insensitive, idempotent), `TextExtractor` (no-regex scanner over `text` and stripped `html`; a bare `$` is no currency; vendor `sender_name` else domain, confidence 20),
  `PdfTextExtractor` (same scanner over each PDF's `text_layer(&[u8]) ->
  Result<Option<String>>`, pdf-extract under `catch_unwind`; image-only ⇒ `NoTextLayer`,
  corrupt ⇒ `Error::Pdf`),
  `XmlInvoiceExtractor` (DIAN zips via `zip::read`: UBL `Invoice` fields at confidence 100; unsupported zip ⇒ `UnreadableArchive`, malformed ⇒ `Error::Zip`), `Chain`
  (`Chain::new(Vec<Box<dyn Extractor>>)`, an `Extractor`: runs each in order, first
  `Err` wins, else `merge`), `Escalate` (`Escalate::new(primary, secondary, min_confidence)`:
  `primary` if complete, else merged with `secondary`; either `Err` propagates).
- `store` — owns persistence: `RawHash`, `InsertOutcome { Inserted, Duplicate }`,
  `trait BillStore` (`insert`, `get`, `find_by_hash`, `list`; async `BoxFuture`,
  `dyn`-safe), `SqliteStore` (sqlx, embedded migrations, WAL; `open_read_only` creates/migrates nothing), `InMemoryStore` test fake.
- `llm` — owns the LLM edges (injectable traits) and the extractor over them: `trait LlmClient { fn complete(&self, &LlmRequest) -> BoxFuture<Result<String,
  Error>> }` over `LlmRequest { instructions, parts: Vec<Part { Text, Png, Pdf }>, schema:
  schemars::Schema }`, `RigClient::new(Provider, model)` (sole rig-core importer; `Provider { Ollama, Anthropic, OpenAi }`, keys redacted in `Debug`; `Pdf` on Ollama ⇒ `Unsupported`),
  `trait Rasterizer { fn rasterize(&self, pdf, max_pages) -> Result<Vec<Vec<u8>>> }` with
  `Pdftoppm::new(dpi)` (shells out), and
  `Config::from_env(get)` (`HAUZ_LLM_PROVIDER` `ollama|anthropic|openai`, `HAUZ_LLM_MODEL`, provider key/URL); `Error { Client, Unsupported, Rasterizer, Config }`.
  `LlmExtractor::new(client, rasterizer, LlmOptions { delivery: PdfDelivery { RasterizedPages | Native }, max_pages, max_confidence, max_body_chars })` (`Default`: pages, 4, 70, 20k) is an `Extractor`: sends the body, PDF pages (or PDFs) and text layers; maps the reply's nullable JSON fields through `bill`, caps confidence, span `Source::Model`; a `Client` error or bad JSON ⇒ empty extraction + `Note::LlmUnavailable`/`LlmMalformed` ; `Unsupported`/`Rasterizer` ⇒ `extract::Error::Llm`.
- `zip` — owns archive decoding, std only: `read(&[u8]) -> Result<Vec<Entry { name, bytes }>,
  Error>` inflates methods 0/8 (hand-rolled RFC 1951), CRC- and
  size-checked; `MAX_ENTRY_BYTES` (64 MiB); `Error { Malformed { reason }, Unsupported {
  feature } }` for zip64, encryption, data descriptors, other methods.
- `ingest` — owns the pipeline: `async fn ingest(raw: &[u8], ex: &dyn Extractor,
  st: &dyn BillStore) -> Result<Outcome, Error>`, `Outcome { Created(BillId),
  Duplicate(BillId) }`, `raw_hash(&[u8]) -> RawHash` (SHA-256), `EXTRACTED_MIN_CONFIDENCE`
  (50), `Error { Email, Extract, Store, Bill }`. Order: hash →
  `find_by_hash` short-circuit → parse → extract → build `Bill` → insert. Bill id = lowercase
  hex of the hash. `Status::Extracted` iff `is_complete(EXTRACTED_MIN_CONFIDENCE)`, else
  `NeedsReview` keeping present fields. A parse or extractor `Err` stores nothing; `notes` are not persisted.

## Entry points
- server (`crates/server`, lib + `main.rs`): `AppState::new(store, extractor, ApiToken)` (`ApiToken::new`: blank ⇒ `None`,
  `Debug` redacted), `router(state)`, `MAX_BODY_BYTES` (25 MiB ⇒ 413). Every `/v1` route needs
  `Authorization: Bearer <token>` (constant-time), else 401 `"unauthorized"` pre-body. `POST /v1/ingest/email` takes raw RFC 5322 bytes: 201 created / 200 duplicate, body `{"id"}`, 400 on `ingest::Error::{Email,
  Extract}`, 500 `"internal error"` otherwise. `GET /v1/bills/{id}`: 200 `Bill` JSON (`BillDraft` shape), else 404. `main.rs`
  reads `HAUZ_API_TOKEN` (required), `DATABASE_URL` and `BIND_ADDR` (default
  `127.0.0.1:8080`), runs `Chain([XmlInvoiceExtractor, TextExtractor, PdfTextExtractor])`, wrapped in `Escalate` over
  `LlmExtractor(RigClient, Pdftoppm::new(150))` when the LLM env is set (config error ⇒ abort). Untested by design.
- cli (`crates/cli`, bin-only, binary `hauz`): `hauz ingest <file.eml> [--db <sqlite path>]`
  (any order; `--db` defaults to `./hauz.db`) reads the file and LLM env (config error ⇒ exit 1 before the DB opens), runs the server's extractor through `ingest`; stdout is `Created <id>` or `Duplicate <id>`. Exit 0; 1 on a runtime error, 2 on a usage error (message on stderr). Args parsed by hand.
- tui (`crates/tui`, lib + bin `hauz-tui`): `hauz-tui [--db <path>]` (default `./hauz.db`); pub `load` (`open_read_only` + `list`, before the terminal opens), `App` (`r` filters `needs_review`) and `draw`. Exit 1 runtime, 2 usage.

## Storage
SQLite through sqlx, one file, WAL mode; a Litestream sidecar replicates the WAL to a DigitalOcean
Spaces bucket from one Droplet (#22).

## Planned (filed, not built)
- `mail` (#42, #43): mail-source edge trait, `GmailSource` (REST over `reqwest`, refresh token,
  `gmail.readonly`, label poll) ingesting in-process; `hauz gmail-auth`/`fetch`; server poll.

## Risks / debt
- Pre-#38 non-ISO currency rows are `Corrupt`, failing `list`. A slightly wrong PDF xref reads empty.
- Single-writer SQLite: more writers need Turso/Postgres.
