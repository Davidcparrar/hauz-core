# Review log (#40)

## Cycle 1
VERDICT: REJECT (code-defect)
- REQUIRED: `ac3_wrong_credentials_are_unauthorized` never sends a strict-prefix credential —
  `token.trim_end_matches(|_| true)` empties the token, duplicating the "no token" case. Use
  `&token[..token.len() - 1]`; optionally add `"Bearer "` as its own case.
- Accepted: AC7 is the existing `e2e_http.rs` suite passing with the token (no `ac7_*` fn).
- Non-blocking: AC4 covers POST only; AC1 checks only the GET `id`.
- verify.sh green; grammar, 401 shape, route_layer ordering, ApiToken, main.rs, constant-time compare all match the spec.

## Cycle 2
VERDICT: APPROVE
- Cycle-1 change resolved: real strict-prefix case plus a separate `"Bearer "` case, each asserting the fixed 401 on both routes and an empty store.
- No regression; verify.sh ALL GREEN. Non-blocking notes from cycle 1 carried (AC4 POST-only, AC1 GET checks `id`).
