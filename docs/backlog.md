# Backlog
<!-- Ideas agreed as future work but deliberately NOT filed yet. When one is filed as a
     `feature` issue, delete its entry here (the issue becomes the record). -->

## Line items for multi-item bills (noted 2026-10-05)
**Need:** analysis per item bought, e.g. a supermarket bill (Éxito) with many products.
Today a `Bill` keeps only the total: no extractor reads line items and raw emails are not
persisted, so the per-item detail is lost after ingest.

**Intended shape when filed:**
- **Source:** DIAN UBL XML only, first. `XmlInvoiceExtractor` already reads that XML at
  confidence 100. Each `cac:InvoiceLine` gives description, quantity, unit price, line total
  and tax, so reading items there is exact, not a guess.
- **Model:** a `LineItem` type in `bill` and a `bill_items` table (one row per item, keyed by
  bill id), added by a migration. The `pub` surface changes in `bill`, `extract` and `store`
  (PROMOTES).
- **Views:** items in the TUI detail pane; optionally in the `GET /v1/bills/{id}` response.
- **Decision to record:** this reverses "Out of scope: analytics" in `docs/architecture.md`.

**Later, separately:** items from PDFs and email bodies through the LLM pass. Keep this out
of the first issue, because a model's line items are much less trustworthy than the XML.
