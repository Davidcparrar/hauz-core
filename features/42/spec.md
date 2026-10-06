# Spec: Gmail API source and `hauz fetch` (#42)

## Problem
`hauz fetch` downloads every Gmail message under a configured label and
runs `ingest` in-process: backfill and catch-up in one, idempotent via the raw hash.

## Non-goals
- `hauz gmail-auth` (consent flow): its own issue (spike §6); the token comes from env.
- No server poll (#43), Pub/Sub, mailbox writes, multi-user, cursor, retry/backoff.

## Assumptions
- [spike-verified] Token refresh = form POST of `client_id`, `client_secret`,
  `refresh_token`, `grant_type=refresh_token`. Bad client ⇒ 401; bad bearer ⇒ Gmail 401.
- [spike-verified] `messages.list`: `q`, `pageToken`, `maxResults` ≤500, which returns
  `messages[{id}]` and `nextPageToken`. `messages.get?format=raw` returns `{"raw": base64url}`.
- [spike-verified] `reqwest 0.13`, `default-features = false, features = ["json",
  "rustls"]` (+ `form`/`query`: `serde_urlencoded` is already locked) adds no crate to
  `Cargo.lock`, nor do dev-deps `axum`, `tokio` (`net`). `reqwest` is allowed (#42).
- [spike-verified] `base64` is locked twice ⇒ private std-only base64url decoder.
- [unverified, not load-bearing] Revoked token ⇒ 400 `invalid_grant` (any token-endpoint
  400/401 maps to `Auth` anyway). An empty list omits `messages` (`#[serde(default)]`).
  Gmail's `raw` is byte-stable across calls (tests serve chosen bytes; **the first real run
  must fetch twice and compare**).
- Design: no cursor; each run re-downloads, cheap for one person's label.
- Design: a per-message `ingest::Error::{Email, Extract}` (malformed input, decision #7)
  is recorded and the run continues. `Store`/`Bill` and source errors abort, and bills
  already stored stay stored.
- Design: access token cached per `GmailSource` until `expires_in`; label required.

## Architecture delta
PROMOTES: `mail`. New `core::mail`:
- `MessageId` (non-empty newtype), `PageToken`, `Page { ids, next: Option<PageToken> }`.
- `trait MailSource: Send + Sync { list(&self, query: &str, page: Option<&PageToken>) ->
  BoxFuture<Result<Page, Error>>; fetch_raw(&self, &MessageId) -> BoxFuture<Result<Vec<u8>,
  Error>> }`.
- `Credentials { client_id, client_secret, refresh_token }`, `GmailSource::new(creds)` /
  `with_endpoints(creds, token_url, api_base)`.
- `Config::from_env(get) -> Result<Option<Config>, Error>` reads `HAUZ_GMAIL_CLIENT_ID`
  (absent ⇒ `None`), then requires `_CLIENT_SECRET`, `_REFRESH_TOKEN` and `_LABEL`. The
  optional overrides are `_TOKEN_URL` and `_API_BASE`. `query()` = `label:<label>`
  verbatim, and `source()` builds a `GmailSource`.
- `async fn fetch(&dyn MailSource, query, &dyn Extractor, &dyn BillStore) ->
  Result<Vec<Fetched { id, outcome: Result<Outcome, ingest::Error> }>, Error>`: all pages
  (`maxResults=500`), then ingest in listing order.
- `#[non_exhaustive] Error { Config { variable }, Auth, Transport, Malformed (each with a
  reason), Ingest { id, source } }`.

`core` declares `reqwest` as above. CLI: `hauz fetch [--db <path>]`
resolves LLM and Gmail config before opening the DB. An absent Gmail config is exit 1
naming `HAUZ_GMAIL_CLIENT_ID`. It reuses `build_extractor` and prints `Created <id>` /
`Duplicate <id>` / `Failed <gmail id>: <error>` per message, then exits 0, or 1 if any
message failed or the run aborted.

## Test plan
Network fake: axum on `127.0.0.1:0` via `with_endpoints` (e2e: env overrides).
- AC1 [unit] WHEN `GmailSource::list` is called THE SYSTEM SHALL POST the four token form
  fields, then GET the list with `Bearer <access_token>`, the given `q`, `maxResults=500`
  and any `pageToken`, returning ids and `next` (empty ids and `None` when both are
  omitted).
- AC2 [unit] WHEN `fetch_raw` gets `raw` as padded or unpadded base64url (with `-`/`_`)
  THE SYSTEM SHALL return exactly the encoded bytes, with one token request across list +
  fetches.
- AC3 [unit] WHEN the token endpoint answers 400 `invalid_grant` or 401, or Gmail 401/403,
  THE SYSTEM SHALL return `Error::Auth`. Other non-2xx ⇒ `Transport`, and bad JSON or bad
  base64url ⇒ `Malformed`.
- AC4 [unit] WHEN `Config::from_env` sees no client id THE SYSTEM SHALL return `Ok(None)`,
  and a missing secret, token or label ⇒ `Error::Config` naming it. `Debug` of `Config`,
  `Credentials`, `GmailSource` and every `Error` SHALL contain neither secret nor token.
- AC5 [integration] WHEN `fetch` runs over a two-page fake source with two bills and one
  sender-less message, via `TextExtractor` + `InMemoryStore`, THE SYSTEM SHALL return one
  `Fetched` per id in listing order (`Created`, `Created`, `Err(Email)`) and store both
  bills. A rerun SHALL return both bills as `Duplicate`.
- AC6 [integration] WHEN `fetch_raw` fails mid-run THE SYSTEM SHALL return that `Err`,
  keeping bills ingested before it.
- AC7 [e2e] WHEN `hauz fetch --db <tmp>` runs against a fake server serving two messages
  THE SYSTEM SHALL print `Created <hash-hex>` twice, exit 0; a rerun, `Duplicate`.
- AC8 [e2e] WHEN `HAUZ_GMAIL_REFRESH_TOKEN` is unset THE SYSTEM SHALL exit 1 naming it,
  creating no DB file. WHEN the token endpoint answers `invalid_grant` it SHALL exit 1
  with an auth error on stderr that omits the refresh token.

<!-- GATE 1 CHECKLIST (Leader self-check, before labelling `approved`):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [x] no [unverified] assumption is load-bearing; spike questions answered or carried
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens (else split into two issues)
     [x] ≤800 words -->
