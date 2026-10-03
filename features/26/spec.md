# Spec: extract: `LlmExtractor` behind `Escalate` with env wiring (#26)

## Problem
Second half of #23. `llm::LlmExtractor` implements `extract::Extractor`: body and PDFs
(pages plus text layer) go to an `LlmClient` with a JSON schema; the reply maps into an
`Extraction`. With `HAUZ_LLM_PROVIDER` set, server and CLI wrap today's chain in `Escalate`,
so bills left `NeedsReview` get a model pass; unset, nothing changes.

## Non-goals
- No retries, caching, streaming, per-provider prompts, zip/UBL-XML (#27), non-PDF documents.
- No change to `merge`, `Escalate`, `ingest`'s status rule, routes or CLI grammar.
- Private corpus stays out of the repo; live runs are local only.

## Assumptions
- no spike: rig, fenced replies and `pdftoppm` are #23 spike-verified; the rest is in-repo.
  `#[schemars(required)]` on an `Option` field: verified in `schemars_derive` 1.2.2.
- Design call: `LlmOptions { delivery: PdfDelivery, max_pages: u8, max_confidence: u8,
  max_body_chars: usize }` (pub fields; `Default` = `RasterizedPages`, 4, 70, 20 000),
  `PdfDelivery { RasterizedPages, Native }`, `LlmExtractor::new(client: Box<dyn LlmClient>,
  rasterizer: Box<dyn Rasterizer>, options: LlmOptions)`; `Debug` shows options.
- Design call: parts, in order: `Text(body)`, body = `text`, else tag-stripped `html`
  (`extract`'s stripper becomes `pub(crate)`), truncated to `max_body_chars` chars; then per
  `application/pdf` document `Png` per rasterized page (`RasterizedPages`, `max_pages`) or
  one `Pdf` (`Native`, rasterizer not called), then `Text(layer)` when
  `PdfTextExtractor::text_layer` is `Ok(Some)`. Other mimes skipped. No parts ⇒ `Ok(default)`, no call.
- Design call: `instructions` names the fields, ISO-8601 dates, integer minor units,
  3-letter currency (resolve `$` from country/language cues), `null` when absent, honest
  `confidence`. Private schema `LlmFields { vendor: Option<String>, amount_minor_units:
  Option<i64>, currency, period_start, period_end, issued, due: Option<String>,
  confidence: u8 }`, every field `#[schemars(required)]` (emitted, nullable). Reply: trim, strip an
  optional ``` fence, `serde_json`.
- Design call: amount iff `amount_minor_units` and `Currency::new` hold; vendor via
  `Vendor::new`; dates via ISO parse; period iff both ends parse and
  `BillingPeriod::new` holds; else `None`. Confidence = `min(confidence, max_confidence)` on
  every field; span = `Span { source: Source::Model, start: 0, end: 0 }`.
- Design call: `llm::Error::Client` ⇒ `Ok(default +
  Note::LlmUnavailable)`; unparseable JSON ⇒ `Ok(default + Note::LlmMalformed)`;
  `Unsupported`/`Rasterizer` ⇒ `Err(extract::Error::Llm(e))` (config faults, 400 per #7);
  text-layer `Error::Pdf` propagates as in `PdfTextExtractor`.
- Design call: `extract` gains `Source::Model`, `Note::{LlmUnavailable, LlmMalformed}`,
  `Error::Llm(llm::Error)` (`transparent`, no `From`).
- Design call: both binaries call `Config::from_env(|k| env::var(k).ok())` after reading
  input, before the store opens. `None` ⇒ `Chain([Text, PdfText])` as today; `Some(c)` ⇒
  `Escalate(chain, LlmExtractor(RigClient::new(c.provider, &c.model), Pdftoppm::new(150),
  LlmOptions::default()), EXTRACTED_MIN_CONFIDENCE)`. A `Config` error is `?`-propagated:
  CLI exit 1 naming the variable, no DB; server aborts startup. Server `main.rs` stays
  untested by design; the CLI is the tested entry point.

## Architecture delta
- `llm`: `LlmExtractor`, `LlmOptions`, `PdfDelivery`.
- `extract`: `Source::Model`, `Note::{LlmUnavailable, LlmMalformed}`, `Error::Llm`.
- `crates/server/src/main.rs`, `crates/cli/src/main.rs`: env wiring. No manifest change.
- `PROMOTES: llm, extract` → `docs/architecture.md` (module lines, entry points, risks),
  one `docs/decisions.md` line, README env section.

## Test plan
Files: `crates/core/tests/unit_llm.rs` (AC1–AC5), `integration_ingest.rs` (AC6),
`crates/cli/tests/e2e_cli.rs` (AC7–AC8). Fakes: `FakeClient { reply: Result<String,
llm::Error>, seen: Mutex<Vec<LlmRequest>> }`, `FakeRasterizer(Result<Vec<Vec<u8>>,
llm::Error>)`; PDF via `common::minimal_pdf`. "Full reply" = vendor
`Acme Power`, 999 `USD`, period 2026-09-01..2026-09-30, issued 2026-10-01, due 2026-10-15,
confidence 100, in a ```` ```json ```` fence.
- AC1 [unit] WHEN an envelope has `text`, `html`, a `minimal_pdf` and an `image/png`
  document and the rasterizer yields two PNGs THE SYSTEM SHALL send exactly `[Text(text),
  Png, Png, Text(layer containing the PDF's line)]`, non-empty `instructions`, a schema
  whose `required` lists all eight `LlmFields` names.
- AC2 [unit] WHEN `text` is `None`, `html` is `<p>Total</p><p>99</p>` and `max_body_chars`
  is 5 THE SYSTEM SHALL send `Text("Total")` first; WHEN delivery is `Native` THE SYSTEM
  SHALL send one `Pdf(bytes)` per PDF and SHALL NOT call the rasterizer.
- AC3 [unit] WHEN the client returns the full reply THE SYSTEM SHALL map every field at
  confidence 70, span `Model/0/0`, no notes.
- AC4 [unit] WHEN the reply has currency `$`, vendor `"  "`, `period_end` null, `due` null,
  confidence 40 THE SYSTEM SHALL return amount, vendor, period and due `None` and `issued`
  at confidence 40.
- AC5 [unit] WHEN the client returns `Err(Client)` THE SYSTEM SHALL return `Ok` default plus
  `LlmUnavailable`; WHEN it returns `not json` THE SYSTEM SHALL return `Ok` default plus
  `LlmMalformed`; WHEN it returns `Err(Unsupported)` or the rasterizer fails THE SYSTEM
  SHALL return `Err(Error::Llm(..))`; WHEN the envelope is empty THE SYSTEM SHALL return
  `Ok` default without calling the client.
- AC6 [integration] WHEN `ingest` runs `Escalate(Chain([TextExtractor, PdfTextExtractor]),
  LlmExtractor(full-reply fake), 50)` over `bill_eml()` into a `SqliteStore` THE SYSTEM
  SHALL store `Extracted` with vendor `Acme Power`, period 2026-09-01..30, due 2026-10-15
  and amount 123 456 `EUR` (heuristic 90 beats 70).
- AC7 [e2e] WHEN `hauz ingest` runs with `HAUZ_LLM_PROVIDER=anthropic`, `HAUZ_LLM_MODEL=m`
  and `ANTHROPIC_API_KEY` removed THE SYSTEM SHALL exit 1, name `ANTHROPIC_API_KEY` on
  stderr and create no DB file.
- AC8 [e2e] WHEN `hauz ingest bill.eml` runs with `HAUZ_LLM_PROVIDER=ollama`,
  `HAUZ_LLM_MODEL=m`, `OLLAMA_API_BASE_URL=http://127.0.0.1:1` THE SYSTEM SHALL exit 0,
  print `Created <hash>` and store the bill `NeedsReview`.

<!-- GATE 1: all seven boxes ticked (e2e: AC8 happy, AC7 failure) -->
