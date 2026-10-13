# L3 Core components

*Verified against: `unaudited`*

`hauz-core` (`crates/core`) holds every domain rule. Each box is a public module and lists
the interface it exposes. Arrows show which module uses which. The binaries call only the
modules named in the table below.

```mermaid
flowchart LR
    subgraph core["hauz-core"]
        ingest["<b>ingest</b><br/>ingest(raw, extractor, store) → Outcome<br/>reextract(…) → Reextracted<br/>raw_hash, EXTRACTED_MIN_CONFIDENCE"]:::component
        email["<b>email</b><br/>Envelope::parse(bytes)<br/>Envelope, Document"]:::component
        extract["<b>extract</b><br/>trait Extractor, Extraction, Field<br/>Text / PdfText / XmlInvoice<br/>Chain, Escalate, merge"]:::component
        llm["<b>llm</b><br/>LlmExtractor, LlmClient, RigClient<br/>Rasterizer, Pdftoppm, Config::from_env"]:::component
        zip["<b>zip</b><br/>read(bytes) → entries"]:::component
        bill["<b>bill</b><br/>Bill, BillDraft, BillId<br/>Money, Currency, Vendor, Status"]:::component
        store["<b>store</b><br/>trait BillStore<br/>SqliteStore, InMemoryStore, RawHash"]:::component
        mail["<b>mail</b><br/>trait MailSource, GmailSource<br/>Config, DateRange, fetch"]:::component
        consent["<b>consent</b><br/>Config, authorize (PKCE loopback)"]:::component
    end
    gmail["Gmail API"]:::external
    provider["LLM provider"]:::external
    sqlite[("SQLite")]:::store

    ingest --> email
    ingest --> extract
    ingest --> bill
    ingest --> store
    extract --> email
    extract --> zip
    extract --> bill
    llm --> extract
    mail --> ingest
    store --> bill
    store --> sqlite
    mail --> gmail
    consent --> gmail
    llm --> provider
    classDef person fill:#08427b,stroke:#052e56,color:#fff
    classDef system fill:#1168bd,stroke:#0b4884,color:#fff
    classDef container fill:#438dd5,stroke:#2e6295,color:#fff
    classDef component fill:#85bbf0,stroke:#5d82a8,color:#000
    classDef external fill:#999,stroke:#6b6b6b,color:#fff
    classDef store fill:#438dd5,stroke:#2e6295,color:#fff
```

| Module | Owns | Used directly by |
|---|---|---|
| `bill` | the domain vocabulary; a `Bill` exists only through `TryFrom<BillDraft>` | all binaries |
| `email` | MIME decoding of raw RFC 5322 bytes | `ingest`, `extract` |
| `extract` | "envelope ⇒ candidate fields with confidence"; composition with `Chain` / `Escalate` | server, `hauz` |
| `llm` | LLM client edge, PDF rasterizer edge, `LlmExtractor` | server, `hauz` |
| `zip` | std-only archive reading (DIAN zips) | `extract` |
| `store` | persistence, idempotency by `RawHash` | all binaries |
| `ingest` | the pipeline hash → short-circuit → parse → extract → build → insert | server, `hauz`, `mail` |
| `mail` | Gmail listing and fetch, poll query | server (poller), `hauz fetch` |
| `consent` | Gmail OAuth consent (PKCE S256, loopback) | `hauz gmail-auth` |

Item-level documentation: `cargo doc -p hauz-core --open`.
