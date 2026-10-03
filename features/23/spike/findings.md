# Spike #23 findings

## Q1 — rig 0.43 + Ollama `gemma4:latest`, structured output
Built with `rig_core::providers::ollama::wire::OllamaConfig::new().with_base_url("http://localhost:11434")`
then `.client()` → `Ollama`; `ollama.completion("gemma4:latest")` returns `Model<Chat>`
(`rig_core::driver::Model`). Request: `rig_core::completion::CompletionRequest::new(Message::User{content})`
with `UserContent::Text(Text::new(..))` + `UserContent::Image(Image{data: DocumentSourceKind::base64(..),
media_type: Some(ImageMediaType::PNG), ..})`, `.output_schema(Some(schemars::schema_for!(BillFields)))`.
Call: `model.call(request).await` → `Result<CompletionResponse, ProviderError>`; read via `.text()`.
Confirmed in `rig-core-0.43.0/src/providers/ollama.rs:208,295`: `req.output_schema` maps straight onto
Ollama's `format` field, so the schema really reaches Ollama (`/api/chat`), not just a prompt hint.

One call made (budget allowed 3; one was decisive). Result **parses as `BillFields`** — but only because
every field but `confidence` is `Option<T>` and serde's derive treats a missing `Option` field as `None`
with no `#[serde(default)]` needed. The model returned only `vendor` (with a rambling parenthetical,
not a clean string) and `confidence: 100`; amount/currency/period/due were omitted even though all were
present as plain text on the page. **Schema conformance (shape) succeeded; field recall did not** —
relevant since issue #23 means confidences the model reports to be a ceiling, not ground truth.
Wall-clock: 52.9s for one call (CPU-bound local `gemma4:latest`, "thinking"-capable model, 1275x1650 PNG).
Gotchas: needs rig-core's own `schemars` major version (1.x, not 0.8 — rig re-exports `schemars` at
`rig_core::schemars` for this reason); no gotcha on feature flags, `rustls`+`reqwest` built clean.

## Q2 — synthetic bill PNG
`gs -q -dBATCH -dNOPAUSE -sDEVICE=pdfwrite -sOutputFile=bill.pdf bill.ps` from a hand-written PostScript
page (vendor, "Total: 123,45 EUR", billing period, due date). Rasterized:
`pdftoppm -r 150 -png bill.pdf bill_150dpi` -> `bill_150dpi-1.png`, **1275x1650 px**;
`pdftoppm -r 100 -png bill.pdf bill_100dpi` -> `bill_100dpi-1.png`, **850x1100 px** (naming: `<prefix>-<page>.png`,
1-indexed). 150 dpi PNG fed to Q1 as above.

## Q3 — compile-only, Anthropic/OpenAI, no network
`AnthropicConfig::new("sk-ant-fake-key").with_base_url(..)` and `OpenAIConfig::new("sk-fake-key").with_base_url(..)`
(both `rig_core::providers::{anthropic,openai}::wire`) compile with explicit key + URL. A `CompletionRequest`
built from one `Message::User` carrying both `UserContent::Document(Document{media_type: Some(DocumentMediaType::PDF), ..})`
and `UserContent::Image(..)` type-checks for both providers — `UserContent` is provider-agnostic; conversion
is per-provider at encode time, not compile time. Source note (`providers/openai/completion/mod.rs:775-801`):
OpenAI's chat wire *does* accept a base64 PDF `Document` (converts it), but rejects `DocumentSourceKind::Raw`
("Raw files not supported, encode as base64 first") — the rejection is about source kind, not PDF itself.

<!-- STATUS: COMPLETE -->
