# Review: Ingest pipeline (#6)

## Cycle 1 — 2026-09-30
VERDICT: APPROVE

`verify: ALL GREEN` (exit 0)

- Pub surface of `ingest` matches the Architecture delta exactly: `EXTRACTED_MIN_CONFIDENCE`,
  4-variant `#[non_exhaustive] Error`, `Outcome`, `raw_hash`, `ingest`; `to_hex` is private.
  Step order hash → `find_by_hash` short-circuit → `Envelope::parse` → `ex.extract` →
  id/draft/status → `insert` as specified (`crates/core/src/ingest.rs:75-108`).
- Status rule is literal: amount present ∧ confidence ≥ 50 ∧ vendor ∧ period, else
  `NeedsReview` with every present field mapped through (`ingest.rs:85-103`).
- AC1–AC4 in `tests/integration_ingest.rs` (real `TextExtractor` + `SqliteStore` on
  `TmpDbFile`), AC5–AC10 in `tests/unit_ingest.rs` (`InMemoryStore`, test-local
  `Fixed`/`Failing`), AC11 in `tests/property_ingest.rs`: one `acN_*` fn each at the tagged
  level, asserting no weaker than the criterion.
- No unwrap/expect/panic, no `#[allow]` in library code; only `sha2 = { workspace = true }`
  added; nothing outside `crates/**` + `Cargo.lock` touched.

Leader follow-ups: append the `#6` decisions line (PROMOTES). The `InsertOutcome::Duplicate`
concurrent-writer branch has no criterion (fine against this spec; note for #7).

Minor, non-blocking: the `arb_*` strategy block in `property_ingest.rs` duplicates
`property_extract.rs`; candidate for `tests/common/mod.rs` later.
