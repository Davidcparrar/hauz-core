# Spec: extract: an unreadable non-invoice zip must not fail the whole ingest (#36)

## Problem
Since #27 every zip attachment goes through `zip::read`, and any `Err` becomes
`extract::Error::Zip`: the extraction fails, the server answers 400, the CLI exits 1, nothing
is stored. Ordinary archives (macOS Archive Utility's data descriptors, zip64, encryption,
bzip2) are `zip::Error::Unsupported`, so one unrelated archive beside a bill loses the bill.
After this feature an *unsupported* archive degrades to a new `Note::UnreadableArchive
{ document }` and the other documents and extractors still count; a *malformed* archive is
still the caller's bad input (`Error::Zip`, 400).

## Non-goals
- No new zip features (data descriptors, zip64, encryption, bzip2) in `core::zip`; that
  would shrink the problem, not remove it, and is its own issue.
- `notes` stay unpersisted (no store/`Bill` change); persisting notes is cross-cutting.
- No content sniffing; which attachments are opened (zip mimes, octet-stream `*.zip`) is unchanged.
- `Malformed` handling, `PdfTextExtractor` and `LlmExtractor` are unchanged.

## Assumptions
- no spike: a match on an existing error enum plus an existing fixture
  (`crates/core/tests/fixtures/zip/bzip2.zip`, method 12 ⇒ `Unsupported`).
- Design call: degrade by error class, not by content. A zip that `zip::read` cannot open
  has unknowable contents, so "no readable `Invoice`" cannot be tested. `Unsupported` (any
  `feature`) means a valid archive this reader cannot open ⇒ degrade. `Malformed` means
  corrupt bytes ⇒ `Err`, consistent with a corrupt PDF (`Error::Pdf`, decisions #5/#7).
- Design call: `Note::UnreadableArchive { document: usize }` (the index into
  `Envelope::documents`; `Note` is `Copy`, so no feature string). `XmlInvoiceExtractor`
  contributes an extraction holding only that note for the document and keeps going with
  the remaining documents.
- Consequence, accepted: an unsupported DIAN zip now yields `NeedsReview` (plus the LLM pass
  when configured) instead of a 400. Notes are not persisted, so the stored bill does not say
  why.
- The #27 malformed-zip criteria (unit AC4, CLI AC9) stay as they are and act as regression tests.
- E2E fixture: `crates/core/tests/fixtures/zip/bill_with_bzip2.eml`, a hand-written text/plain
  part identical to `crates/server/tests/fixtures/bill.eml`'s body (amount 1,234.56 EUR, due
  15/10/2026, sender `billing@acme-power.example`) plus `bzip2.zip` attached base64 as
  `application/zip`, filename `archive.zip`.

## Architecture delta
- `extract`: `Note::UnreadableArchive { document }`; `XmlInvoiceExtractor` maps
  `zip::Error::Unsupported` to that note and keeps `Malformed` as `Error::Zip`.
- No dependency or manifest change; binaries untouched.
- `PROMOTES: extract` → architecture line + decisions line (Leader).

## Test plan
"A test-built descriptor zip" = `tests/common`'s stored-zip writer output with general-purpose
flag bit 3 set in its local and central headers.
- AC1 [unit] WHEN document 0 is `bzip2.zip` as `application/zip` THE SYSTEM SHALL have
  `XmlInvoiceExtractor` return `Ok` with no fields and notes `{UnreadableArchive { document: 0 }}`.
- AC2 [unit] WHEN document 0 is a test-built descriptor zip and document 1 is `dian_full.zip`
  THE SYSTEM SHALL return the #27 AC1 fields and notes `{UnreadableArchive { document: 0 }}`.
- AC3 [unit] WHEN document 0 is `bzip2.zip` and document 1 is `dian_full.zip` minus its last
  10 bytes THE SYSTEM SHALL return `Err(Error::Zip { document: 1, source: Malformed { .. } })`.
- AC4 [integration] WHEN `bill_with_bzip2.eml` is ingested through `Chain([XmlInvoiceExtractor,
  TextExtractor, PdfTextExtractor])` into a `SqliteStore` THE SYSTEM SHALL return `Created`
  and store a `NeedsReview` bill with amount `Money::new(123456, EUR)` and due 2026-10-15.
- AC5 [e2e] WHEN `bill_with_bzip2.eml` is POSTed to `/v1/ingest/email` on a router running
  that chain THE SYSTEM SHALL answer 201, and `GET /v1/bills/{id}` SHALL return the bill
  with AC4's amount.
- AC6 [e2e] WHEN `dian_corrupt.eml` is POSTed to that router THE SYSTEM SHALL answer 400 and
  store nothing.
- AC7 [e2e] WHEN `hauz ingest bill_with_bzip2.eml --db <tmp>` runs THE SYSTEM SHALL exit 0,
  print `Created <id>`, and store the bill.
- AC8 [property] FOR ALL envelopes whose zip documents are each a test-built descriptor zip
  or `bzip2.zip`, at arbitrary indices among `text/plain` documents, THE SYSTEM SHALL return
  `Ok` with no fields and exactly one `UnreadableArchive { document: i }` per zip index `i`.

<!-- GATE 1 CHECKLIST (Leader self-check, before labelling `approved`):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [x] no [unverified] assumption is load-bearing; spike questions answered or carried
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens (else split into two issues)
     [x] ≤800 words -->
