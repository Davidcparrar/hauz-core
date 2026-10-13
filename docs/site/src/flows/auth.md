# API authentication

*Verified against: `unaudited`*

Every matched `/v1` route sits behind one bearer guard that runs before the handler, so an
unauthenticated client cannot make the server buffer a body.

```mermaid
flowchart TD
    Q[request to a /v1 route] --> H{exactly one Authorization header?}
    H -->|no| X
    H -->|yes| S{"scheme Bearer (any case),<br/>single space, then token?"}
    S -->|no| X
    S -->|yes| T{token == HAUZ_API_TOKEN?<br/>constant-time compare}
    T -->|no| X["401 {&quot;error&quot;:&quot;unauthorized&quot;}<br/>WWW-Authenticate: Bearer"]
    T -->|yes| B{body ≤ 25 MiB?}
    B -->|no| Z[413]
    B -->|yes| OK[handler]
```

- Missing and wrong tokens get the same response.
- The server refuses to start without a non-blank `HAUZ_API_TOKEN`.
- Unmatched paths are not guarded and answer 404.
- `hauz-web` has **no** auth and binds `127.0.0.1` only.
