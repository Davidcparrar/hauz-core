# Spec: extract: exact extractor for DIAN e-invoice zips (UBL XML + PDF) (#27)

## Problem
Colombian DIAN e-invoices arrive as one `application/zip` attachment: `ad<id>.xml` (a UBL
`AttachedDocument` whose CDATA `Description` wraps the `Invoice`) plus a PDF. This feature adds
`extract::XmlInvoiceExtractor`: it opens the zip with `core::zip`, plucks the exact amount,
currency, supplier, issue/due dates and (when declared) period at confidence 100, and joins
the heuristic chain ahead of any LLM escalation. No LLM, no new dependency.

## Non-goals
- No general XML parser (no `quick-xml`): a local-name plucker handling CDATA and the five
  predefined entities.
- Only `Invoice` roots: `CreditNote`/`DebitNote` or a zip without one contribute nothing.
- The zip's PDF is not read.
- No period guessed from the issue date: without `InvoicePeriod` the bill stays `NeedsReview`
  (`Escalate`'s LLM pass may still supply one).
- Corpus bytes never enter the repo; fixtures are synthetic (`features/27/make_fixtures.py`).

## Assumptions
- [spike-verified] (`features/27/spike/findings.md` Q2) the `Invoice` sits in CDATA under
  `AttachedDocument/Attachment/ExternalReference/Description`; `PayableAmount` always has
  `currencyID` and a `D+.DD` body; `PaymentMeans/PaymentDueDate` is present in 3/3,
  `Invoice/DueDate` in 2/3, `InvoicePeriod` in 1/3; dates are `YYYY-MM-DD`; supplier name
  repeats across `PartyTaxScheme`, `PartyLegalEntity` and `PartyName`.
- [spike-verified] (Q3) the plucker in `features/27/spike/ref/src/xml.rs` finds a nested chain
  and decodes `&amp;`; a raw-text scan finds tags inside CDATA.
- Design call: `pub struct XmlInvoiceExtractor;` (derives as `PdfTextExtractor`, an
  `Extractor`) in `crates/core/src/extract/ubl.rs` (`pub use` from
  `extract.rs`), plucker in private `extract/xml.rs`. Documents with mime `application/zip` or
  `application/x-zip-compressed` go through `zip::read`; the first `*.xml` entry (ASCII
  case-insensitive) whose `from_utf8_lossy` text holds an `Invoice` start tag is parsed;
  per-document extractions are `merge`d.
- Design call: lookups are anchored under `Invoice` (parent chain of local names, any prefix)
  so the outer `IssueDate`/`SenderParty` never match. amount = `LegalMonetaryTotal/
  PayableAmount` text (`digits[.d{1,2}]`, scaled to 2 decimals) with `currencyID` via
  `Currency::new`; vendor = under `AccountingSupplierParty/Party` the first non-empty of
  `PartyTaxScheme/RegistrationName`, `PartyLegalEntity/RegistrationName`, `PartyName/Name`;
  issued = `IssueDate`; due = `PaymentMeans/PaymentDueDate`, else `DueDate`; period =
  `InvoicePeriod/StartDate`+`EndDate` via `BillingPeriod::new`. Dates take the leading
  `YYYY-MM-DD`. A value failing its grammar or constructor omits that field (`Ok`). Confidence
  100; span = `Source::Document(i)`, the element body's byte range in the entry's text.
- Design call: `extract::Error` gains `Zip { document: usize, #[source] source: zip::Error }`
  (decision #5 precedent: a corrupt attachment is surfaced, not silently filed).
- Design call: both binaries run `Chain([XmlInvoiceExtractor, TextExtractor,
  PdfTextExtractor])`; the CLI e2e covers the shape, `server/main.rs` stays untested.

## Reference implementation
`features/27/spike/ref/src/xml.rs` (145 lines). Illustrative; reimplement under the lints.

## Architecture delta
- `extract`: `XmlInvoiceExtractor`, `Error::Zip`; `extract` now uses `zip`.
- `server/src/main.rs`, `cli/src/main.rs`: the chain gains the new extractor first.
- No manifest change. `PROMOTES: extract` → architecture + decisions line (Leader, step 8).

## Test plan
Fixtures: `python3 features/27/make_fixtures.py` once from the repo root writes
`crates/core/tests/fixtures/ubl/{dian_full,dian_no_period,not_invoice}.zip`, `dian_full.xml`,
`{dian_full,dian_no_period,dian_corrupt}.eml` (the last = `dian_full.zip` minus 10 bytes).
Files: `unit_extract.rs` (AC1–AC5), `integration_extract_email.rs` (AC6),
`integration_ingest.rs` (AC7), `crates/cli/tests/e2e_cli.rs` (AC8–AC9, `include_bytes!` of
the core fixtures), `property_extract.rs` (AC10). Lift `property_zip.rs`'s stored-zip writer
into `tests/common/mod.rs` for AC5/AC10. "AC1 fields" = amount `Money::new(18435000, COP)`,
vendor `Acme & Luz S.A.S. E.S.P.`, issued 2026-09-01, due 2026-09-25, period 2026-08-01..31.
- AC1 [unit] WHEN document 0 is `dian_full.zip` as `application/zip` THE SYSTEM SHALL return
  the AC1 fields at confidence 100 with `Source::Document(0)` spans over each element body in
  the entry's text; notes empty.
- AC2 [unit] WHEN document 0 is `dian_no_period.zip` THE SYSTEM SHALL return amount
  `Money::new(9950, USD)`, vendor `Gas Natural Ejemplo S.A.`, issued 2026-09-10, due
  2026-09-30, `period: None`; `is_complete(50)` false.
- AC3 [unit] WHEN no document is a zip, or the only zip is `not_invoice.zip` THE SYSTEM SHALL
  return `Extraction::default()`.
- AC4 [unit] WHEN document 0 is `text/plain` and document 1 is `dian_full.zip` minus its last
  10 bytes THE SYSTEM SHALL return `Err(Error::Zip { document: 1, source: Malformed { .. } })`.
- AC5 [unit] WHEN `dian_full.xml` is rewritten (test-built stored zip) with `PayableAmount`
  `1,234.56`, `currencyID="cop"`, `IssueDate` `01/09/2026`, or `EndDate` before `StartDate`
  THE SYSTEM SHALL omit respectively amount, amount, issued, period, keeping the rest.
- AC6 [integration] WHEN `dian_full.eml` is parsed by `Envelope::parse` and given to
  `XmlInvoiceExtractor` THE SYSTEM SHALL return the AC1 fields.
- AC7 [integration] WHEN `dian_full.eml` is ingested through `Chain([XmlInvoiceExtractor,
  TextExtractor, PdfTextExtractor])` into a `SqliteStore` THE SYSTEM SHALL store an `Extracted`
  bill with the AC1 vendor, amount, period and due; WHEN `dian_no_period.eml`, a `NeedsReview`
  bill with AC2's amount, vendor and due, `period: None`.
- AC8 [e2e] WHEN `hauz ingest dian_full.eml --db <tmp>` runs THE SYSTEM SHALL exit 0, print
  `Created <id>` and leave an `Extracted` bill with the AC1 amount.
- AC9 [e2e] WHEN `hauz ingest dian_corrupt.eml --db <tmp>` runs THE SYSTEM SHALL exit 1,
  write stderr and store nothing.
- AC10 [property] FOR ALL strings of 0..=512 chars stored as the `.xml` entry of a one-entry
  zip given as `application/zip` THE SYSTEM SHALL return `Ok` from `extract` without panicking.

<!-- GATE 1: all seven boxes ticked (e2e: AC8 happy, AC9 failure) -->
