# Review log (#38)

## Cycle 1
VERDICT: APPROVE
- verify.sh ALL GREEN (rerun by reviewer); AC1–AC8 present at named level/file and asserting the criterion.
- No new `pub` item, no dependency change; ISO table private const in `bill.rs`, plausible for 2026 (BGN/ANG out, XCG/ZWG in; XXX/XTS/HRK/ZWL/SLL/XBA–XBD out).
- Call sites (`ubl.rs`, `extract.rs`, `llm.rs`) omit the amount on `Err`; `store.rs` maps to `Corrupt`.
- Non-blocking: `CUC` absent — confirm ISO status in a later change.
