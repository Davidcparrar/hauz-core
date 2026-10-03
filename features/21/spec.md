# Spec: extract: async `Extractor` and `Escalate` combinator (#21)

## Problem
`Extractor::extract` is synchronous; the LLM-backed extractor (#23) is network-bound. This
feature makes the trait async the way `BillStore` already is (decision #2: boxed `Send`
futures, `dyn`-safe, no `async-trait`) and adds `Escalate`, a combinator that runs an
expensive extractor only when a cheap one leaves the bill short of `Extracted`. What any
existing bill extracts does not change.

## Non-goals
- No LLM, no network, no new dependency. Core gains no runtime dependency: `BoxFuture` is
  `std` only, and `tokio` stays a dev-dependency.
- No change to `merge`, to what `TextExtractor`/`PdfTextExtractor` return, to `ingest`'s
  status rule, to server routes, or to the CLI grammar.
- No parallelism inside `Chain` (sequential, first `Err` wins, as today); no
  `spawn_blocking` around the CPU-bound extractors.
- `Escalate` never decides `Status`; `ingest` still does.

## Assumptions
- no spike: every fact is in the repo (`store::BoxFuture` pattern, the call sites, the
  test fakes).
- [verified] `store::BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>`.
  Design call: the alias moves to the crate root as `hauz_core::BoxFuture`; `store` keeps
  `pub use crate::BoxFuture` so existing paths compile.
- [verified] `ingest` is the only production caller of `extract`; the server and CLI hold
  `dyn Extractor` and never call it, so they need no source change.
- Design call: completeness is one rule in one place. `Extraction::is_complete(&self,
  min_confidence: u8) -> bool` is true iff amount is present at `confidence >=
  min_confidence` and vendor and period are present. `ingest` uses it with
  `EXTRACTED_MIN_CONFIDENCE`; `Escalate` uses it with its own threshold. The threshold is
  a plain `u8` (a cut-off, not a confidence; `> 100` simply means "always escalate").
- Design call: `Escalate` returns `merge([primary, secondary])`, never the secondary
  alone, so the cheap extractor's higher-confidence fields and notes survive.
- Design call: an `Err` from either extractor propagates (decision #6: an extractor `Err`
  stores nothing). How an LLM outage is represented is #23's problem.
- Design call: `TextExtractor` and `PdfTextExtractor` keep their synchronous bodies as
  private fns and return `Box::pin(async move { self.extract_sync(envelope) })`; the
  future is `Send` because `Envelope` and the extractors are.
- Design call: test fakes implement the async trait the same way; extract tests become
  `#[tokio::test]` where they await (tokio is already a core dev-dependency).

## Architecture delta
- `lib.rs`: `pub type BoxFuture<'a, T>` (moved from `store`, which re-exports it).
- `extract`: `Extractor::extract<'a>(&'a self, envelope: &'a Envelope) -> BoxFuture<'a,
  Result<Extraction, Error>>`; `Extraction::is_complete(&self, min_confidence: u8) ->
  bool`; `pub struct Escalate` with `Escalate::new(primary: Box<dyn Extractor>,
  secondary: Box<dyn Extractor>, min_confidence: u8) -> Self` and a `Debug` impl; `impl
  Extractor for Escalate` runs `primary`, returns its result when complete, else runs
  `secondary` and returns `merge([primary, secondary])`.
- `ingest`: `ex.extract(&envelope).await?`; status via `is_complete`.
- Tests adapt: `unit_extract.rs`, `integration_extract_email.rs`, `unit_ingest.rs`,
  `property_ingest.rs` (fakes and call form); all assertions kept. The server tests import
  `store::BoxFuture`, which the re-export keeps valid.
- `PROMOTES: extract` (and the root alias) → `docs/architecture.md` extract and `store`
  lines, one `docs/decisions.md` line.

## Test plan
Files: `crates/core/tests/unit_extract.rs` (AC1–AC4, appended), `integration_ingest.rs`
(AC5), `property_extract.rs` (AC6). Fakes: `Fixed(Extraction)` and `Counting` (a `Fixed`
that records how many times it was called, via `AtomicUsize`), `Failing`.
- AC1 [unit] WHEN `is_complete(50)` is asked of an extraction with amount at confidence 50,
  vendor and period THE SYSTEM SHALL return `true`; WHEN the amount is at 49, or the vendor
  or period is absent, THE SYSTEM SHALL return `false`.
- AC2 [unit] WHEN `Escalate`'s primary returns a complete extraction THE SYSTEM SHALL return
  that extraction unchanged and SHALL NOT call the secondary (count stays 0).
- AC3 [unit] WHEN the primary returns amount-only at confidence 80 and the secondary returns
  amount at 60 plus vendor and period plus a `NoTextLayer` note THE SYSTEM SHALL call the
  secondary exactly once and return the amount at 80, the secondary's vendor and period, and
  the note.
- AC4 [unit] WHEN the primary returns `Err` THE SYSTEM SHALL return that `Err` and SHALL NOT
  call the secondary; WHEN the primary is incomplete and the secondary returns `Err` THE
  SYSTEM SHALL return that `Err`.
- AC5 [integration] WHEN `ingest` runs `bill_eml()` (amount, due and vendor, no period) with
  `Escalate(TextExtractor, Fixed(period only), EXTRACTED_MIN_CONFIDENCE)` over a `SqliteStore`
  THE SYSTEM SHALL store the bill as `Extracted` with that period; WHEN it runs with
  `TextExtractor` alone THE SYSTEM SHALL store it as `NeedsReview` (existing AC1 behaviour).
- AC6 [property] FOR ALL extractions `a`, `b` and thresholds `t`, `Escalate(Fixed(a),
  Fixed(b), t)` SHALL return `a` when `a.is_complete(t)` and `merge([a, b])` otherwise.
- Regression: every existing test in the files above keeps its assertions and passes
  under the async call form; `verify.sh` green.

<!-- GATE 1: [x] EARS, pub-only [x] integration (extract+ingest+store); no entry point touched
     [x] no [unverified] load-bearing [x] non-goals [x] PROMOTES [x] ~15k context [x] ≤800 words -->
