# Web admin

*Verified against: `unaudited`*

`hauz-web` is an HTML admin that does not change the database. It listens on
`127.0.0.1:<port>` (default 3000). The server makes the pages with Topcoat
([request sequence](../sequences/web-request.md)).

> **WARNING:** `hauz-web` has no authentication. Do not make it available outside
> `127.0.0.1`. Other persons can then read all bills.

| Method | Path | Shows |
|---|---|---|
| `GET` | `/admin` | all bills, newest first: start of the id, vendor, amount, period, dates, status |
| `GET` | `/admin/needs-review` | only `needs_review` bills |
| `GET` | `/admin/bills/{id}` | all fields of one bill. 404 if the id is not known or not valid |
| `GET` | `/style.css` | the stylesheet. Each color, font, radius and space is a custom property in `theme.css` (light and dark) |

The server reads the bills for each request. To see new bills, load the page again.
