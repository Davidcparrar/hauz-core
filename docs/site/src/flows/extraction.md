# Extraction decision

*Verified against: `unaudited`*

This flow shows how hauz gives an envelope the status `extracted` or `needs_review`. The
heuristic chain always runs. The LLM runs only if you set an LLM provider and the result of the
chain is not complete.

```mermaid
flowchart TD
    A[Envelope] --> B["Chain: XmlInvoice → Text → PdfText<br/>(first Err wins, else merge: highest confidence per field)"]
    B -->|Err| E1[[extraction error: nothing stored<br/>webhook 400 / CLI exit 1]]
    B --> C{HAUZ_LLM_PROVIDER set?}
    C -->|no| F
    C -->|yes| D{"chain result complete?<br/>amount ≥ 50 + vendor + (period or issued)"}
    D -->|yes| F
    D -->|no| L["LlmExtractor<br/>(confidence capped at 70)"]
    L -->|unreachable / bad JSON| N["empty + note<br/>(LlmUnavailable / LlmMalformed)"]
    L -->|Unsupported / rasterizer| E1
    L --> M[merge with chain result]
    N --> M
    M --> F{"complete at<br/>EXTRACTED_MIN_CONFIDENCE (50)?"}
    F -->|yes| X[status: extracted]
    F -->|no| R["status: needs_review<br/>(present fields kept, never guessed)"]
```

- **Complete result:** an amount with a confidence of 50 or more, a vendor, and a period or
  an issue date.
- **Confidence:** a value from 0 to 100. A DIAN XML invoice gets 100. A vendor from the email
  header gets 20. This vendor is the display name of the sender, or else the domain.
- **Notes:** hauz does not keep notes, for example `NoTextLayer`, in the database.
