# L2 Containers

*Verified against: `unaudited`*

The four binaries share one library (`hauz-core`) and one SQLite file. Only the server and
`hauz` write to the file; `hauz-tui` and `hauz-web` open it read-only.

```mermaid
flowchart TB
    owner["<b>Owner</b><br/>[Person]"]:::person
    fwd["<b>Mail forwarder</b><br/>[External]"]:::external

    subgraph hauz["hauz"]
        server["<b>hauz-server</b><br/>[Rust, axum]<br/>/v1 HTTP API, webhook ingest, Gmail poller"]:::container
        cli["<b>hauz</b><br/>[Rust CLI]<br/>ingest, fetch, gmail-auth"]:::container
        tui["<b>hauz-tui</b><br/>[Rust, ratatui]<br/>read-only bill browser"]:::container
        web["<b>hauz-web</b><br/>[Rust, Topcoat]<br/>read-only admin on 127.0.0.1"]:::container
        db[("<b>SQLite</b><br/>[WAL file]<br/>bills, raw hashes")]:::store
    end

    gmail["<b>Gmail API</b><br/>[External]"]:::external
    llm["<b>LLM provider</b><br/>[External]"]:::external

    fwd -- "POST /v1/ingest/email" --> server
    owner -- "argv / stdout" --> cli
    owner -- "keys" --> tui
    owner -- "browser, HTTP" --> web
    server -- "read/write (sqlx)" --> db
    cli -- "read/write (sqlx)" --> db
    tui -- "read-only" --> db
    web -- "read-only" --> db
    server -- "poll label" --> gmail
    cli -- "fetch label, OAuth consent" --> gmail
    server -- "LLM pass" --> llm
    cli -- "LLM pass" --> llm
    classDef person fill:#08427b,stroke:#052e56,color:#fff
    classDef system fill:#1168bd,stroke:#0b4884,color:#fff
    classDef container fill:#438dd5,stroke:#2e6295,color:#fff
    classDef component fill:#85bbf0,stroke:#5d82a8,color:#000
    classDef external fill:#999,stroke:#6b6b6b,color:#fff
    classDef store fill:#438dd5,stroke:#2e6295,color:#fff
```

| Container | Exposes | Reads env | Details |
|---|---|---|---|
| `hauz-server` | `/v1` HTTP on `BIND_ADDR` | `HAUZ_API_TOKEN`, `DATABASE_URL`, `BIND_ADDR`, LLM, Gmail | [HTTP API](../reference/api.md) |
| `hauz` | `ingest`, `fetch`, `gmail-auth` | LLM, Gmail | [CLI](../reference/cli.md) |
| `hauz-tui` | terminal UI | none | [CLI](../reference/cli.md#hauz-tui) |
| `hauz-web` | HTML pages on `127.0.0.1:<port>` | none | [Web admin](../reference/web.md) |
| SQLite | one file plus `-wal`/`-shm` | — | [Build and run](../setup/index.md#the-database) |
