# Extraction decision

*Verified against: `unaudited`*

How an envelope becomes `extracted` or `needs_review`. The heuristic chain always runs. The
LLM runs only when an LLM provider is configured and the chain's result is incomplete.

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

- **Completeness** = an amount at or above the threshold, plus a vendor, plus a period or an
  issue date.
- **Confidence** runs from 0 to 100. DIAN XML invoices score 100. The header-derived vendor
  (the sender's display name, else its domain) scores 20.
- Notes such as `NoTextLayer` are not persisted.
