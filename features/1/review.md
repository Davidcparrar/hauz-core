# Review log: #1 Domain types

## Cycle 1 — 2026-09-21
VERDICT: REJECT (code-defect)
verify.sh: `verify: ALL GREEN`

REQUIRED CHANGES
1. `docs/architecture.md` `bill` line: spec says `PROMOTES: bill`; add `Currency` and `BillDraft` to the interface list.
2. `docs/decisions.md`: append the line for the `BillDraft` + `TryFrom`/`serde(try_from, into)` pattern.

Both are Leader-owned artifacts (step 8, docs delta); no `crates/**` change needed.

NOTES
- AC1–AC10 in `tests/unit_bill.rs`, AC11–AC12 in `tests/property_bill.rs`; pub API only, no mocks.
- Public surface matches the spec; `Error` is `#[non_exhaustive]`; newtypes deserialize via `try_from`; `Status` is snake_case.
- No unwrap/expect/panic in `crates/core/src`; deps limited to workspace serde, time, dev serde_json; root `Cargo.toml` untouched.
- `Money::new` infallible does not conflict with the spec: AC2 requires accepting any `i64`.

## Cycle 2 — 2026-09-21
VERDICT: APPROVE
verify.sh: `verify: ALL GREEN`

- `docs/architecture.md` `bill` line lists `BillDraft`, `Currency`, and the `TryFrom<BillDraft>`-only construction note (cycle-1 item 1 closed).
- `docs/decisions.md` has exactly one appended `2026-09-21 #1:` line (cycle-1 item 2 closed).
- Diff vs `origin/main` touches only `crates/core/**`, `docs/architecture.md`, `docs/decisions.md`, `features/1/**`, and `Cargo.lock` (mechanical consequence of adopting workspace-pinned deps).
- Crates code byte-identical to cycle 1; cycle-1 code findings stand. REQUIRED CHANGES: none.
