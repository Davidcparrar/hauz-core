# Spec: Gmail API source and `hauz fetch` (#42)

## Problem
`hauz fetch` ingests the Gmail messages under a label in-process (idempotent via the raw
hash); `--after`/`--before` bound the run, else it is a full backfill.

## Non-goals
- `hauz gmail-auth` (consent flow): #57; the token comes from env.
- No server poll (#43), Pub/Sub, mailbox writes, multi-user, cursor, retries.

## Assumptions
- [spike-verified] Token refresh form fields, error shapes, `messages.list` paging
  (`maxResults` ≤500), `format=raw` base64url: see spike findings.
- [spike-verified] `reqwest 0.13` (`default-features = false`, `json`, `rustls`, `form`,
  `query`) and dev-deps `axum`, `tokio` (`net`) add no crate to `Cargo.lock`.
- [spike-verified] `base64` is locked twice ⇒ private std-only base64url decoder.
- [unverified, not load-bearing] Revoked token ⇒ 400 `invalid_grant`; empty list omits
  `messages`; `raw` is byte-stable (**first real run: fetch twice, compare**).
- Design (Gate 2 amendment): no persisted cursor; the caller bounds a run by UTC days
  (`--after` inclusive, `--before` exclusive) sent as epoch seconds, because Gmail reads
  `YYYY/MM/DD` as Pacific midnight (Gmail API filtering guide).
- Design: per-message `Email`/`Extract` errors are recorded and the run continues; other
  errors abort, keeping stored bills.
- Design: access token cached until `expires_in`; label required.

## Architecture delta
PROMOTES: `mail`:
- `MessageId` (non-empty), `PageToken`, `Page { ids, next }`.
- `trait MailSource: Send + Sync { list(&self, query: &str, page: Option<&PageToken>) ->
  BoxFuture<Result<Page, Error>>; fetch_raw(&self, &MessageId) -> BoxFuture<Result<Vec<u8>,
  Error>> }`.
- `Credentials`, `GmailSource::new(creds)` / `with_endpoints(creds, token_url, api_base)`.
- `Config::from_env(get) -> Result<Option<Config>, Error>`: `HAUZ_GMAIL_CLIENT_ID` (absent
  ⇒ `None`), then `_CLIENT_SECRET`, `_REFRESH_TOKEN`, `_LABEL` required; optional
  `_TOKEN_URL`, `_API_BASE`; `source() -> GmailSource`.
- `DateRange::parse(after: Option<&str>, before: Option<&str>) -> Result<DateRange, Error>`
  (`YYYY-MM-DD`, `after < before`); `Config::query(&range)` =
  `label:<label>` [` after:<epoch>`][` before:<epoch>`], UTC midnights.
- `async fn fetch(&dyn MailSource, query, &dyn Extractor, &dyn BillStore) ->
  Result<Vec<Fetched { id, outcome: Result<Outcome, ingest::Error> }>, Error>`: all pages,
  then ingest in order.
- `#[non_exhaustive] Error { Config { variable }, Auth, Transport, Malformed,
  InvalidRange (each with a reason), Ingest { id, source } }`.

CLI: `hauz fetch [--db <path>] [--after <day>] [--before <day>]` (any order; bad range ⇒ exit 2) resolves LLM and Gmail config before opening the DB; absent Gmail
config ⇒ exit 1 naming `HAUZ_GMAIL_CLIENT_ID`. Prints `Created <id>` / `Duplicate <id>` /
`Failed <gmail id>: <error>` per message; exit 1 if any failed or the run aborted.

## Test plan
Network fake: axum on `127.0.0.1:0`.
- AC1 [unit] WHEN `GmailSource::list` is called THE SYSTEM SHALL POST the four token form
  fields, then GET with `Bearer <access_token>`, the given `q`, `maxResults=500` and any
  `pageToken`, returning ids and `next` (empty and `None` when both are omitted).
- AC2 [unit] WHEN `fetch_raw` gets padded or unpadded base64url (with `-`/`_`) THE SYSTEM
  SHALL return the exact bytes, with one token request across list + fetches.
- AC3 [unit] WHEN the token endpoint answers 400 `invalid_grant` or 401, or Gmail 401/403,
  THE SYSTEM SHALL return `Error::Auth`; other non-2xx ⇒ `Transport`; bad JSON or
  base64url ⇒ `Malformed`.
- AC4 [unit] WHEN `Config::from_env` sees no client id THE SYSTEM SHALL return `Ok(None)`;
  a missing secret, token or label ⇒ `Error::Config` naming it. `Debug` of `Config`,
  `Credentials`, `GmailSource` and every `Error` SHALL contain neither secret nor token.
- AC5 [integration] WHEN `fetch` runs over a two-page fake source (two bills, one
  sender-less message) with `TextExtractor` + `InMemoryStore` THE SYSTEM SHALL return
  `Created`, `Created`, `Err(Email)` in listing order and store both bills; a rerun SHALL
  return both as `Duplicate`.
- AC6 [integration] WHEN `fetch_raw` fails mid-run THE SYSTEM SHALL return that `Err`,
  keeping earlier bills.
- AC7 [e2e] WHEN `hauz fetch --db <tmp>` runs against a fake server serving two messages
  THE SYSTEM SHALL print `Created <hash-hex>` twice (rerun: `Duplicate`), exit 0.
- AC8 [e2e] WHEN `HAUZ_GMAIL_REFRESH_TOKEN` is unset THE SYSTEM SHALL exit 1 naming it,
  creating no DB file. WHEN the token endpoint answers `invalid_grant` it SHALL exit 1
  with an auth error on stderr that omits the refresh token.
- AC9 [unit] WHEN `DateRange::parse` gets valid days THE SYSTEM SHALL make
  `Config::query` append `after:`/`before:` UTC-midnight epochs for the bounds given
  (none ⇒ label only); a non-`YYYY-MM-DD` value or `after >= before` ⇒
  `Error::InvalidRange`.
- AC10 [e2e] WHEN `hauz fetch --after 2026-09-01 --before 2026-10-01` runs THE SYSTEM SHALL
  send `q=label:<label> after:1788220800 before:1790812800`; WHEN `--after` is malformed or
  not before `--before` it SHALL exit 2 creating no DB file.

<!-- GATE 1 CHECKLIST (Leader self-check, before labelling `approved`):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [x] no [unverified] assumption is load-bearing; spike questions answered or carried
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens (else split into two issues)
     [x] ≤800 words -->
