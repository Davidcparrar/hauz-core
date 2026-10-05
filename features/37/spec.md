# Spec: bill: persist the issue date; Extracted = amount + vendor + (period or issued) (#37)

## Problem
`Extracted` demands amount + vendor + period (decision #6); the 2026-10-04 corpus replay put
0/7 bills there: most state no period. `Extraction::issued` is extracted but never persisted.
This feature stores it on `Bill` and makes the rule amount (≥ threshold) + vendor + (period
**or** issued), in `bill` and `extract`. DIAN invoices without `InvoicePeriod` become `Extracted`.

## Non-goals
- No period derived from `issued` or anything else.
- No threshold change, and no threshold on `issued` (vendor/period have none).
- `notes` stay unpersisted; no output change beyond the bill JSON's `issued`.
- No backfill of rows stored before the migration.

## Assumptions
- no spike: a nullable date like `due` plus an additive migration; nothing unknown.
- Design call: `BillDraft` gains `pub issued: Option<time::Date>`, `Bill` a private field +
  `issued() -> Option<time::Date>`. serde derive reads a missing `Option` key as `None`, so
  pre-#37 JSON still deserializes; serialized bills always carry `issued`, encoded like `due`.
- Design call: the new rule keeps `Error::IncompleteBill`; its message and `Status::Extracted`'s
  doc say "vendor, amount, and period or issue date".
- Design call: `migrations/0002_bill_issued.sql` = `ALTER TABLE bills ADD COLUMN issued DATE;`;
  `0001` is never edited (sqlx checksums applied migrations).
- Accepted risk: `TextExtractor`'s fallback `issued` (any non-due date, low confidence) can now
  complete a bill with amount and vendor; the issue rules out threshold changes.
- Old-rule tests are amended in place: `unit_bill` AC7 (#1), `unit_extract` AC1 (#21) and AC2
  (#27), `integration_ingest` AC7 (#27), `property_bill`'s generator, `property_ingest`'s law.
  Every `BillDraft { .. }` literal gains `issued`.

## Architecture delta
- `bill`: `BillDraft.issued`, `Bill::issued()`, amended `Extracted` invariant.
- `extract`: `is_complete` ⇔ amount ≥ threshold ∧ vendor ∧ (period ∨ issued).
- `store`: migration `0002`; `SqliteStore` writes/reads `issued`.
- `ingest`: maps `extraction.issued` into the draft.
- No dependency or manifest change; binaries untouched.
- `PROMOTES: bill, extract, store, ingest` → architecture + decisions line revising #6.

## Test plan
Fixtures exist: `crates/core/tests/fixtures/ubl/dian_no_period.{zip,eml}` (issued 2026-09-10,
no period); `crates/server/tests/fixtures/bill.eml` (no issued, no period). Server e2e reads the
DIAN eml via `include_bytes!("../../core/tests/fixtures/…")` through a router running
`Chain([XmlInvoiceExtractor, TextExtractor, PdfTextExtractor])` ("the #27 chain").
- AC1 [unit] WHEN an `Extracted` draft has amount, vendor, `issued: Some(d)`, no period THE
  SYSTEM SHALL return `Ok(bill)` with `issued() == Some(d)`, `period() == None`.
- AC2 [unit] WHEN an `Extracted` draft lacks vendor, or amount, or both period and issued THE
  SYSTEM SHALL return `Err(Error::IncompleteBill)`; period without issued SHALL succeed.
- AC3 [unit] WHEN a bill's `serde_json::to_value` has its `issued` key removed THE SYSTEM SHALL
  deserialize it with `issued() == None`; a serialized bill SHALL always contain `issued`.
- AC4 [unit] WHEN an `Extraction` has amount at 50, vendor, issued, no period THE SYSTEM SHALL
  report `is_complete(50)` true; neither period nor issued ⇒ false; amount at 49 ⇒ false.
- AC5 [unit] WHEN `XmlInvoiceExtractor` reads `dian_no_period.zip` THE SYSTEM SHALL return the
  #27 AC2 fields and `is_complete(50)` true.
- AC6 [integration] WHEN a bill with `issued: Some(d)` is inserted into a `SqliteStore` THE
  SYSTEM SHALL return `issued() == Some(d)` from `get`, `find_by_hash` and `list`.
- AC7 [integration] WHEN a SQLite file migrated with only `0001` (copied into a tmp dir, run by
  `sqlx::migrate::Migrator::new(dir)`) holds a raw-SQL row and `SqliteStore::open` is called on
  it THE SYSTEM SHALL apply `0002` and return that row with `issued() == None`, rest unchanged.
- AC8 [integration] WHEN `dian_no_period.eml` is ingested through the #27 chain into a
  `SqliteStore` THE SYSTEM SHALL store it `Extracted`, issued 2026-09-10, `period: None`.
- AC9 [e2e] WHEN `dian_no_period.eml` is POSTed to `/v1/ingest/email` and its bill fetched THE
  SYSTEM SHALL answer 200 with `status` `"extracted"`, `period` `null`, `issued` 2026-09-10.
- AC10 [e2e] WHEN `bill.eml` is POSTed and fetched THE SYSTEM SHALL answer `status`
  `"needs_review"` with `issued` `null`.
- AC11 [e2e] WHEN `hauz ingest dian_no_period.eml --db <tmp>` runs THE SYSTEM SHALL exit 0,
  print `Created <id>`, and store it `Extracted` with issued 2026-09-10.
- AC12 [property] FOR ALL valid `Bill`s (generator includes `issued`) THE SYSTEM SHALL
  round-trip JSON to an equal bill, and every `Extracted` bill SHALL carry vendor, amount and
  period-or-issued.
- AC13 [property] FOR ALL ingested extractions THE SYSTEM SHALL store `Extracted` iff amount
  confidence ≥ 50 ∧ vendor ∧ (period ∨ issued), with stored `issued` equal to the extraction's.

<!-- GATE 1 CHECKLIST (Leader self-check, before labelling `approved`):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [x] no [unverified] assumption is load-bearing; spike questions answered or carried
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens (else split into two issues)
     [x] ≤800 words -->
