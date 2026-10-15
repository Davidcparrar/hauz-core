# L1 System context

*Verified against: `unaudited`*

This diagram shows the persons and systems that connect to hauz. One owner operates hauz.
A blue box is hauz. A grey box is an external system. A dashed arrow is a planned connection.

```mermaid
flowchart TB
    owner["<b>Owner</b><br/>[Person]<br/>Receives bills by email; reviews what hauz could not read"]:::person
    hauz["<b>hauz</b><br/>[Software System]<br/>Turns emailed bills into structured, idempotent Bill records"]:::system
    gmail["<b>Gmail</b><br/>[External System]<br/>Holds bill emails under one label; OAuth consent"]:::external
    sender["<b>Mail forwarder</b><br/>[External System]<br/>Anything that POSTs raw RFC 5322 bytes"]:::external
    llm["<b>LLM provider</b><br/>[External System]<br/>Ollama (local), Anthropic or OpenAI"]:::external
    spaces["<b>DigitalOcean Spaces</b><br/>[External System, planned #22]<br/>Litestream replica of the database"]:::external

    owner -- "CLI, TUI, local web admin" --> hauz
    sender -- "HTTPS POST /v1/ingest/email (bearer token)" --> hauz
    hauz -- "Gmail API, read-only (OAuth refresh token)" --> gmail
    hauz -- "Chat completion + JSON schema (only when heuristics are short)" --> llm
    hauz -. "WAL replication (planned)" .-> spaces
    classDef person fill:#08427b,stroke:#052e56,color:#fff
    classDef system fill:#1168bd,stroke:#0b4884,color:#fff
    classDef container fill:#438dd5,stroke:#2e6295,color:#fff
    classDef component fill:#85bbf0,stroke:#5d82a8,color:#000
    classDef external fill:#999,stroke:#6b6b6b,color:#fff
    classDef store fill:#438dd5,stroke:#2e6295,color:#fff
```

| Boundary | Protocol | Direction | Credential |
|---|---|---|---|
| Owner ↔ hauz | process arguments, stdout and exit codes, terminal, HTTP on `127.0.0.1` | in | none (local) |
| Forwarder → hauz | HTTP `/v1` | in | `HAUZ_API_TOKEN` bearer |
| hauz → Gmail | HTTPS REST, OAuth 2.0 | out | `HAUZ_GMAIL_*` |
| hauz → LLM | HTTPS (or local HTTP for Ollama) | out | provider key |
| hauz → Spaces | S3 API via Litestream (planned) | out | — |
