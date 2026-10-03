# Spec: `llm` plumbing: rig-core client, pdftoppm rasterizer, env config (#23)

## Problem
First half of the LLM-backed extractor (the extractor and its wiring are #26). Core gains an `llm` module: three system edges behind injectable traits, a chat client
(rig-core; Ollama in dev, Anthropic/OpenAI in prod), a PDF page rasterizer (`pdftoppm`) and
env configuration. No bill behaviour changes.

## Non-goals
- No `Extractor` impl, prompt, schema mapping, `Escalate` wiring or binary change (#26).
- No streaming, tools, retries, caching, cost accounting or zip/UBL-XML attachments.
- The private corpus never enters the repo.

## Assumptions
- [spike-verified] rig-core 0.43 API paths and shapes as in `features/23/spike/findings.md`
  and `spike/ref/src/main.rs`; the output schema reaches Ollama's `format`; `pdftoppm -r 150
  -png in.pdf prefix` writes `prefix-<n>.png` (1-indexed); gemma4 answers in ~53 s and, when an image part is present, wraps its JSON in a
  ```` ```json ```` fence despite `format` (amended after Gate 1; #26 strips fences).
- Dependencies (human yes 2026-10-03; the MSRV bump lets clippy 1.99 demand let-chains at
  three `extract.rs` sites, fixed here): `rig-core = "0.43"` (`default-features = false`,
  features `rustls`, `reqwest`), `schemars = "1"` (rig's own major, already in the tree);
  workspace `rust-version` 1.85 → 1.95 (rig's MSRV). Pinned in the root manifest by the
  Leader; core adds `{ workspace = true }`. Base64 is a private 15-line encoder, no crate.
- Design call: `Part { Text(String), Png(Vec<u8>), Pdf(Vec<u8>) }`, `LlmRequest {
  instructions: String, parts: Vec<Part>, schema: schemars::Schema }`, `trait LlmClient:
  Send + Sync { fn complete<'a>(&'a self, req: &'a LlmRequest) -> BoxFuture<'a,
  Result<String, Error>> }` returning the raw reply.
- Design call: `Provider { Ollama { base_url: String }, Anthropic { api_key: String,
  base_url: Option<String> }, OpenAi { api_key: String, base_url: Option<String> } }` with `Debug` printing `api_key: "<redacted>"`. `RigClient::new(provider: Provider, model: &str)` alone imports rig. `complete` builds
  `CompletionRequest::new(Message::User { content }).preamble(instructions)
  .output_schema(Some(schema))`, mapping `Text`→`UserContent::Text`, `Png`→`Image` (PNG,
  base64), `Pdf`→`Document` (PDF, base64); rig errors become `Error::Client { message }`. A
  `Pdf` part for `Ollama` is `Error::Unsupported { part: "pdf", provider: "ollama" }`
  before any network call.
- Design call: `trait Rasterizer: Send + Sync { fn rasterize(&self, pdf: &[u8], max_pages:
  u8) -> Result<Vec<Vec<u8>>, Error> }` (one PNG per page, in order).
  `Pdftoppm::new(dpi: u16)` runs `pdftoppm` from `PATH`; `with_program(program: PathBuf, dpi)` overrides it. Each call writes the PDF to a fresh temp dir, runs `<program> -r <dpi> -f 1 -l <max_pages> -png in.pdf page`, reads `page-<n>.png` in
  order, removes the dir. Missing program, non-zero exit or no output ⇒ `Error::Rasterizer { reason }`.
- Design call: `Config { provider: Provider, model: String }`; `Config::from_env(get: impl
  Fn(&str) -> Option<String>) -> Result<Option<Config>, Error>`: `HAUZ_LLM_PROVIDER` unset ⇒
  `Ok(None)`; `ollama` | `anthropic` | `openai` (else `Error::Config { variable: "HAUZ_LLM_PROVIDER" }`), `HAUZ_LLM_MODEL` required, `OLLAMA_API_BASE_URL` default
  `http://localhost:11434`, `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` required for their
  provider, `ANTHROPIC_BASE_URL` / `OPENAI_BASE_URL` optional; a missing required variable is
  `Error::Config { variable }`.
- Design call: `Error { Client { message }, Unsupported { part, provider }, Rasterizer {
  reason }, Config { variable } }`: `thiserror`, `#[non_exhaustive]`, `Clone + Eq`.

## Reference implementation
`features/23/spike/ref/src/main.rs`.

## Architecture delta
- Root `Cargo.toml`: `rust-version = "1.95"`, pins `rig-core`, `schemars`; `crates/core/Cargo.toml`
  and the constitution's allowed list += both.
- `lib.rs`: `pub mod llm`. `llm` (new): `Part`, `LlmRequest`, `LlmClient`, `Provider`,
  `RigClient`, `Rasterizer`, `Pdftoppm`, `Config`, `Error`.
- `PROMOTES: llm` → `docs/architecture.md` module line, one `docs/decisions.md` line.

## Test plan
Files: `crates/core/tests/unit_llm.rs`, `crates/core/tests/integration_llm.rs`. Fixture:
`crates/core/tests/fixtures/synthetic_bill.pdf`, copied from `features/23/spike/ref/bill.pdf`.
- AC1 [unit] WHEN `from_env` sees no `HAUZ_LLM_PROVIDER` THE SYSTEM SHALL return `Ok(None)`;
  WHEN it sees `ollama` and `HAUZ_LLM_MODEL=gemma4:latest` THE SYSTEM SHALL return
  `Provider::Ollama { base_url: "http://localhost:11434" }` and that model; WHEN it sees
  `anthropic` with a model and no key THE SYSTEM SHALL return `Err(Config { variable:
  "ANTHROPIC_API_KEY" })`; WHEN it sees `bogus` THE SYSTEM SHALL return `Err(Config {
  variable: "HAUZ_LLM_PROVIDER" })`.
- AC2 [unit] WHEN `RigClient(Ollama, …)` completes a request containing a `Pdf` part THE
  SYSTEM SHALL return `Err(Unsupported { part: "pdf", provider: "ollama" })` without network.
- AC3 [unit] WHEN `Provider::Anthropic { api_key: "sk-secret", .. }` is formatted with
  `{:?}` THE SYSTEM SHALL print `<redacted>` and not `sk-secret`.
- AC4 [unit] WHEN `Pdftoppm::with_program("/nonexistent/pdftoppm", 150)` rasterizes the
  fixture THE SYSTEM SHALL return `Err(Rasterizer { .. })`.
- AC5 [integration] WHEN `pdftoppm` is on `PATH` THE SYSTEM SHALL rasterize the fixture at
  150 dpi with `max_pages` 4 into exactly one buffer starting with `\x89PNG`; otherwise the test returns `Ok(())` early.
- AC6 [integration] WHEN `HAUZ_LIVE_OLLAMA=1` THE SYSTEM SHALL have `RigClient(Ollama,
  "gemma4:latest")` answer a request of one `Text` and one `Png` (AC5's page) with
  a schema of `{ vendor: Option<String> }` such that the reply, after stripping an optional
  markdown code fence, parses as a JSON object; otherwise the test returns `Ok(())` early.

<!-- GATE 1: [x] EARS, pub-only [x] integration present; no entry point [x] no [unverified]
     load-bearing [x] non-goals [x] PROMOTES [x] ~15k context [x] ≤800 words -->
