# Spec: re-extract an already-stored bill — `hauz ingest --reextract` (#54)

## Problem
`ingest` short-circuits on the raw hash, so a bill stored `needs_review` before an extractor
improved can never be re-read: re-ingesting prints `Duplicate`. `hauz ingest --reextract
<file.eml>` re-runs parse → extract → build `Bill` and overwrites the stored row's extracted
fields and status, keeping id, hash and `inserted_at`. It prints `Updated <id>`, `Unchanged
<id>` (new bill equals stored), or `Created <id>` (message never stored).

## Non-goals
- No HTTP route, no TUI action, no `--reextract` on `hauz fetch`.
- No bulk mode / `--all`; no re-extraction from stored messages (raw emails aren't kept).
- No history/audit of previous extractions; `notes` stay unpersisted.
- No "keep the better field" merge with the stored row.

## Assumptions
- no spike: a store `UPDATE` plus a pipeline variant over existing pub types.
- Design call — **replace exactly**: the row becomes precisely what the new run built,
  including a downgrade `Extracted` → `NeedsReview` and fields becoming `None`. Tuning must
  see the extractor's real output; merging with the old row would hide regressions.
- Design call — `Unchanged` performs no write (`Bill: Eq`).
- Design call — `replace` is keyed by `BillId` (= hash hex, what callers hold). Unknown id
  ⇒ new `store::Error::NotFound(BillId)` (`Error` is `#[non_exhaustive]`).
- Design call — new `Reextracted` enum, not new `Outcome` variants: `Outcome` is matched
  exhaustively in server, cli and `mail`, which never produce these cases.
- Every failure (parse, extract, `Bill` build) precedes any write, so a failed run leaves
  the row untouched.

## Architecture delta
- `store`: `BillStore` gains `fn replace<'a>(&'a self, bill: &'a Bill) -> BoxFuture<'a,
  Result<(), Error>>`: overwrites vendor, amount, period, issued, due, status of the row
  with `bill.id()`; never touches `id`, `hash`, `inserted_at` or list position. No row ⇒
  `Err(NotFound(id))`, nothing written. `SqliteStore`: one `UPDATE … WHERE id = ?`, zero
  rows ⇒ `NotFound`. `InMemoryStore`: in-place swap. No migration.
- `ingest`: `pub async fn reextract(raw: &[u8], ex: &dyn Extractor, st: &dyn BillStore) ->
  Result<Reextracted, Error>`, `pub enum Reextracted { Created(BillId), Updated(BillId),
  Unchanged(BillId) }`. Order: hash → parse → extract → build `Bill` (shared private helper
  with `ingest`) → `find_by_hash`: `None` ⇒ `insert` ⇒ `Created` (an `insert` `Duplicate`
  from a concurrent writer falls through to the stored path); equal ⇒ `Unchanged`; else
  `replace` ⇒ `Updated`.
- cli: `hauz ingest [--reextract] <file.eml> [--db <path>]`, flag anywhere after `ingest`;
  stdout `Created|Updated|Unchanged <id>`; errors exit 1 as `ingest`; USAGE updated; config
  still resolves before the DB opens.
- PROMOTES: store, ingest — `docs/architecture.md` + a `docs/decisions.md` line.

## Test plan
- AC1 [unit] WHEN `InMemoryStore::replace` gets a bill whose id is stored THE SYSTEM SHALL
  return `Ok(())`; then `get` and `find_by_hash` (original hash) return the new bill and
  `list` keeps the row's position.
- AC2 [unit] WHEN `InMemoryStore::replace` gets an unstored id THE SYSTEM SHALL return
  `Err(Error::NotFound(id))` and `list` is unchanged.
- AC3 [integration] WHEN `SqliteStore::replace` runs THE SYSTEM SHALL behave as AC1 and AC2,
  leaving the row's `hash` and `inserted_at` columns identical (read via raw SQL).
- AC4 [integration] WHEN `reextract` runs `Chain([Xml, Text, PdfText])` on `dian_full.eml` whose hash
  holds a bare `NeedsReview` bill THE SYSTEM SHALL return `Updated(id)` and store it
  `Extracted` with the DIAN amount.
- AC5 [integration] WHEN `reextract` runs `TextExtractor` on `bill_eml()` whose hash holds a
  complete `Extracted` bill THE SYSTEM SHALL return `Updated(id)` and the stored bill equals
  exactly the new `NeedsReview` bill (no old field kept).
- AC6 [integration] WHEN `reextract` runs on a message already ingested with the same
  extractor THE SYSTEM SHALL return `Unchanged(id)` with the stored bill unchanged.
- AC7 [integration] WHEN `reextract` runs on an unstored message THE SYSTEM SHALL return
  `Created(id)` and store the bill `ingest` would.
- AC8 [integration] WHEN `reextract` gets a malformed message, or an extractor returning
  `Err`, for a stored id THE SYSTEM SHALL return `Error::Email` / `Error::Extract` with the
  stored bill unchanged.
- AC9 [e2e] WHEN `hauz ingest --reextract dian_full.eml --db <db>` runs on a db seeded (via
  `SqliteStore`) with a bare `NeedsReview` bill at that hash THE SYSTEM SHALL exit 0, print
  `Updated <id>\n`, and store it `Extracted` with 18 435 000 COP.
- AC10 [e2e] WHEN `hauz ingest bill.eml` then `hauz ingest bill.eml --reextract` run on one
  db THE SYSTEM SHALL print `Unchanged <id>\n` the second time, exit 0.
- AC11 [e2e] WHEN `hauz ingest --reextract malformed.eml` runs on a db seeded with a bill at
  that hash THE SYSTEM SHALL exit 1, empty stdout, non-empty stderr, stored bill unchanged;
  `hauz fetch --reextract` SHALL exit 2.

<!-- GATE 1 CHECKLIST (Leader self-check):
     [x] EARS, tagged, numbered, pub behavior only
     [x] required levels present (integration; e2e happy + failure)
     [x] no unverified load-bearing assumption
     [x] non-goals exclude the creep
     [x] binaries → core; PROMOTES present
     [x] fits ~15k implementer context
     [x] ≤800 words -->
