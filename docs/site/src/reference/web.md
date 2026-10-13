# Web admin

*Verified against: `unaudited`*

`hauz-web` is a read-only HTML admin, served on `127.0.0.1:<port>` (default 3000) **without
authentication**: it must never be exposed beyond localhost as it is. Pages are
server-rendered with Topcoat ([request sequence](../sequences/web-request.md)).

| Method | Path | Shows |
|---|---|---|
| `GET` | `/admin` | all bills, newest first: id prefix, vendor, amount, period, dates, status |
| `GET` | `/admin/needs-review` | only `needs_review` bills |
| `GET` | `/admin/bills/{id}` | one bill's full detail; 404 for an unknown or malformed id |
| `GET` | `/style.css` | the stylesheet; every color, font, radius and spacing is a `theme.css` custom property (light + dark) |

The bill list is read on every request, so new bills appear on reload.
