# Ingest idempotency

*Verified against: `unaudited`*

The same raw bytes always give the same bill. The bill id is the hex SHA-256 of the raw
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

Thus hauz stores no new bill when:
- you ingest the same folder again;
- you run `hauz fetch` again over a range that overlaps a previous range;
- a forwarder sends the same webhook again.

A change to an extractor does not change the stored bills. To apply the change to a stored
bill, run `hauz ingest --reextract`.
