# hauz fetch and the server poller

*Verified against: `unaudited`*

Both list one Gmail label and push every message through `ingest`. `hauz fetch` runs once
over a caller-given UTC day range. The server poller runs at startup and then every
`HAUZ_GMAIL_POLL_SECS` seconds, over the window since UTC midnight yesterday.

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

Overlapping runs are safe: an already-stored message is `Duplicate` (see
[ingest idempotency](../flows/idempotency.md)). Auth, transport or store errors stop the run;
bills stored before the stop stay stored.
