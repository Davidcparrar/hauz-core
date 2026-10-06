# Review log (#42)

## Cycle 1
VERDICT: APPROVE
- verify.sh ALL GREEN (rerun). AC1–AC4 `unit_mail.rs`, AC5–AC6 `integration_mail_fetch.rs`, AC7–AC8 cli `e2e_cli.rs`; each asserts its criterion; pub surface only; Google faked on 127.0.0.1:0.
- Cargo.lock: no new `[[package]]`; `reqwest` 0.13 declared per spec. Secrets redacted in `Debug`; error reasons hold status codes / URL-stripped reqwest errors; AC8 asserts stderr omits the refresh token.
- Error policy and token cache (until `expires_in` − 60s) match the spec; no unwrap/expect/panic in core; config resolves before `SqliteStore::open`.
- Non-blocking: `mail.rs:210` `reqwest::Client::new()` panics if the TLS backend fails to initialise (fallible builder would change `with_endpoints`' signature); `cli/main.rs:82` `process::exit(1)` skips dropping the store (harmless under WAL); base64url decoder is lenient on misplaced padding/trailing bits.
- Leader: architecture not yet updated for PROMOTES `mail` (done in the docs commit).

## Cycle 2 (Gate 2 amendment: `--after`/`--before`)
VERDICT: REJECT (code-defect)
- REQUIRED (Leader-owned, no code change): commit the `docs/architecture.md` delta for `DateRange`/`--after` (it was still uncommitted in the working tree at review time), together with the `decisions.md` and README edits. Reviewer: approve once committed, no further cycle.
- Code: verify.sh ALL GREEN. AC9 `ac9_date_range_bounds_the_query_and_rejects_bad_input` checks every bound combination with exact epochs 1788220800/1790812800, and rejects malformed values (`2026/09/01`, `2026-9-1`, `2026-02-30`), equal ranges and inverted ranges. AC10 `ac10_fetch_bounds_query_by_date_range_or_exits_2` asserts the exact `q` with the flags reversed, plus exit 2 with no DB file. The range is parsed before any config or DB work. No panics in core, no Cargo changes, AC1–AC8 intact.
- Non-blocking: a malformed `--before` is covered only at unit level (AC9).
- Resolution: docs committed in the following commit; per the reviewer's instruction this closes cycle 2 without a re-review.
