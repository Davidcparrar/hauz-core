# HTTP API

*Verified against: `unaudited`*

Served by `hauz-server` on `BIND_ADDR` (default `127.0.0.1:8080`). Every `/v1` route requires
`Authorization: Bearer <HAUZ_API_TOKEN>` ([auth flow](../flows/auth.md)). Errors are JSON
`{"error": "<message>"}`. `verify.sh` fails if the server registers a route this page does
not list.

| Method | Path | Success | Errors |
|---|---|---|---|
| `POST` | `/v1/ingest/email` | `201` created / `200` duplicate, `{"id"}` | 400, 401, 413, 500 |
| `GET` | `/v1/bills/{id}` | `200` Bill JSON | 401, 404, 500 |

## `POST /v1/ingest/email`
- **Body:** the raw RFC 5322 message bytes. Any `Content-Type` is accepted; at most 25 MiB.
- **Responses:**
  - `201 {"id": "<hex sha-256>"}` when a new bill was stored.
  - `200 {"id": …}` when the same bytes were already stored (nothing changes).
  - `400 {"error": "<parse or extraction error>"}` for unparseable mail or a broken
    attachment; nothing is stored.
  - `401 {"error":"unauthorized"}` with `WWW-Authenticate: Bearer`.
  - `413` when the body is over 25 MiB.
  - `500 {"error":"internal error"}` on a store failure.
- [Sequence](../sequences/webhook-ingest.md).

```
curl -s -H "Authorization: Bearer $HAUZ_API_TOKEN" \
     --data-binary @crates/cli/tests/fixtures/bill.eml http://127.0.0.1:8080/v1/ingest/email
{"id":"afa8d2209db4777ad5b29e377775a70adf8d513549b70fade49b96a87380dd47"}
```

## `GET /v1/bills/{id}`
- **Path:** `id`, the 64-character hex id returned by ingest.
- **Responses:** `200` with the Bill JSON; `404 {"error":"not found"}` for an unknown or
  malformed id; `401`; `500 {"error":"internal error"}`.

```json
{"id":"afa8d2209db4777ad5b29e377775a70adf8d513549b70fade49b96a87380dd47",
 "vendor":"acme-power.example",
 "amount":{"minor_units":123456,"currency":"EUR"},
 "period":null,"issued":null,"due":[2026,288],
 "status":"needs_review"}
```

| Field | Type | Notes |
|---|---|---|
| `id` | string | hex SHA-256 of the raw email |
| `vendor` | string or null | |
| `amount` | `{minor_units: int, currency: ISO 4217}` or null | `123456` EUR = 1234.56 |
| `period` | `{start, end}` or null | dates as below |
| `issued`, `due` | date or null | serialized as `[year, day-of-year]` (the `time` crate's default) |
| `status` | `"extracted"` or `"needs_review"` | |
