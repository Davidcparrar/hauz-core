# hauz ingest / --reextract

*Verified against: `unaudited`*

`hauz ingest <file.eml>` runs the same pipeline as the webhook against one file.
`--reextract` skips the hash short-circuit and overwrites a stored row with the new result.

```mermaid
sequenceDiagram
    autonumber
    actor O as Owner
    participant C as hauz
    participant I as core::ingest
    participant X as Extractor
    participant D as SQLite
    O->>C: hauz ingest [--reextract] bill.eml --db path
    C->>C: llm::Config::from_env (exit 1 if invalid, before the DB opens)
    C->>D: SqliteStore::open (create + migrate)
    C->>C: read bill.eml (exit 1 if unreadable)
    alt plain ingest
        C->>I: ingest(raw, extractor, store)
        I->>D: find_by_hash
        alt stored
            I-->>C: Duplicate(id)
        else new
            I->>X: extract
            I->>D: insert
            I-->>C: Created(id)
        end
    else --reextract
        C->>I: reextract(raw, extractor, store)
        I->>X: extract (always)
        alt hash not stored
            I->>D: insert
            I-->>C: Created(id)
        else stored, result differs
            I->>D: replace(bill) (fields + status only)
            I-->>C: Updated(id)
        else stored, identical
            I-->>C: Unchanged(id) (no write)
        end
    end
    C-->>O: stdout "Created|Duplicate|Updated|Unchanged ID", exit 0
```
