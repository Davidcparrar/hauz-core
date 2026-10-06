# Spec: server: bearer-token auth on every /v1 route (#40)
<!-- ≤800 words, verify-enforced. Drafted and self-checked by the Leader (Gate 1); the human
     reads it in the PR (Gate 2). Design calls go under Assumptions so they are visible there. -->

## Problem
Every `/v1` route answers anyone today. Before deploy each must require
`Authorization: Bearer <token>` (token from `HAUZ_API_TOKEN`): missing or wrong ⇒ a fixed
401 before the body is read; no token configured ⇒ the server does not start.

## Non-goals
- No users, sessions, SSO, token rotation or multiple tokens. No rate limiting, lockout,
  HMAC signing or request logging.
- CLI and TUI are unchanged (local, no HTTP).
- No new dependency. No TLS (deploy concern).

## Assumptions
- no spike: axum 0.8 `middleware::from_fn_with_state` via `route_layer` runs before
  handler extractors (so before `Bytes`); AC5 pins that ordering.
- [design] New pub `hauz_server::ApiToken`: `ApiToken::new(impl Into<String>) ->
  Option<ApiToken>`, `None` when the token is empty or all whitespace (an empty token would
  be a silently open server). `Debug` redacted; no `Display`, no accessor.
- [design] `AppState::new(store, extractor, token: ApiToken)`: no unauthenticated
  `AppState` can be built.
- [design] Header grammar: exactly `<scheme> <token>`. The scheme is `Bearer` matched
  ASCII-case-insensitively (RFC 7235), followed by a single space, and the rest is compared
  byte-for-byte with the configured token. Any other shape gets 401: another scheme, a
  missing token, a non-UTF-8 header, or more than one `Authorization` header.
- [design] 401 response: body `{"error":"unauthorized"}` (the existing `ErrorBody` shape)
  plus `WWW-Authenticate: Bearer` (RFC 6750). Identical for missing and wrong tokens.
- [design] Constant time: the compare XOR-folds every byte when the lengths are equal and
  returns false at once when they differ. Only the length leaks (fine for a long random
  secret). Private helper: the reviewer checks it by reading; no criterion covers it.
- [design] The layer is a `route_layer`, so only matched `/v1` routes are guarded. An
  unknown path stays 404 (no data exposed); later `/v1` routes inherit the guard.
- [design] `main.rs` reads `HAUZ_API_TOKEN` before opening the DB. Unset or blank ⇒ abort
  with an error naming the variable. Untested by design (constitution › Shape).
- [design] `tests/common` builds every `AppState` with a fixture token; `e2e_http.rs`
  helpers send it.

## Architecture delta
- `crates/server/src/lib.rs`: pub `ApiToken`; `AppState` gains a `token` field and
  `new`'s third parameter; a private `require_bearer` middleware on the `/v1` routes,
  installed with `route_layer`.
- `crates/server/src/main.rs`: reads `HAUZ_API_TOKEN` and refuses to start without it.
- Core untouched; no dependency changes.
- PROMOTES: server (`AppState::new` signature, new pub `ApiToken`). The Leader updates
  `docs/architecture.md` › Entry points and `docs/decisions.md`, and the README env table.

## Test plan
All in `crates/server/tests/e2e_auth.rs` (in-process `oneshot` against `router(state)`,
fixture token from `tests/common`), except where noted.
- AC1 [e2e] WHEN `POST /v1/ingest/email` carries `Authorization: Bearer <configured token>`
  and a valid bill THE SYSTEM SHALL answer 201 with `{"id"}` and store the bill, and WHEN
  `GET /v1/bills/{that id}` carries the same header THE SYSTEM SHALL answer 200 with the
  bill JSON.
- AC2 [e2e] WHEN either route is called with no `Authorization` header THE SYSTEM SHALL
  answer 401 with body exactly `{"error":"unauthorized"}` and header
  `WWW-Authenticate: Bearer`, and (for the POST of a valid bill) the store SHALL stay empty.
- AC3 [e2e] WHEN either route carries a wrong credential THE SYSTEM SHALL answer the same
  401 as AC2 and store nothing. The wrong credentials are: a same-length token differing
  in one byte, a strict prefix of the token, the token plus one extra byte, `Basic <token>`,
  `Bearer` with no token, and `Bearer  <token>` (two spaces).
- AC4 [e2e] WHEN the scheme is written `bearer` (lowercase) with the correct token THE
  SYSTEM SHALL accept the request as in AC1.
- AC5 [e2e] WHEN an unauthenticated `POST /v1/ingest/email` has a body of
  `MAX_BODY_BYTES + 1` bytes THE SYSTEM SHALL answer 401, not 413 (auth is checked before
  the body is read). The same body *with* the token still answers 413.
- AC6 [e2e] WHEN `ApiToken::new` is given `""` or `"  \t"` THE SYSTEM SHALL return `None`,
  and WHEN it is given a non-blank token THE SYSTEM SHALL return `Some` whose `{:?}` does
  not contain the token text.
- AC7 [e2e] (regression, `e2e_http.rs`) WHEN the existing `e2e_http.rs` criteria run with
  the fixture token THE SYSTEM SHALL keep passing unchanged in status and body.

<!-- GATE 1 CHECKLIST (Leader self-check, before labelling `approved`):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [x] no [unverified] assumption is load-bearing; spike questions answered or carried
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens (else split into two issues)
     [x] ≤800 words -->
