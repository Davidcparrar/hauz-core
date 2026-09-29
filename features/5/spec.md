# Spec: PDF attachment text extraction feeding the extractor (#5)

## Problem
Bills usually arrive as a PDF attachment with an empty body. Adds `PdfTextExtractor`
to `extract`: for every `Document` whose mime is `application/pdf` it pulls the text
layer with `pdf-extract` and runs the existing heuristic scanner over it, so amount and
dates found inside the attachment reach `Extraction` with a `Document(i)` span. An
image-only PDF is honest, not silent: the extraction carries a `NoTextLayer` note.

## Non-goals
- No OCR, no vision model, no LLM; scanned PDFs only get the note.
- No body scanning: `text`/`html` stay `TextExtractor`'s job; callers `merge` the two.
- No mime sniffing (`%PDF-` magic on `application/octet-stream`), no encrypted PDFs
  (password-protected ⇒ `Error::Pdf`), no page/layout awareness, no new anchors.
- No vendor from PDF text; `vendor` and `period` stay `None`.
- No partial results: one corrupt PDF fails the whole `extract` call.

## Assumptions
- [spike-verified] Bad header, empty input, truncation and a corrupt `startxref` give
  `Err`; a well-formed PDF whose content stream uses a font missing from `/Resources`
  PANICS inside `pdf-extract` (`Option::expect`). `catch_unwind` is mandatory.
- [spike-verified] A std-only `minimal_pdf(lines)` (Helvetica base-14, one `Tj` per
  line, computed xref) extracts to `"\n\n<line1>\n<line2>"`: no inner spaces added,
  lines joined by `\n`. Base-14 fonts cannot encode `€`; fixtures use ASCII `EUR`.
- [spike-verified] Empty content, vector-only and image-only pages all return `Ok("")`;
  `trim().is_empty()` is the "no text layer" test. A slightly wrong xref offset also
  yields `Ok("")` (silent, not an error): accepted limitation.
- [spike-verified] `pdf-extract = "0.12"` builds clean on native Linux.

## Reference implementation
`features/5/spike/ref/src/lib.rs`: `minimal_pdf`, `image_only_pdf`,
`pdf_missing_font_resource` and their xref assembly. Illustrative only.

## Architecture delta
`crates/core/Cargo.toml` += `pdf-extract = { workspace = true }` (already pinned; no
root change, no new decision). Module `extract`:
- `Error` += `Pdf { document: usize, reason: String }` (`pdf-extract`'s error is not
  `Clone`/`Eq`, so its `Display` text is carried; a caught panic's message likewise).
- New `#[non_exhaustive] pub enum Note { NoTextLayer { document: usize } }` (`Copy`,
  `Ord`, `Hash`).
- `Extraction` += `pub notes: BTreeSet<Note>`; `merge` unions the sets (keeps merge
  order-insensitive and idempotent; #4's AC9 laws still hold).
- `PdfTextExtractor` (unit struct, `Default`, `Copy`), `impl Extractor`: for each
  `(i, doc)` in `documents` with `doc.mime == application/pdf`: `text_layer(&doc.bytes)`
  ⇒ `Err` ⇒ return `Error::Pdf { document: i, .. }`; `None` ⇒ insert
  `NoTextLayer { document: i }`; `Some(text)` ⇒ scan it (same scanner, anchors and
  confidences as `TextExtractor`) with `Source::Document(i)`. Merge all parts. Other
  mimes and an empty `documents` ⇒ `Ok(Extraction::default())`.
- `PdfTextExtractor::text_layer(bytes: &[u8]) -> Result<Option<String>, Error>`:
  runs `pdf_extract::extract_text_from_mem` inside
  `std::panic::catch_unwind(AssertUnwindSafe(..))`; a panic or `Err` ⇒ `Error::Pdf`
  (`document` filled by the caller); `Ok(None)` when the text trims to empty. Spans
  index into the returned string.
- `PROMOTES: extract` → update its `docs/architecture.md` line; one `docs/decisions.md`
  line: notes as a set on `Extraction`, `text_layer` wrapped in `catch_unwind`, one
  corrupt PDF fails the call.

## Test plan
Files: `tests/unit_extract.rs` (append), `tests/integration_extract_email.rs`,
`tests/property_extract.rs` (extend the `Extraction` strategy with `notes`). Fixture
builders in `tests/common/mod.rs`, std only, per the reference recipe:
`minimal_pdf(&[&str])`, `image_only_pdf()`, `pdf_missing_font(&[&str])`. Tests
hand-build an `Envelope` with `documents`. Confidence numbers are the implementer's.
- AC1 [unit] WHEN a PDF from `minimal_pdf(&["Total: 1,234.56 EUR", "Due date:
  15/10/2026"])` is document 0 THE SYSTEM SHALL return amount `Money(123456, EUR)`, due
  2026-10-15, both with `span.source == Document(0)` slicing exactly the matched text of
  `text_layer(bytes)`; `notes` empty; `vendor == None`.
- AC2 [unit] WHEN document 0 is `image_only_pdf()` THE SYSTEM SHALL return `Ok` with
  every field `None` and `notes == {NoTextLayer { document: 0 }}`; `text_layer` SHALL
  return `Ok(None)`.
- AC3 [unit] WHEN document 0 is `b"%PDF-1.4 fake"`, an empty slice, the AC1 PDF cut to
  half its length, or `pdf_missing_font(..)` THE SYSTEM SHALL return
  `Err(Error::Pdf { document: 0, .. })` in every case; no panic escapes.
- AC4 [unit] WHEN documents are `[text/csv, image-only PDF, text PDF]` THE SYSTEM SHALL
  ignore the csv, note `NoTextLayer { document: 1 }`, and return AC1's fields with
  `Document(2)` spans.
- AC5 [unit] WHEN `documents` is empty, or holds only non-PDF mimes THE SYSTEM SHALL
  return `Ok(Extraction::default())`.
- AC6 [unit] WHEN `merge` gets extractions with notes `{A}` and `{A, B}` THE SYSTEM
  SHALL return notes `{A, B}`.
- AC7 [integration] WHEN `html_pdf.eml` is `Envelope::parse`d and given to
  `PdfTextExtractor` THE SYSTEM SHALL return `Err(Error::Pdf { document: 0, .. })`.
- AC8 [integration] WHEN a 7-bit MIME message built at test time around
  `minimal_pdf(..)` (AC1 lines) is parsed and extracted THE SYSTEM SHALL return AC1's
  amount and due date.
- AC9 [property] FOR ALL `Vec<Extraction>` (fields, confidences, spans, notes) THE
  SYSTEM SHALL satisfy `merge(v) == merge(shuffle(v))`, `merge(vec![merge(v)]) ==
  merge(v)`, `merge(v ++ v) == merge(v)`.

<!-- GATE 1: [ ] EARS, pub-only [ ] integration present (email→extract) [ ] no
     [unverified] load-bearing [ ] non-goals [ ] PROMOTES [ ] ~15k context [ ] ≤800 words -->
