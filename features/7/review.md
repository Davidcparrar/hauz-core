# Review: axum server: POST /v1/ingest/email and GET /v1/bills/{id} (#7)

## Cycle 1 — 2026-10-02

`verify: ALL GREEN` (exit 0)

- AC1–AC6 are `acN_*` `#[tokio::test]` fns in `crates/server/tests/e2e_http.rs` via
  `tower::oneshot` against `router`. AC7/AC8 `[unit]` in `crates/core/tests/unit_extract.rs`;
  AC9 `[integration]` in `crates/core/tests/integration_extract_email.rs` against the amended
  `minimal_pdf` text. Assertions no weaker than the criteria (AC1 id/status/amount/due/vendor;
  AC2 `list().len()==1`; AC3 non-empty `error` + empty store; AC5 exact `{"error":"internal error"}`).
- AC4 covers both branches: `BillId::new` accepts ascii-graphic, so `does-not-exist` is the
  well-formed-but-absent case and `has%20spaces` the rejected one.
- Pub surface of `crates/server` is exactly `MAX_BODY_BYTES`, `AppState`, `AppState::new`,
  `router`; `IdBody`/`ErrorBody` private. `extract::Chain` adds only `new` + `Debug` +
  `impl Extractor`. Status mapping 201/200/400/500/404/413 matches the delta; nothing leaks on 500.
- No unwrap/expect/panic in `src/`; `main.rs` is `anyhow::Result` with `?`. Both `#[allow]`s
  carry a one-line reason. Root `Cargo.toml` untouched; all server deps `workspace = true`.
  No spike paths; `docs/architecture.md` carries the PROMOTES delta; direction server → core.
- Fakes at edges only: `FailingStore` (storage); `Fixed`/`Failing` extractors prescribed by AC7/AC8.

Non-blocking nits: `time` added as a server dev-dep (workspace-pinned, not in the spec's dev
list); `tests/common/mod.rs` splits the spec's `app(store)` into `app()` + `app_with_store()`.

VERDICT: APPROVE
