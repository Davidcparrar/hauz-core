# hauz gmail-auth (PKCE loopback)

*Verified against: `unaudited`*

You do this consent one time. It writes a refresh token. The `state` and the PKCE verifier
come from 64 bytes of `/dev/urandom`.

```mermaid
sequenceDiagram
    autonumber
    actor O as Owner
    participant C as hauz gmail-auth
    participant L as Loopback listener (127.0.0.1:random)
    participant B as Browser
    participant A as Google OAuth
    O->>C: hauz gmail-auth
    C->>C: consent::Config::from_env (client id + secret, exit 1 if missing)
    C->>L: bind 127.0.0.1:0
    C-->>O: stderr: consent URL (code_challenge S256, state, gmail.readonly, offline)
    O->>B: open URL
    B->>A: authorize, owner approves
    A-->>B: 302 to http://127.0.0.1:port/?code&state
    B->>L: GET /?code=…&state=…
    Note over L: requests without code or error get 404<br/>(e.g. favicon, idle preconnects)
    L-->>B: "Consent received. You can close this tab."
    C->>C: state matches? (else StateMismatch, exit 1)
    C->>A: POST token (code, code_verifier, redirect_uri)
    A-->>C: refresh_token (missing ⇒ exit 1)
    C-->>O: stdout: HAUZ_GMAIL_REFRESH_TOKEN=TOKEN, exit 0
```
