# Review log: #3 Email envelope: parse raw RFC 5322 message into body + Documents

## Cycle 1 — 2026-09-24
VERDICT: REJECT (code-defect: incomplete Architecture delta — Leader-owned `docs/` items, not implementer work)
verify.sh: `verify: ALL GREEN`

REQUIRED CHANGES
1. `docs/architecture.md` — spec declares `PROMOTES: email` but the branch touches no `docs/` file. Rewrite the `email` bullet to the shipped surface.
2. `docs/decisions.md` — append the `#3` line the spec requires (`MimeType` newtype over an enum, pub-field records).

NOTES
- `html_of` (`crates/core/src/email.rs:147`) is a faithful implementation, not an amendment: AC1 (`html == None` for a single `text/plain` part) is authoritative and mail-parser's `body_html(0)` synthesizes HTML from text, so the `is_content_type("text","html")` guard is the only way to honor the criterion. Only the parenthetical assumption in the spec is imprecise; no spec change needed.
- AC1–AC9 all present at the tagged level with the right names; fixtures genuinely exercise nested `multipart/mixed`, base64 PDF, quoted-printable CSV and a filename-less `image/png`; no new pub item outside the spec; no `unwrap`/`expect`/`panic`/indexing/`allow` in `src/`; `Error` is `#[non_exhaustive]`; dep is workspace-pinned and pre-approved; no files outside `crates/**` + `Cargo.lock` + the spec commit.
- Thin but non-blocking: `ac2_*`/`ac3_*` assert only `is_some()` for the bodies; exact body strings would pin "return both bodies".

## Cycle 2 — 2026-09-24 (after docs delta, commit 9562422)
VERDICT: APPROVE
verify.sh: `verify: ALL GREEN`

REQUIRED CHANGES
none

NOTES
- Both cycle-1 required changes satisfied: `docs/architecture.md` `email` bullet matches the shipped surface in `crates/core/src/email.rs` exactly (no item claimed that isn't shipped, none shipped that isn't described; 539 words); one `2026-09-24 #3` line appended to `docs/decisions.md` in file format.
- Only `docs/architecture.md`, `docs/decisions.md`, `features/3/review.md` changed since 23bca44; `crates/**` and `Cargo.lock` untouched, so cycle-1 findings on AC1–AC9, fixtures, error discipline and the workspace-pinned dep still hold.
- Carry-forward nit, non-blocking: `ac2_*`/`ac3_*` in `tests/unit_email.rs` assert only `is_some()` for the bodies.
