# Spec: axum server: POST /v1/ingest/email and GET /v1/bills/{id} (#7)

## Problem
First entry point: `crates/server`, a lib exposing `pub fn router(state: AppState) ->
axum::Router` plus a thin `main.rs`. `POST /v1/ingest/email` takes the raw RFC 5322 bytes,
runs `ingest`, answers 201 (created) or 200 (duplicate) with the bill id;
`GET /v1/bills/{id}` returns the `Bill` as JSON or 404. The extractor composition deferred
from #6 lands in `core::extract` as `Chain`, so the server stays a shell.

## Non-goals
- No `Content-Type` enforcement (no 415): the body bytes are the message.
- No auth, TLS, CORS, rate limit, graceful shutdown, request logging or `tracing` (a
  dependency decision of its own).
- No list/search/delete endpoint; no raw-message archival to S3; no CLI (its own issue).
- No change to `bill`, `email`, `store`, `ingest`; nothing new is persisted.

## Assumptions
- no spike: axum 0.8 + `oneshot` are the constitution's path; core's pub surface settles
  the rest.
- axum 0.8: path params are `{id}`; `DefaultBodyLimit::max(n)` makes the `Bytes`
  extractor reject a larger body with 413.
- `Bill` serialises as `BillDraft` (snake_case fields) via `#[serde(into)]`; no DTO.
- Amended after Gate 1: AC9's fixture was `html_pdf.eml`, whose PDF is #5's deliberately
  corrupt one, so the merge equality could not hold; now a readable `minimal_pdf`.
- `DATABASE_URL` is a SQLite file path (a leading `sqlite://` is stripped); `BIND_ADDR`
  defaults to `127.0.0.1:8080`. `main.rs` is untested by design (constitution › Shape).
- All deps are workspace-pinned; root `Cargo.toml` is untouched (`members = ["crates/*"]`).
- Storage is a true system edge behind the injected `BillStore` trait, so a test-local
  always-`Err` store is a legitimate fake for the 500 path.

## Architecture delta
`core::extract` += `pub struct Chain(Vec<Box<dyn Extractor>>)` with `Chain::new(..)`,
`impl Extractor`: runs each extractor in order, propagates the first `Err`, returns
`merge(results)`. `Debug` implemented by hand (count of extractors).

New crate `crates/server` (`hauz-server`), `[lints] workspace = true`. Deps
(`workspace = true`): `hauz-core` (path), axum, tokio (+ `net` feature, crate-local),
serde, anyhow. Dev: tower, http-body-util, serde_json. Lib pub surface:
- `pub const MAX_BODY_BYTES: usize = 25 * 1024 * 1024`.
- `#[derive(Clone)] pub struct AppState { .. }`, `AppState::new(store: Arc<dyn
  BillStore>, extractor: Arc<dyn Extractor>) -> Self`; manual `Debug`.
- `pub fn router(state: AppState) -> Router` with `DefaultBodyLimit::max(MAX_BODY_BYTES)`:
  - `POST /v1/ingest/email`: body `Bytes` → `ingest(&body, &*extractor, &*store)`.
    `Created(id)` ⇒ 201, `Duplicate(id)` ⇒ 200, both `{"id":"<hex>"}`.
    `Error::Email | Error::Extract` ⇒ 400 `{"error":"<Display>"}`; any other variant
    (`Store`, `Bill`, future) ⇒ 500 `{"error":"internal error"}` (nothing leaked).
  - `GET /v1/bills/{id}`: `BillId::new` fails or `store.get` is `None` ⇒ 404
    `{"error":"not found"}`; `Some(bill)` ⇒ 200 `Json(bill)`; store `Err` ⇒ 500 as above.
- `main.rs`: `fn main() -> anyhow::Result<()>` (`#[tokio::main]`), reads `DATABASE_URL`
  (required) and `BIND_ADDR`, opens `SqliteStore`, builds `Chain([TextExtractor,
  PdfTextExtractor])`, `axum::serve` on a `TcpListener`.
- `PROMOTES: extract, server` → `docs/architecture.md`: `Chain` on the `extract` line,
  `AppState` + status codes on the server entry-point line; one `docs/decisions.md` line.

## Test plan
Files: `crates/server/tests/e2e_http.rs` (+ `tests/common/mod.rs`: `tmp_db_path()`,
`app(store) -> (Router, Arc<SqliteStore>)` with `Chain([TextExtractor, PdfTextExtractor])`,
`FailingStore`; `tests/fixtures/bill.eml` = #6's `bill_eml()` bytes, `malformed.eml`
copied from core), `crates/core/tests/unit_extract.rs`, `integration_extract_email.rs`.
Requests via `oneshot`; bodies read with `http_body_util::BodyExt::collect`.
- AC1 [e2e] WHEN `bill.eml` is POSTed to `/v1/ingest/email` on an empty store THE SYSTEM
  SHALL answer 201 with `{"id": h}` where `h` = lowercase hex of `raw_hash(bytes)`, and
  `GET /v1/bills/{h}` SHALL answer 200 with a body deserialising to a `Bill` of id `h`,
  `NeedsReview`, amount `Money(123456, EUR)`, due 2026-10-15, vendor `acme-power.example`.
- AC2 [e2e] WHEN the same bytes are POSTed again THE SYSTEM SHALL answer 200 with AC1's
  id and `store.list()` SHALL have length 1.
- AC3 [e2e] WHEN `malformed.eml` is POSTed THE SYSTEM SHALL answer 400 with a non-empty
  `error` field and `store.list()` SHALL be empty.
- AC4 [e2e] WHEN `GET /v1/bills/{id}` names a well-formed id that is not stored, or a
  string `BillId::new` rejects, THE SYSTEM SHALL answer 404 in both cases.
- AC5 [e2e] WHEN the state holds `FailingStore` THE SYSTEM SHALL answer 500 with
  `{"error":"internal error"}` for both a POST of `bill.eml` and a GET.
- AC6 [e2e] WHEN the POST body is `MAX_BODY_BYTES + 1` bytes THE SYSTEM SHALL answer 413.
- AC7 [unit] WHEN `Chain` of two `Fixed` extractors (one yields amount at 60, the other
  vendor and amount at 40) extracts THE SYSTEM SHALL return `merge(vec![a, b])`.
- AC8 [unit] WHEN any extractor in a `Chain` returns `Err` THE SYSTEM SHALL return that
  `Err`.
- AC9 [integration] WHEN `Chain([TextExtractor, PdfTextExtractor])` extracts a 7-bit
  `multipart/mixed` message (text part `Total: 1,234.56 EUR`, `application/pdf` part =
  `common::minimal_pdf(&["Due date: 15/10/2026"])`, as #5's AC8 builds it) THE SYSTEM SHALL
  return `Ok` equal to `merge(vec![text, pdf])` of the two run separately, with amount
  `Money(123456, EUR)` and due 2026-10-15 both present.

<!-- GATE 1: [ ] EARS, pub-only [ ] e2e happy + failure [ ] no [unverified] load-bearing
     [ ] non-goals [ ] PROMOTES [ ] ~15k context [ ] ≤800 words -->
