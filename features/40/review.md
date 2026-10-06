# Review log (#40)

## Cycle 1
VERDICT: REJECT (code-defect)
- REQUIRED: `ac3_wrong_credentials_are_unauthorized` never sends a strict-prefix credential —
  `token.trim_end_matches(|_| true)` empties the token, duplicating the "no token" case. Use
  `&token[..token.len() - 1]`; optionally add `"Bearer "` as its own case.
- Accepted: AC7 is the existing `e2e_http.rs` suite passing with the token (no `ac7_*` fn).
- Non-blocking: AC4 covers POST only; AC1 checks only the GET `id`.
- verify.sh green; grammar, 401 shape, route_layer ordering, ApiToken, main.rs, constant-time compare all match the spec.
