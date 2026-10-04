# Review: exact extractor for DIAN e-invoice zips (#27)

## Cycle 1 — 2026-10-03, reviewer on a5f2d16
VERDICT: REJECT (code-defect)
verify.sh: `verify: ALL GREEN`

REQUIRED CHANGES:
1. `crates/core/src/extract/xml.rs` `find_element`: parent lookups scan forward with no
   scope, so a step can match past the end of its parent. Probes through the pub API: an
   AC2-shaped supplier (only `PartyLegalEntity`) plus a customer
   `PartyTaxScheme/RegistrationName` yields the customer as vendor; an `Invoice` with no due
   date and an outer `AttachedDocument` `DueDate` after the CDATA yields the outer date.
   Restrict each step (including `Invoice` itself) to the matched parent's body.
2. `crates/core/tests/unit_extract.rs`: regression tests for both probes (vendor is the
   supplier; `due` is `None`).
3. `ac4_corrupt_zip_attachment_is_zip_error` must also assert `source: Malformed { .. }`.

- Passed: AC1–AC10 present at their tagged levels in the named files, pub API only; both
  chains `[XmlInvoiceExtractor, TextExtractor, PdfTextExtractor]`; `Error::Zip` carries the
  index; checked amount scaling, `.get`-based slicing; no manifest/dependency/docs change.
