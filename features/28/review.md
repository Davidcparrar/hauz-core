# Review: a bare `$` must not be read as USD (#28)

## Cycle 1 — 2026-10-03, reviewer on fedc627
VERDICT: APPROVE
verify.sh: `verify: ALL GREEN`

- AC1 `ac1_bare_dollar_numeral_is_not_an_amount`, AC2
  `ac2_glued_or_adjacent_currency_code_resolves` (all three clauses, `COP$` span starts at
  the `C`), AC3 `ac3_bare_dollar_skipped_unanchored_fallback_wins` in
  `crates/core/tests/unit_extract.rs`; AC4 `ac4_bare_dollar_bill_is_needs_review_with_no_amount`
  and AC5 `ac5_bare_dollar_bill_escalates_to_model_amount` in `integration_ingest.rs`
  (real `SqliteStore`, `Chain` / `Escalate(.., 50)`). Values, confidences and statuses match.
- `us_total.txt` changes only `$1,234.56` → `US$1,234.56`; `ac2_us_text_anchored_fields`
  untouched. No `unwrap`/`panic`/`#[allow]`, no new `pub` item, no dependency change, fakes
  only at the LLM/rasterizer edges. Docs delta matches the spec.
- Non-blocking: `match_currency_before`'s bare-code path duplicates the new `boundary_before`
  helper; `COP $ 1.234` (space before the sign) does not resolve — by the ≤1-space rule, check
  against the corpus; `integration_ingest.rs` now carries `ac4_*`/`ac5_*` from two specs.
