# Spec: Heuristic extractor over text and HTML bodies (#4)

## Problem
Adds the `extract` module: `trait Extractor` (heuristic now; PDF/LLM/OCR later),
`Extraction` (optional fields, each with confidence and source span), `merge` by
confidence, and `TextExtractor`: a dependency-free heuristic pass over the text body and
tag-stripped HTML body finding amount, issue date, due date, and a sender-domain vendor.

## Non-goals
- No LLM/OCR/PDF extractors; `documents` is never read.
- No new dependency (no `regex`): hand-written scanner plus `time` parsing.
- No period detection (`TextExtractor` leaves the field `None`).
- No month-first numeric, year-less or relative dates.
- No vendor from body text; no HTML sanitising; no serde; no `BillDraft` building.

## Assumptions
- no spike: formats, anchors and tie-break are defined here, not discovered.
- [source-verified] `time::Date::parse` + `format_description!` covers every listed
  date format; `time` is already a `core` dependency.

## Architecture delta
New `pub mod extract`; no `Cargo.toml` change. All types `Debug, Clone, PartialEq, Eq`.
- `Error` (`thiserror`, `#[non_exhaustive]`): `InvalidConfidence(u8)`.
- `Confidence(u8)` in `0..=100`: `new(u8) -> Result`, `get()`, `Ord`, `Hash`.
- `Source` (`#[non_exhaustive]`, `Ord`): `Text`, `Html`, `Document(usize)`.
- `Span { pub source: Source, pub start: usize, pub end: usize }`, `Ord`; byte offsets
  into the scanned string (`text` verbatim, or the stripped HTML).
- `Field<T> { pub value: T, pub confidence: Confidence, pub span: Span }`.
- `Extraction` (pub fields, `Default`): `amount: Option<Field<Money>>`, `issued`, `due:
  Option<Field<time::Date>>`, `period: Option<Field<BillingPeriod>>`,
  `vendor: Option<Field<Vendor>>`.
- `trait Extractor: Send + Sync` (`dyn`-safe): `fn extract(&self, envelope: &Envelope)
  -> Result<Extraction, Error>`.
- `pub fn merge(Vec<Extraction>) -> Extraction`: per field keep the highest confidence;
  ties → smallest value under a structural order (amount: (minor units, currency code);
  dates: chronological; period: (start, end); vendor: name), then smallest `Span`.
- `TextExtractor` (unit struct, `Default`): scans `text` and stripped `html`, merges
  both. Stripping: drop tags, block/cell closers and `<br>` become whitespace, decode a
  fixed entity table (`&amp; &nbsp; &euro; &#N;`, …).
- Scanner:
  - Amount = digit groups with `.`, `,`, space/NBSP separators plus an adjacent (≤1
    space) `€`/`$`/`£` (EUR/USD/GBP) or 3-letter uppercase code; else not an amount.
    Decimal separator = the last of `.`/`,` when both occur, or a lone one followed by
    exactly 2 digits; otherwise grouping.
  - Amount pick: first amount ≤40 bytes after an anchor (`total`, `amount due`,
    `balance due`, `gesamtbetrag`, `total a pagar`, `importe total`, `montant total`;
    case-insensitive), high confidence; else the largest amount, low confidence.
  - Dates: `yyyy-mm-dd`, `dd.mm.yyyy`, `dd/mm/yyyy`, `d Mon yyyy`, `d Month yyyy`,
    `Month d, yyyy` (English months, case-insensitive).
  - `due` = first date ≤40 bytes after `due`, `pay by`, `fällig`, `vencimiento`,
    `échéance`; `issued` = first date after `invoice date`, `date`, `rechnungsdatum`,
    `fecha` that is not the due match; else the earliest non-due date, low confidence.
  - `vendor` = lowercased domain after `@` in `sender`, low confidence, span
    `Text 0..0`; `None` when absent or `Vendor::new` rejects it.
- `PROMOTES: extract` → rewrite its `docs/architecture.md` line; one `docs/decisions.md`
  line: `Extractor` takes `&Envelope`, no regex, structural tie-break.

## Test plan
Files: `tests/unit_extract.rs`, `tests/property_extract.rs`; fixtures in
`tests/fixtures/extract/`. Tests hand-build an `Envelope` (`sender`, `text` or `html`).
Confidence numbers are the implementer's; only their ordering is asserted.
- AC1 [unit] WHEN `de_total.txt` (`Gesamtbetrag: 1.234,56 €`, `Rechnungsdatum:
  24.09.2026`, `Fällig am 15.10.2026`; sender `rechnung@stadtwerke.de`) is extracted THE
  SYSTEM SHALL return amount `Money(123456, EUR)`, issued 2026-09-24, due 2026-10-15,
  vendor `stadtwerke.de`, `period == None`, each span slicing exactly the matched text.
- AC2 [unit] WHEN `us_total.txt` (`Amount due: $1,234.56`, `Invoice date: Sep 24, 2026`,
  `Due date: October 15, 2026`) is extracted THE SYSTEM SHALL return
  `Money(123456, USD)`, issued 2026-09-24, due 2026-10-15.
- AC3 [unit] WHEN `fr_space.txt` (`Montant total : 1 234,56 EUR`, NBSP grouping; `Date
  d'échéance : 15/10/2026`) is extracted THE SYSTEM SHALL return `Money(123456, EUR)`,
  due 2026-10-15, `issued == None`.
- AC4 [unit] WHEN `es_table.html` (`html` only; rows `Subtotal`, `IVA`, `Total a
  pagar</td><td>121,00&nbsp;€`; `Fecha de vencimiento: 2026-10-15`) is extracted THE
  SYSTEM SHALL return `Money(12100, EUR)` with `span.source == Html`, due 2026-10-15.
- AC5 [unit] WHEN `noise.txt` (three unanchored amounts) is extracted THE SYSTEM SHALL
  return the largest, at a confidence strictly below AC1's.
- AC6 [unit] WHEN the body has no amount or date THE SYSTEM SHALL return `Ok` with only
  `vendor` set; WHEN `sender` has no `@` THE SYSTEM SHALL return `vendor == None`.
- AC7 [unit] WHEN `Confidence::new(101)` THE SYSTEM SHALL return
  `Err(Error::InvalidConfidence(101))`; `new(0)` and `new(100)` succeed.
- AC8 [unit] WHEN `merge` gets two extractions whose `amount`s differ in confidence THE
  SYSTEM SHALL keep the higher one and every field only one of them sets.
- AC9 [property] FOR ALL `Vec<Extraction>` (arbitrary fields, confidences, spans) THE
  SYSTEM SHALL satisfy `merge(v) == merge(shuffle(v))`, `merge(vec![merge(v)]) ==
  merge(v)`, and `merge(v ++ v) == merge(v)`.

<!-- GATE 1: [ ] EARS, pub-only [ ] no integration needed [ ] no [unverified]
     [ ] non-goals [ ] PROMOTES [ ] ~15k context [ ] ≤800 words -->
