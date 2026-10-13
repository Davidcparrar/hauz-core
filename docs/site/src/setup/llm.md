# LLM providers

*Verified against: `unaudited`*

Extraction is heuristic by default. Setting `HAUZ_LLM_PROVIDER` adds an LLM pass that runs
**only** when the heuristics leave a bill short of `extracted`. Its fields are merged in at
a confidence of at most 70 ([extraction decision](../flows/extraction.md)). The server,
`hauz ingest` and `hauz fetch` all read the same variables.

| Provider | Variables | Notes |
|---|---|---|
| `ollama` | `HAUZ_LLM_MODEL`, optional `OLLAMA_API_BASE_URL` | local; run `ollama serve` first |
| `anthropic` | `HAUZ_LLM_MODEL`, `ANTHROPIC_API_KEY`, optional `ANTHROPIC_BASE_URL` | hosted |
| `openai` | `HAUZ_LLM_MODEL`, `OPENAI_API_KEY`, optional `OPENAI_BASE_URL` | hosted |

```
export HAUZ_LLM_PROVIDER=ollama HAUZ_LLM_MODEL=gemma4:latest
export HAUZ_LLM_PROVIDER=anthropic HAUZ_LLM_MODEL=claude-sonnet-5-5 ANTHROPIC_API_KEY=sk-ant-...
unset HAUZ_LLM_PROVIDER     # back to heuristics only
```

## What is sent
The email's text (up to 20,000 characters) plus attachments. PDFs go as page images (up to 4
pages at 150 dpi), rasterized by poppler's `pdftoppm`, which must be on `PATH`. The model
must answer JSON matching a schema hauz sends with the request.

## When it fails
- **Model unreachable or bad JSON:** the bill is still stored, as `needs_review` with the
  heuristic fields. A down Ollama therefore looks like "the LLM found nothing".
- **Unsupported input or rasterizer failure:** an extraction error (`400` on the webhook,
  exit 1 in the CLI). Nothing is stored.
- **Bad configuration:** reported before the database opens ([Environment reference](env.md)).
