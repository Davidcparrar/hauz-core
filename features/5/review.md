# Review: PDF attachment text extraction feeding the extractor (#5)
<!-- Appended by the Leader from the reviewer's return; one section per cycle. -->

## Cycle 1 — 2026-09-28, commit 5dbb220
`verify.sh` exit 0 (fmt, clippy -D warnings, doc budgets, spike-leakage check);
`cargo test -p hauz-core` 65 tests, 0 failed (`unit_extract` 14,
`integration_extract_email` 2, `property_extract` 1).

- AC1–AC6 → `ac1_…`–`ac6_…` in `tests/unit_extract.rs`; AC7–AC8 → `ac7_…`/`ac8_…` in
  `tests/integration_extract_email.rs` (real `Envelope::parse` + extract, no mocks); AC9
  → `ac9_merge_is_order_independent_idempotent_and_duplicate_safe` in
  `tests/property_extract.rs`, strategy includes `arb_notes()` = `btree_set(.., 0..3)`
  so non-empty note sets are generated and all three laws hold over them. No acceptance
  test inside `#[cfg(test)]`; tests touch only `pub` items.
- `catch_unwind(AssertUnwindSafe(..))` closure borrows only `&[u8]`; `panic_message`
  downcasts `&str`/`String` with a fixed fallback; no panic hook installed; nothing else
  on the path panics. `merge` unions `notes`, keeping #4's AC9 laws.
- No new `#[allow(..)]`; no `path = ".../features/..."` dep; `tests/common/mod.rs` is a
  rewrite of `spike/ref/src/lib.rs` (renamed, `pub(crate)`, no `unwrap`), not a copy.
- `crates/core/Cargo.toml` += `pdf-extract = { workspace = true }`; root `Cargo.toml`
  untouched (pre-pinned, not a new dependency). PROMOTES honoured: `docs/architecture.md`
  extract line updated, `docs/decisions.md` #5 added. Dependency direction intact.
- Non-blocking, no change requested: `text_layer` returns `Error::Pdf { document: 0 }`
  as a placeholder the caller overwrites (spec-prescribed); `unit_extract.rs` holds #4's
  `ac1..ac8` beside #5's `ac1..ac6`, distinguished only by suffix.

VERDICT: APPROVE
