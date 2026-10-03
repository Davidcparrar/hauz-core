# Review: `llm` plumbing — rig-core client, pdftoppm rasterizer, env config (#23)

## Cycle 1 — 2026-10-03, reviewer on 0321e7d (spec amended at ec4a0fc)
VERDICT: APPROVE
verify.sh: `verify: ALL GREEN`
Live: `HAUZ_LIVE_OLLAMA=1 cargo test -p hauz-core --test integration_llm` → 2 passed (ac5, ac6;
ac6 takes 3.3 s with the flag, 0 s without, so the live branch really runs).

- AC1–AC4 as `ac1_*` (×4), `ac2_*`, `ac3_*`, `ac4_*` in `crates/core/tests/unit_llm.rs`;
  AC5/AC6 in `integration_llm.rs`; levels and files as the spec names them, each asserting
  the criterion's own claim (default base URL and model, `ANTHROPIC_API_KEY`,
  `HAUZ_LLM_PROVIDER`, `Unsupported { pdf, ollama }`, `<redacted>` and no `sk-secret`,
  `Rasterizer { .. }`, one PNG-signed buffer, fence-stripped reply parses as an object).
- `crates/core/src/llm.rs`: no `unwrap`/`expect`/`panic`/indexing; base64 padding correct
  for all chunk lengths; both traits `Send + Sync` over the crate's `Send` `BoxFuture`; sole
  `rig_core` importer; `Part::Pdf` on Ollama rejected before any request is built;
  `Provider`'s manual `Debug` redacts both key variants; `Pdftoppm` uses a per-call temp dir
  removed on both paths; `from_env` matches the spec's variables.
- Only `crates/core/Cargo.toml` gained `{ workspace = true }` (rig-core, schemars), pinned
  in the root manifest, backed by decisions.md #23 and the constitution's allowed list.
- The three `extract.rs` hunks are pure let-chain collapses (clippy 1.99 at MSRV 1.95).
- Fence-stripping helper is test-local; no fakes of own types; no path dependency on
  `features/`. `docs/architecture.md` carries the `llm` line (PROMOTES), direction intact.
- Non-blocking: ac6 also returns early when `pdftoppm` is absent (it needs AC5's page);
  `from_env` checks `HAUZ_LLM_MODEL` before the provider name, unobservable in AC1.

REQUIRED CHANGES: none.
