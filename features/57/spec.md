# Spec: cli: hauz gmail-auth loopback OAuth consent (#57)

## Problem
`hauz fetch` and the server poll need `HAUZ_GMAIL_REFRESH_TOKEN`, and nothing obtains one.
`hauz gmail-auth` runs Google's loopback consent for a "Desktop app" client. It prints an auth
URL, waits for the redirect on `127.0.0.1`, checks `state`, exchanges the code with PKCE, and
prints `HAUZ_GMAIL_REFRESH_TOKEN=<token>`.

## Non-goals
- No token storage, `.env` editing, multi-user, browser auto-open, timeout (Ctrl-C) or revoke.

## Assumptions
- no spike: spike #42 §1/§6 covered the token endpoint; live consent is unverifiable offline,
  so criteria use local fakes.
- [unverified, not load-bearing] Google returns `refresh_token` only with
  `access_type=offline&prompt=consent`. If it is absent, that is `Error::Malformed`, never success.
- Design call — **randomness**: read 64 bytes from `/dev/urandom` via `std::fs`. `getrandom`
  (locked ×3) would be a dependency decision; `std::random` is unstable. Unix-only, as is the deployment.
- Design call — **PKCE S256 always**: bytes 0..16 become `state` and 16..64 become `code_verifier`,
  each base64url without padding (22/64 chars). challenge = base64url(SHA-256(verifier)) via `sha2`.
- Design call — **listener**: tokio `TcpListener` on `127.0.0.1:0`, with
  `redirect_uri = http://127.0.0.1:<port>`. The handler reads the request line (≤8 KiB). A
  query without `code`/`error` gets 404 and the wait continues (favicon); the first with one
  ends it (200 success, else 400). `core` gains tokio `net` + `io-util` (already locked).
- Design call — **endpoints**: `HAUZ_GMAIL_AUTH_URL` (new) and `HAUZ_GMAIL_TOKEN_URL` override
  Google's `https://accounts.google.com/o/oauth2/v2/auth` and token URL; URLs use `reqwest::Url`.
- Design call — **output**: the URL goes to stderr and stdout carries only the env line, so
  `hauz gmail-auth >> .env` works.

## Architecture delta
PROMOTES: consent — new module `core::consent`:
- `pub struct Config` (`Debug` redacts the secret).
  `Config::from_env(get: impl Fn(&str) -> Option<String>) -> Result<Config, Error>` requires
  non-empty `HAUZ_GMAIL_CLIENT_ID` and `_CLIENT_SECRET`, and reads optional `_AUTH_URL` and
  `_TOKEN_URL`.
- `pub fn entropy() -> Result<[u8; 64], Error>`.
- `pub fn pkce_challenge(verifier: &str) -> String`.
- `pub async fn authorize(config: &Config, entropy: [u8; 64], show: impl FnOnce(&str)) ->
  Result<String, Error>`:
  1. Bind the listener and call `show(url)` once. The URL carries `client_id`, `redirect_uri`,
     `response_type=code`, `scope=https://www.googleapis.com/auth/gmail.readonly`,
     `access_type=offline`, `prompt=consent`, `state`, `code_challenge` and
     `code_challenge_method=S256`.
  2. Await the redirect.
  3. POST the form `grant_type=authorization_code`, `code`, `redirect_uri`, `client_id`,
     `client_secret` and `code_verifier`.
  4. Return `refresh_token`.
- `#[non_exhaustive] pub enum Error { Config { variable }, Io { reason }, StateMismatch,
  Denied { reason }, Auth { reason }, Transport { reason }, Malformed { reason } }`. Reasons
  never carry secrets, codes or tokens.

cli: `hauz gmail-auth` (no further tokens) joins the grammar/`USAGE`; `main`: config → `entropy` →
`authorize` (show = `eprintln!`) → print. Exit 1 on `Error`, 2 on usage.

## Test plan
`[unit]` tests: `unit_consent.rs`, real loopback socket (the transport is the criterion), axum
fake token endpoint; the "browser" is a `reqwest` GET to the shown URL's `redirect_uri`.
- AC1 [unit] WHEN `HAUZ_GMAIL_CLIENT_ID` or `_CLIENT_SECRET` is missing or empty THE SYSTEM
  SHALL return `Error::Config` naming it, even when `_REFRESH_TOKEN`/`_LABEL` are unset.
- AC2 [unit] WHEN given RFC 7636 Appendix B's verifier THE SYSTEM SHALL return
  `E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM`.
- AC3 [unit] WHEN `entropy()` is called twice THE SYSTEM SHALL return different arrays.
- AC4 [unit] WHEN the browser redirects with the matching `state` and a `code` THE SYSTEM SHALL
  have shown a URL carrying every parameter above (`redirect_uri` on `127.0.0.1`, nonzero port),
  POST the token form with `pkce_challenge(code_verifier) == code_challenge`, answer 200, and
  return the fake's `refresh_token`.
- AC5 [unit] WHEN a request without `code`/`error` arrives first THE SYSTEM SHALL answer 404 and
  still complete on the next valid redirect.
- AC6 [unit] WHEN the redirect's `state` differs THE SYSTEM SHALL return `Error::StateMismatch`,
  answer 400, and make no token request.
- AC7 [unit] WHEN the redirect carries `error=access_denied` THE SYSTEM SHALL return
  `Error::Denied { reason: "access_denied" }` and make no token request.
- AC8 [unit] WHEN the token endpoint answers 400 `invalid_grant` THE SYSTEM SHALL return
  `Error::Auth`, and WHEN it answers 200 without `refresh_token` THE SYSTEM SHALL return
  `Error::Malformed`.
- AC9 [e2e] WHEN `hauz gmail-auth` runs with client env and `_AUTH_URL`/`_TOKEN_URL` set to fakes,
  and the test reads the URL from stderr and performs the redirect, THE SYSTEM SHALL exit 0 with
  stdout exactly `HAUZ_GMAIL_REFRESH_TOKEN=<fake token>\n`.
- AC10 [e2e] WHEN `HAUZ_GMAIL_CLIENT_SECRET` is unset THE SYSTEM SHALL exit 1 with stderr naming
  it and empty stdout, and WHEN `hauz gmail-auth extra` runs THE SYSTEM SHALL exit 2.

<!-- GATE 1 CHECKLIST (Leader self-check, before labelling `approved`):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [x] no [unverified] assumption is load-bearing; spike questions answered or carried
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens (else split into two issues)
     [x] ≤800 words -->
