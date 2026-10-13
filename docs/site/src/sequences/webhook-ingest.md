# Webhook ingest

*Verified against: `unaudited`*

A client sends `POST /v1/ingest/email` with the raw RFC 5322 bytes as the body. The server
examines the token before it reads the body ([API auth flow](../flows/auth.md)).

```mermaid
sequenceDiagram
    autonumber
    participant F as Forwarder
    participant S as hauz-server
    participant I as core::ingest
    participant X as Extractor (Chain / Escalate)
    participant D as SQLite (BillStore)
    F->>S: POST /v1/ingest/email (Bearer, ≤ 25 MiB)
    alt missing or wrong token
        S-->>F: 401 {"error":"unauthorized"}
    else body > 25 MiB
        S-->>F: 413
    else
        S->>I: ingest(raw, extractor, store)
        I->>I: raw_hash = SHA-256(raw)
        I->>D: find_by_hash(hash)
        alt already stored
            D-->>I: BillId
            I-->>S: Duplicate(id)
            S-->>F: 200 {"id"}
        else new
            I->>I: Envelope::parse(raw)
            I->>X: extract(envelope)
            X-->>I: Extraction (fields + confidence)
            I->>I: build Bill (extracted or needs_review)
            I->>D: insert(bill, hash)
            I-->>S: Created(id)
            S-->>F: 201 {"id"}
        end
    end
    Note over S,F: parse or extractor error ⇒ 400 {"error": "..."}, nothing stored<br/>store error ⇒ 500 {"error":"internal error"}
```
