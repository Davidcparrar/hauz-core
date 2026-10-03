# Spec: Ingest pipeline: raw message → Bill persisted, idempotent, needs_review fallback (#6)

## Problem
Adds the `ingest` module: one `async fn ingest(raw, &dyn Extractor, &dyn BillStore) ->
Result<Outcome>` that hashes the raw bytes, short-circuits on a known hash, parses,
extracts, decides `Status`, inserts. The same message twice yields one row and the same
id. What the extractor cannot read is stored as `NeedsReview` with the partial fields kept.

## Non-goals
- No extractor composition (`TextExtractor` + `PdfTextExtractor` behind one `Extractor`);
  that combinator ships with the server (#7).
- No persistence of `Extraction::issued`, `notes`, spans or confidences: `Bill` has no
  field for them (a schema change, its own issue).
- No change to `bill`, `store`, `extract`, `email`; no retry, no clock, no S3.

## Assumptions
- no spike: every question is settled by the existing pub surface.
- `Bill::try_from` rejects `Extracted` without vendor, amount AND period, so the issue's
  "amount and currency present" rule is tightened to match (`Money` carries its currency).
- No shipped extractor sets `period`, so every bill `TextExtractor` ingests today is
  `NeedsReview`; the `Extracted` path is tested with a test-local `impl Extractor` fake
  (the trait is the injected seam for the future LLM edge).
- `sha2` is workspace-pinned; the crate adds `{ workspace = true }` only.
- `TextExtractor` reads `Total: 1,234.56 EUR` ⇒ `Money(123456, EUR)` at 90, `Due date:
  15/10/2026` ⇒ due at 90, sender domain ⇒ vendor at 20 (#4, #5).

## Reference implementation
None (no spike).

## Architecture delta
`crates/core/Cargo.toml` += `sha2 = { workspace = true }`. `lib.rs` += `pub mod ingest;`.
New module `ingest`:
- `pub const EXTRACTED_MIN_CONFIDENCE: u8 = 50` — amount confidence floor for `Extracted`.
- `#[non_exhaustive] pub enum Error { Email(#[from] email::Error), Extract(#[from]
  extract::Error), Store(#[from] store::Error), Bill(#[from] bill::Error) }`,
  `thiserror`, `Debug` only (store's error is not `Eq`).
- `pub enum Outcome { Created(BillId), Duplicate(BillId) }` (`Debug, Clone, PartialEq, Eq`).
- `pub fn raw_hash(raw: &[u8]) -> RawHash` — SHA-256 of the raw bytes.
- `pub async fn ingest(raw: &[u8], ex: &dyn Extractor, st: &dyn BillStore) ->
  Result<Outcome, Error>`; the future is `Send`. In order:
  1. `h = raw_hash(raw)`; `st.find_by_hash(&h)` is `Some(b)` ⇒ `Ok(Duplicate(b.id()))`,
     extractor never called.
  2. `Envelope::parse(raw)?`, then `ex.extract(&env)?`.
  3. `id` = `BillId` of the lowercase hex of `h` (64 chars; deterministic, so the store's
     `DuplicateId` is unreachable). Draft from the extraction's values (amount, due,
     vendor, period); `Extracted` iff `amount` is `Some` with `confidence >=
     EXTRACTED_MIN_CONFIDENCE` and `vendor` and `period` are `Some`, else `NeedsReview`
     keeping every present field. `Bill::try_from(draft)?`.
  4. `st.insert(&h, &bill)?`: `Inserted(id)` ⇒ `Created(id)`; `Duplicate(id)` (concurrent
     writer won) ⇒ `Duplicate(id)`.
- `PROMOTES: ingest` → fill its `docs/architecture.md` line; one `docs/decisions.md` line
  (id = hex(sha256(raw)); `Extracted` = amount ≥ 50 + vendor + period; extractor `Err`
  stores nothing).

## Test plan
Files: `tests/unit_ingest.rs` (`InMemoryStore`; test-local `Fixed(Extraction)` and
`Failing` extractors), `tests/integration_ingest.rs` (real `TextExtractor` +
`SqliteStore` on `TmpDbFile`), `tests/property_ingest.rs`. New in `tests/common/mod.rs`:
`bill_eml()`, a 7-bit `text/plain` message from `billing@acme-power.example`, body
`Total: 1,234.56 EUR` / `Due date: 15/10/2026`. Async tests are `#[tokio::test]`.
- AC1 [integration] WHEN `bill_eml()` is ingested into an empty store THE SYSTEM SHALL
  return `Created(id)`, `id.as_str()` = lowercase hex of `raw_hash(raw)`, and `st.get(&id)`
  SHALL be `NeedsReview` with amount `Money(123456, EUR)`, due 2026-10-15, vendor
  `acme-power.example`, no period.
- AC2 [integration] WHEN the same bytes are ingested again THE SYSTEM SHALL return
  `Duplicate(id)` with AC1's id and `st.list()` SHALL have length 1.
- AC3 [integration] WHEN `fixtures/plain.eml` (no amount, no date) is ingested THE SYSTEM
  SHALL return `Created(_)` and store `NeedsReview` with amount and due `None`.
- AC4 [integration] WHEN `fixtures/malformed.eml` is ingested THE SYSTEM SHALL return
  `Err(Error::Email(_))` and `st.list()` SHALL be empty; a second call SHALL also `Err`.
- AC5 [unit] WHEN `Fixed` yields amount at 90, vendor and period THE SYSTEM SHALL store
  `Extracted` with amount, due, vendor, period equal to the extraction's values.
- AC6 [unit] WHEN `Fixed` yields amount at 49, vendor and period THE SYSTEM SHALL store
  `NeedsReview` with the amount still present.
- AC7 [unit] WHEN `Fixed` yields amount at 90 and vendor but no period THE SYSTEM SHALL
  store `NeedsReview`.
- AC8 [unit] WHEN `Failing` returns `Err(extract::Error::InvalidConfidence(101))` THE
  SYSTEM SHALL return `Err(Error::Extract(_))` and `st.list()` SHALL be empty.
- AC9 [unit] WHEN a bill is already stored under `raw_hash(raw)` and `Failing` is the
  extractor THE SYSTEM SHALL return `Duplicate(that id)`.
- AC10 [unit] WHEN `ingest(..)` is passed to `fn assert_send<T: Send>(_: &T)` THE SYSTEM
  SHALL compile.
- AC11 [property] FOR ALL `Extraction` e (#4's strategy) THE SYSTEM SHALL ingest
  `bill_eml()` with `Fixed(e)` into a fresh `InMemoryStore` as `Created`, stored status
  `Extracted` iff `e.amount.confidence >= 50 ∧ e.vendor ∧ e.period`, stored amount, due,
  vendor, period equal to e's values either way.

<!-- GATE 1: [ ] EARS, pub-only [ ] integration present (email+extract+store) [ ] no
     [unverified] load-bearing [ ] non-goals [ ] PROMOTES [ ] ~15k context [ ] ≤800 words -->
