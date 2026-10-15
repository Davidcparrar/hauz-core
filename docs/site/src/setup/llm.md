# LLM providers

*Verified against: `unaudited`*

By default, hauz uses only heuristics to extract the fields. When you set
`HAUZ_LLM_PROVIDER`, hauz adds an LLM pass. The LLM pass runs **only** when the heuristics
cannot make the bill `extracted`. The LLM fields get a confidence of 70 or less
([extraction decision](../flows/extraction.md)). The server, `hauz ingest` and `hauz fetch`
read the same variables.

| Provider | Variables | Notes |
|---|---|---|
| `ollama` | `HAUZ_LLM_MODEL`, optional `OLLAMA_API_BASE_URL` | local. Run `ollama serve` first |
| `anthropic` | `HAUZ_LLM_MODEL`, `ANTHROPIC_API_KEY`, optional `ANTHROPIC_BASE_URL` | hosted |
| `openai` | `HAUZ_LLM_MODEL`, `OPENAI_API_KEY`, optional `OPENAI_BASE_URL` | hosted |

```
export HAUZ_LLM_PROVIDER=ollama HAUZ_LLM_MODEL=gemma4:latest
export HAUZ_LLM_PROVIDER=anthropic HAUZ_LLM_MODEL=claude-sonnet-5-5 ANTHROPIC_API_KEY=sk-ant-...
unset HAUZ_LLM_PROVIDER     # back to heuristics only
```

## Data that hauz sends
hauz sends the text of the email (maximum 20,000 characters) and the attachments. A PDF goes
as page images (maximum 4 pages, at 150 dpi). The `pdftoppm` tool of poppler makes the images.
This tool must be on `PATH`. The model must reply with JSON that agrees with the schema in the
request.

## Errors
- **No connection to the model, or JSON that is not correct:** hauz stores the bill as
  `needs_review` with the heuristic fields.

  > **NOTE:** Thus, if Ollama does not run, the result looks like "the LLM found nothing".
- **Input that the provider does not support, or a rasterizer failure:** hauz gives an
  extraction error. The webhook gives `400` and the CLI gives exit 1. hauz stores nothing.
- **Configuration that is not correct:** hauz gives the error before it opens the database
  ([Environment reference](env.md)).
