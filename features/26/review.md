# Review: `LlmExtractor` behind `Escalate` with env wiring (#26)

## Cycle 1 — 2026-10-03, reviewer on 28fc27e (spec amended at ea1dea6)
VERDICT: REJECT (code-defect)
verify.sh: `verify: ALL GREEN`

- AC1 `ac1_sends_text_then_pdf_pages_then_text_layer_with_full_schema`; AC2
  `ac2_short_body_truncated_and_html_only_when_text_absent`,
  `ac2_native_delivery_sends_pdf_part_and_skips_rasterizer`; AC3
  `ac3_full_reply_maps_every_field_at_capped_confidence_no_notes`; AC4
  `ac4_invalid_currency_blank_vendor_and_null_fields_become_none`; AC5 five `ac5_*` fns
  (client error, malformed, unsupported, rasterizer, empty envelope) — all in
  `crates/core/tests/unit_llm.rs`; AC6 `ac6_escalate_with_llm_extractor_fills_vendor_and_period`
  in `integration_ingest.rs`; AC7 `ac7_missing_llm_api_key_env_var_exits_1_and_creates_no_db`
  and AC8 `ac8_llm_client_failure_degrades_to_needs_review_exit_0` in
  `crates/cli/tests/e2e_cli.rs`. Levels, files and assertions match the criteria.
- Code audit clean: no `unwrap`/`panic`/`#[allow]`, no own-type mocks, no `features/` path
  dependency, `Cargo.toml` adds only `serde_json = { workspace = true }`, keys redacted in
  every `Debug`, fence stripper and `max_pages` wiring correct.
- Blocking: the `PROMOTES` docs delta (`docs/architecture.md`, `docs/decisions.md`, README
  env section) was not yet on the branch (Leader's step 8, applied after this verdict).
- Non-blocking: `llm.rs` module doc still calls the extractor "a later feature (#26)";
  `schema_for!` re-runs per call; two `acN_*` families per test file across features.

REQUIRED CHANGES: docs delta (Leader) and the `llm.rs` module doc sentence (implementer).
