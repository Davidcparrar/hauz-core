# Spec: server: GET /v1/bills listing with status filter and paging (#66)

## Problem
The server can return one bill by id but cannot list them. `GET /v1/bills` returns stored bills
newest first, optionally only one status, one bounded page at a time with a cursor to the next.
It is the read base for corrections, a remote TUI and server-backed web pages (`docs/prd.md`).

## Non-goals
- No writes, search, sort options or date filters.
- No scoped tokens (the existing bearer guard applies).
- No change to `hauz-web`, `hauz-tui` or `BillStore::list`.

## Assumptions
- no spike: SQL `ORDER BY rowid DESC LIMIT` with a keyset cursor is plain sqlx. The ordering
  is the existing insertion order, reversed.
- Design call — **paging lives in the store**, not the server, so a page reads at most
  `limit + 1` rows instead of the whole table.
- Design call — **cursor = a bill id** (`before=<id>`: bills inserted before that one). It is
  stable under inserts, needs no new column, and the id is already in the JSON. A well-formed id
  that is not stored ⇒ `store::Error::NotFound` ⇒ 400 (not an empty page).
- Design call — **limits**: default 50, range 1..=200. The query params are `status`
  (`extracted` | `needs_review`), `limit` and `before`. Unknown params are ignored. Every
  invalid value ⇒ 400 `{"error":"invalid query"}`.
- Design call — **response** `{"bills": [Bill…], "next": "<id>" | null}`. Each `Bill` has the
  same JSON shape as `GET /v1/bills/{id}`. `next` is the last returned id iff more matching
  bills exist.

## Architecture delta
PROMOTES: store (pub interface):
- `pub struct PageSize` with `PageSize::new(u32) -> Result<PageSize, InvalidPageSize>` (1..=200),
  `PageSize::DEFAULT` (50) and `get()`. `pub struct InvalidPageSize(pub u32)` is a
  `thiserror` error.
- `pub struct ListQuery { pub status: Option<Status>, pub limit: PageSize, pub before:
  Option<BillId> }`.
- `pub struct BillPage { pub bills: Vec<Bill>, pub next: Option<BillId> }`.
- `BillStore::list_page<'a>(&'a self, query: &'a ListQuery) -> BoxFuture<'a, Result<BillPage,
  Error>>` returns newest first, filters by status, then pages. An unknown `before` ⇒
  `Error::NotFound(id)`. Implemented for `SqliteStore` (keyset on `rowid`, fetching
  `limit + 1`) and `InMemoryStore` (same semantics).

server: `GET /v1/bills` inside the existing `route_layer` guard. It parses the query into
`ListQuery` and answers 200 JSON, 400 on invalid input or unknown cursor, and 500 on any other
store error.

## Test plan
- AC1 [unit] WHEN `PageSize::new` gets 0 or 201 THE SYSTEM SHALL return `InvalidPageSize`,
  and WHEN it gets 1 or 200 THE SYSTEM SHALL accept it; `PageSize::DEFAULT.get() == 50`
  (`unit_store.rs`).
- AC2 [integration] WHEN three bills are stored and `list_page` runs with limit 2, then with
  `before = next`, THE SYSTEM SHALL return the two newest with `next` = the second's id, then
  the oldest with `next = None`. This holds for both `SqliteStore` (tmp dir) and `InMemoryStore`
  (`integration_store.rs`).
- AC3 [integration] WHEN bills of both statuses are stored and `list_page` runs with
  `status = NeedsReview` and limit 1 THE SYSTEM SHALL page through only `needs_review` bills,
  newest first, on both stores.
- AC4 [integration] WHEN `before` is a well-formed id that is not stored THE SYSTEM SHALL
  return `Error::NotFound` on both stores.
- AC5 [property] FOR ALL inserted sequences of bills (mixed statuses), status filters and page
  sizes, following `next` from `before = None` until `None` SHALL yield exactly the filtered
  `list()` reversed, with no repeats or gaps (`InMemoryStore`, `property_store.rs`).
- AC6 [e2e] WHEN `GET /v1/bills` with the token and two stored bills THE SYSTEM SHALL answer
  200 `{"bills": [newest, oldest], "next": null}`. WHEN `limit=1` THE SYSTEM SHALL return the
  newest with `next` set, and following `before=<next>` SHALL return the oldest
  (`e2e_http.rs`).
- AC7 [e2e] WHEN `status=needs_review` THE SYSTEM SHALL list only `needs_review` bills.
- AC8 [e2e] WHEN `status=bogus`, `limit=0`, `limit=201`, `limit=abc`, `before=nothex` or
  `before=<well-formed unknown id>` THE SYSTEM SHALL answer 400 `{"error":"invalid query"}`.
- AC9 [e2e] WHEN `GET /v1/bills` has no or a wrong bearer token THE SYSTEM SHALL answer 401.
- AC10 [e2e] WHEN the store fails (the server tests' `FailingStore`, which gains `list_page`)
  THE SYSTEM SHALL answer 500 `{"error":"internal error"}`.

<!-- GATE 1 CHECKLIST (Leader self-check, before labelling `approved`):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [x] no [unverified] assumption is load-bearing; spike questions answered or carried
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens (else split into two issues)
     [x] ≤800 words -->
