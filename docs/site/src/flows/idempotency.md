# Ingest idempotency

*Verified against: `unaudited`*

The same raw bytes always map to the same bill. The bill id is the hex SHA-256 of the raw
email.

```mermaid
flowchart TD
    R[raw bytes] --> H[raw_hash = SHA-256]
    H --> Q{find_by_hash}
    Q -->|found| D[Duplicate id<br/>no parse, no extract, no write]
    Q -->|absent| P[parse → extract → build Bill]
    P -->|error| E[[nothing stored]]
    P --> I[insert bill + hash]
    I --> C[Created id]
    R2["--reextract"] --> H2[raw_hash] --> P2[parse → extract → build Bill<br/>no short-circuit]
    P2 --> Q2{hash stored?}
    Q2 -->|no| C2[insert → Created]
    Q2 -->|yes, differs| U[replace fields + status → Updated]
    Q2 -->|yes, equal| N[Unchanged, no write]
```

Consequences: replaying a folder, re-running `hauz fetch` over an overlapping range, or a
forwarder retrying a webhook stores nothing new. Improving an extractor does not touch
stored rows until you run `--reextract`.
