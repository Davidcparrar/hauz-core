# API authentication

*Verified against: `unaudited`*

One bearer guard protects each `/v1` route. The guard runs before the handler. Thus a
client without a correct token cannot make the server read a body into memory.

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

- A missing token and a wrong token get the same response.
- The server does not start if `HAUZ_API_TOKEN` is not set or is blank.
- The guard does not protect a path that has no route. That path gives 404.
- `hauz-web` has **no** authentication. It listens on `127.0.0.1` only.
