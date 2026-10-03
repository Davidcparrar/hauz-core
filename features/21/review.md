# Review: async `Extractor` and `Escalate` combinator (#21)

## Cycle 1 — 2026-10-03, reviewer on 2abddb3
VERDICT: APPROVE
verify.sh: `verify: ALL GREEN`

- AC1–AC4 are `ac1_is_complete_at_threshold_below_and_missing_fields`,
  `ac2_escalate_returns_complete_primary_without_calling_secondary`,
  `ac3_escalate_calls_secondary_once_and_merges_when_primary_incomplete`,
  `ac4_escalate_propagates_primary_err_without_calling_secondary` and
  `ac4_escalate_propagates_secondary_err_when_primary_incomplete` in
  `crates/core/tests/unit_extract.rs`; AC5 is
  `ac5_escalate_fills_missing_period_text_extractor_alone_needs_review` in
  `integration_ingest.rs` (real `SqliteStore`, both branches); AC6 is
  `ac6_escalate_returns_primary_when_complete_else_merge` under `proptest!` in
  `property_extract.rs`. Levels, files and assertions match the criteria.
- Existing tests kept every assertion; only `#[test]` → `#[tokio::test]` and `.await`
  changed in `unit_extract.rs`, `integration_extract_email.rs`, `unit_ingest.rs`,
  `property_ingest.rs`.
- Library code adds no `unwrap`/`expect`/`panic`/indexing and no `#[allow]`. `Extractor`
  stays `Send + Sync` and `dyn`-safe with a `Send` `BoxFuture`; `Escalate` returns the
  primary unchanged when complete, else `merge(vec![primary, secondary])`, and `?`
  propagates either `Err`. `ingest` delegates to the single `is_complete` rule.
- No dependency or `Cargo.toml` change; the implementation commit touches only
  `crates/core/**`; `BoxFuture` lives in `lib.rs` with the `store` re-export as specced.
- PROMOTES honoured on disk: `docs/architecture.md` and `docs/decisions.md` carry the delta.
- Fakes (`Fixed`, `Counting`, `Failing`) implement the public trait at the injection seam.

REQUIRED CHANGES: none.
