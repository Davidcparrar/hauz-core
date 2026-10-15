# hauz fetch and the server poller

*Verified against: `unaudited`*

`hauz fetch` and the poller list one Gmail label and ingest each message. `hauz fetch` runs
one time, over the UTC days that you give. The poller runs when the server starts. Then it
runs each `HAUZ_GMAIL_POLL_SECS` seconds. It reads the messages from UTC midnight of the
previous day.

```mermaid
sequenceDiagram
    autonumber
    participant T as hauz fetch / Poller tick
    participant M as core::mail
    participant G as Gmail API
    participant I as core::ingest
    participant D as SQLite
    T->>M: fetch(source, query, extractor, store)
    M->>G: POST token endpoint (refresh token ⇒ access token)
    loop each page
        M->>G: GET messages, q = label:LABEL after:EPOCH (before:EPOCH)
        G-->>M: ids, nextPageToken
    end
    loop each message id
        M->>G: GET message by id, raw format
        G-->>M: base64url raw bytes
        M->>I: ingest(raw)
        I->>D: find_by_hash, then insert if new
        alt ok
            I-->>M: Created / Duplicate
        else Email or Extract error
            I-->>M: recorded as Failed, continue
        else Store or Bill error
            I-->>M: abort the run
        end
    end
    M-->>T: one result per message
    Note over T: hauz fetch prints one line per message, exit 1 if any failed<br/>Poller logs one summary line to stderr, a failed tick is retried next tick
```

You can run over the same days again. A stored message gives `Duplicate`
([ingest idempotency](../flows/idempotency.md)). An authentication, transport or store error
stops the run. The bills that hauz stored before the error stay in the database.
