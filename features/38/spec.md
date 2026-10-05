# Spec: bill: Currency accepts only ISO 4217 codes (#38)

## Problem
The 2026-10-04 corpus replay stored the Enel bill as `86006387500 NIT`: `NIT 860063875-0` is a
Colombian tax ID, and `Currency::new` accepts any three uppercase ASCII letters, so the
scanner took the digits beside `NIT` as money. Now `Currency::new` accepts only active ISO 4217
codes, so every extractor drops such a pair: the real `COP` total wins, or the amount is
absent and the bill is `needs_review`.

## Non-goals
- No currency inference, default or alias (`NIT` → `COP`, `$` → `COP`); an amount without a
  valid code stays absent.
- No migration or repair of stored rows; no skip-on-read.
- No new `pub` item: the code table stays private (no `Currency::all()`, no `ISO_4217` const).
- No minor-unit exponent per currency (JPY has 0 decimals): `Money` stays as it is.

## Assumptions
- no spike: a private table lookup in an existing constructor; all three call sites
  (`extract::find_amounts`, `extract::ubl`, `llm` mapping) already map `Err` from
  `Currency::new` to "omit the amount" (read in the source).
- Design call (table): a private `const` sorted slice of the active ISO 4217 alphabetic
  codes (list one, as of 2026) in `bill.rs`, with no dependency. It includes funds codes
  (`COU`, `CLF`, `BOV`, …) and the real `X` currencies (`XAF`, `XOF`, `XCD`, `XPF`) plus
  `XAU`/`XAG`/`XPT`/`XPD`/`XDR`/`XSU`/`XUA`. It **excludes** `XXX` (no currency) and `XTS`
  (testing), because neither can denominate a bill. Withdrawn codes (e.g. `HRK`, `ZWL`,
  `SLL`) are rejected.
- Design call (stored rows): a stored row whose currency is no longer valid makes `get`,
  `find_by_hash` and `list` return `store::Error::Corrupt { id, reason }`, the same contract
  already used for a row with an unknown status. It is not skipped. Skipping would hide a record;
  only throwaway replay DBs hold such rows today.
- Design call (adjacency): the scanner still prefers the marker *after* a numeral. If that
  code is invalid, the numeral is not an amount, and the scanner does not fall back to the
  marker before it. `1.000 NIT` is not money, whatever precedes it.
- `Error::InvalidCurrency`'s message becomes "currency must be an active ISO 4217 code".
- Test fallout: `property_bill.rs::arb_currency` samples a fixed list of valid codes (a
  `[A-Z]{3}` filter rejects ~99%); tests using made-up codes switch to real ones.

## Reference implementation
Omitted (no spike).

## Architecture delta
`bill::Currency::new` validates against a private ISO 4217 table. Its signature is unchanged,
and so is the shape of every type.
PROMOTES: bill. The meaning of the `Currency` contract changes, so `docs/architecture.md`
(the `bill` line, Planned, Risks) and `docs/decisions.md` are updated in this PR.

## Test plan
- AC1 [unit] WHEN `Currency::new` receives an active ISO 4217 code (`USD`, `EUR`, `GBP`, `COP`,
  `JPY`, `XOF`, `COU`) THE SYSTEM SHALL return `Ok`, with `as_str()` equal to the input.
  (`unit_bill.rs`, `ac1_accepts_active_iso_4217_codes`)
- AC2 [unit] WHEN `Currency::new` receives a well-shaped code that is not active ISO 4217
  (`NIT`, `ABC`, `ZZZ`, `HRK`, `XXX`, `XTS`), or a shape-invalid one (`usd`, `US`, `USDD`, `""`),
  THE SYSTEM SHALL return `Err(Error::InvalidCurrency)`, and deserializing `"NIT"` as
  `Currency` SHALL fail. (`unit_bill.rs`, `ac2_rejects_non_iso_codes`)
- AC3 [unit] WHEN `TextExtractor` reads a body containing `NIT 860063875-0` near a
  `Total a pagar COP 184.350` THE SYSTEM SHALL return an amount of `Money(18_435_000, COP)`
  and never a `NIT` amount; and WHEN the body's only numeral-plus-code pair is
  `NIT 860063875-0`, THE SYSTEM SHALL return `amount: None`. The fixture is synthetic, in
  `tests/fixtures/extract/`. (`unit_extract.rs`, `ac3_tax_id_is_not_an_amount`)
- AC4 [unit] WHEN `XmlInvoiceExtractor` reads a DIAN invoice whose `PayableAmount` has
  `currencyID="NIT"` THE SYSTEM SHALL omit `amount` and keep vendor, issued and period.
  (`unit_extract.rs`, `ac4_non_iso_currency_id_omits_amount`)
- AC5 [unit] WHEN `LlmExtractor` gets a reply with `"currency":"NIT"` and a non-null
  `amount_minor_units` THE SYSTEM SHALL return `amount: None` and keep the other fields.
  (`unit_llm.rs`, `ac5_non_iso_currency_omits_amount`)
- AC6 [integration] WHEN a `SqliteStore` row has `currency = 'NIT'` (written with raw SQL)
  THE SYSTEM SHALL return `Error::Corrupt` carrying that row's id from `get`, `find_by_hash`
  and `list`. (`integration_store.rs`, `ac6_non_iso_stored_currency_is_corrupt`)
- AC7 [integration] WHEN `ingest` runs the AC3 body as an email through `TextExtractor` into a
  store THE SYSTEM SHALL persist a bill whose amount is `Money(18_435_000, COP)`.
  (`integration_ingest.rs`, `ac7_tax_id_email_stores_cop_total`)
- AC8 [property] FOR ALL strings `s`, IF `Currency::new(s)` is `Ok(c)` THEN `s` is exactly
  3 ASCII uppercase letters, and `c` round-trips through serde_json unchanged.
  (`property_bill.rs`, `ac8_valid_currency_is_well_shaped_and_round_trips`)

<!-- GATE 1 CHECKLIST (Leader self-check, before labelling `approved`):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [x] no [unverified] assumption is load-bearing; spike questions answered or carried
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens (else split into two issues)
     [x] ≤800 words -->
