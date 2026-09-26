# Review: Heuristic extractor over text and HTML bodies (#4)

## Cycle 1 — 2026-09-24, commit 43e6211
`verify.sh` exit 0 (fmt, clippy -D warnings, 57 tests incl. proptest).

Findings:
- `docs/architecture.md` / `docs/decisions.md` — code-defect (branch incomplete): spec
  requires `PROMOTES: extract` (rewrite the module line) and one decisions line; the
  diff touches no `docs/`. Leader-owned.
- `crates/core/src/extract.rs:322-339` — code-defect (minor): `anchored` takes an unused
  `end: impl Fn(&M) -> usize` parameter silenced with `let _ = end;`; drop it and the
  two call-site closures (`:237`, `:257`).

Verified clean: pub surface exactly the spec's delta; no `Cargo.toml`/dependency change;
`bill.rs`/`email.rs`/`store.rs` untouched; no panicking slicing (all `.get()` /
char-boundary-derived); AC1–AC8 unit, AC9 proptest, pub API only; merge keys faithful to
the newtypes so AC9's laws hold by construction.

VERDICT: REJECT code-defect
REQUIRED CHANGES:
1. `docs/architecture.md`: rewrite the `extract` bullet to the shipped interface.
2. `docs/decisions.md`: append the `2026-09-24 #4:` line.
3. `crates/core/src/extract.rs`: remove the unused `end` parameter and its callers.

## Cycle 2 — 2026-09-24, commit da53673
`verify.sh` exit 0 (fmt, clippy -D warnings, 57 tests incl. proptest, budgets).

Required change 1 satisfied (`docs/architecture.md:40-48`, extract bullet = shipped
interface, 603 words). Required change 2 satisfied (`docs/decisions.md:13`). Required
change 3 satisfied (`extract.rs:322-338`, `anchored` takes only `start`; call sites
updated; behaviour-neutral). Regression sweep clean: tests and fixtures byte-identical
since cycle 1; pub surface exactly the spec delta; no dependency or `Cargo.toml` change.

VERDICT: APPROVE
