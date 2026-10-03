# Spec: extract: a bare `$` must not be read as USD (#28)

## Problem
`TextExtractor` maps every `$` to `USD`. Four of the seven corpus bills are Colombian, so the
stored currency is wrong while the amount still looks understood, and the wrong heuristic
amount at confidence 90 beats the model's correct `COP` amount capped at 70. After this
feature a bare `$` is not a currency marker: the numeral next to it is not an amount, the
bill lands `NeedsReview` with `amount` `None`, and an explicit code glued to the sign
(`COP$`, `USD$`, `US$`) or standing beside the numeral (`$ 12 USD`) still resolves.

## Non-goals
- No locale inference from the sender's TLD or language; no configurable default currency
  (issue options b/c, follow-ups if the corpus still needs them).
- No change to `€`/`£`, the 3-letter-code rule, anchors, confidences, `merge`, `Escalate`,
  `ingest`'s status rule, `LlmExtractor`, routes, CLI.
- No new fixtures from the private corpus; synthetic text only.

## Assumptions
- no spike: the change is confined to the private currency-marker helpers in
  `crates/core/src/extract.rs` (`symbol_currency`, `match_currency_before/after`); the
  parsing of `1.234.567` as 123 456 700 minor units is the existing documented rule.
- Design call: issue option (a) plus the glued-code form. `<CODE>$` where `CODE` is three
  ASCII uppercase letters means `CODE`; `US$` means `USD`; a `$` in any other position is
  skipped as if it were not there, so the numeral is kept only when another marker is
  adjacent. The match span covers the code and the sign.
- Design call: `us_total.txt` changes its amount line to `Amount due: US$1,234.56`; the
  existing `ac2_us_text_anchored_fields` keeps asserting `USD` and its dates. Server and CLI
  `bill.eml` already say `EUR` and are untouched.
- Design call: `TextExtractor` doc comment and `docs/architecture.md` (the `extract` module
  line and the Risks entry naming #28) are updated on the branch; no `PROMOTES` because no
  `pub` signature changes.

## Architecture delta
- `extract` (private): `symbol_currency` drops `'$'`; the before/after matchers learn
  `<CODE>$` / `US$`. No `pub` change, no dependency change, no entry-point change.
- Docs: `docs/architecture.md` `extract` line and Risks; one `docs/decisions.md` line
  (Leader, step 8). `PROMOTES`: none.

## Test plan
Files: `crates/core/tests/unit_extract.rs` (AC1–AC3), `integration_ingest.rs` (AC4–AC5).
New fixture `crates/core/tests/fixtures/extract/co_bare_dollar.txt`:
```
Factura de servicios
Total a pagar: $ 1.234.567
Fecha de vencimiento: 15/10/2026
```
New fixture `crates/core/tests/fixtures/co_bare_dollar.eml`: the `bill.eml` headers
(sender `facturacion@acme-energia.example`, `text/plain`, 7bit) with that body. No e2e: the
entry points are not touched and their fixtures already use `EUR`.
- AC1 [unit] WHEN `TextExtractor` scans `co_bare_dollar.txt` THE SYSTEM SHALL return
  `amount` `None`, `due` 2026-10-15 at confidence 90 and vendor `acme-energia.example`.
- AC2 [unit] WHEN the amount line reads `Total a pagar: COP$ 1.234.567` THE SYSTEM SHALL
  return 123 456 700 `COP` at confidence 90 with a span that starts at the `C`; WHEN it
  reads `Amount due: US$1,234.56` THE SYSTEM SHALL return 123 456 `USD`; WHEN it reads
  `Total: $ 1,234.56 USD` THE SYSTEM SHALL return 123 456 `USD`.
- AC3 [unit] WHEN the text is `Total: $ 999.00\nReference from a previous statement, not a
  charge: 12.00 EUR` THE SYSTEM SHALL return 1 200 `EUR` at confidence 40 (the bare-`$`
  numeral is not a candidate, the `EUR` one lies past the 40-byte anchor window, so the
  unanchored fallback wins).
- AC4 [integration] WHEN `ingest` runs `Chain([TextExtractor, PdfTextExtractor])` over
  `co_bare_dollar.eml` into a `SqliteStore` THE SYSTEM SHALL store `NeedsReview` with
  `amount` `None`, `due` 2026-10-15 and vendor `acme-energia.example`.
- AC5 [integration] WHEN `ingest` runs `Escalate(Chain([TextExtractor,
  PdfTextExtractor]), LlmExtractor(fake replying vendor `Acme Energía`, 123 456 700 `COP`,
  period 2026-09-01..30, due 2026-10-15, confidence 100), 50)` over `co_bare_dollar.eml`
  THE SYSTEM SHALL store `Extracted` with amount 123 456 700 `COP` (the model's amount no
  longer loses to a wrong heuristic one).

<!-- GATE 1: all seven boxes ticked (no e2e: entry points untouched; integration present
     because the stored outcome crosses extract → ingest → store) -->
