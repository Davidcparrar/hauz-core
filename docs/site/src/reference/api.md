# HTTP API

*Verified against: `unaudited`*

`hauz-server` supplies this API on `BIND_ADDR` (default `127.0.0.1:8080`). Each `/v1` route
requires `Authorization: Bearer <HAUZ_API_TOKEN>` ([auth flow](../flows/auth.md)). An error
is JSON `{"error": "<message>"}`. If the server has a route that this page does not show,
`verify.sh` fails.

| Method | Path | Success | Errors |
|---|---|---|---|
| `POST` | `/v1/ingest/email` | `201` created / `200` duplicate, `{"id"}` | 400, 401, 413, 500 |
| `GET` | `/v1/bills/{id}` | `200` Bill JSON | 401, 404, 500 |

## `POST /v1/ingest/email`
- **Body:** the raw bytes of the RFC 5322 message. The server accepts all `Content-Type`
  values. The maximum size is 25 MiB.
- **Responses:**
  - `201 {"id": "<hex sha-256>"}`: the server stored a new bill.
  - `200 {"id": …}`: the server stored the same bytes before. No data changes.
  - `400 {"error": "<parse or extraction error>"}`: the server cannot parse the mail or an
    attachment. The server stores nothing.
  - `401 {"error":"unauthorized"}` with `WWW-Authenticate: Bearer`.
  - `413`: the body is larger than 25 MiB.
  - `500 {"error":"internal error"}`: the store failed.
- [Sequence](../sequences/webhook-ingest.md).

```
curl -s -H "Authorization: Bearer $HAUZ_API_TOKEN" \
     --data-binary @crates/cli/tests/fixtures/bill.eml http://127.0.0.1:8080/v1/ingest/email
{"id":"afa8d2209db4777ad5b29e377775a70adf8d513549b70fade49b96a87380dd47"}
```

## `GET /v1/bills/{id}`
- **Path:** `id` is the hex id (64 characters) that the ingest response gives.
- **Responses:**
  - `200`: the Bill JSON.
  - `404 {"error":"not found"}`: the id is not known or is not a valid id.
  - `401`: see above.
  - `500 {"error":"internal error"}`: the store failed.

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
| `period` | `{start, end}` or null | the date format is in the next row |
| `issued`, `due` | date or null | the format is `[year, day-of-year]` (the default of the `time` crate) |
| `status` | `"extracted"` or `"needs_review"` | |
