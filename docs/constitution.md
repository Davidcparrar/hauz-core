# Constitution
<!-- ≤500 words, verify-enforced. Only PROJECT-SPECIFIC rules live here. Rust-language rules
     are lints (Cargo.toml + clippy.toml) and the tdd-rust skill — do not restate them. -->

## Shape
- Workspace: `crates/core` = library (all domain logic, the only thing specs test);
  binaries (`server`, `app`) are thin shells. Crate map: `docs/architecture.md`.
- Dependency direction: binaries → core. Core never depends on a binary or on a spike.

## Public surface (binding)
The testable surface of a crate is exactly the `pub` items reachable from its root
(`lib.rs`). Everything else is `pub(crate)` (the `unreachable_pub` lint nags otherwise).
Acceptance tests live in `crates/<c>/tests/` and therefore cannot see internals. In-file
`#[cfg(test)]` modules may cover private helpers but never satisfy a spec criterion.

## Test levels (binding)
| Tag | Meaning | File | Tooling |
|---|---|---|---|
| `[unit]` | one module's pub API, nothing else involved | `crates/core/tests/unit_<module>.rs` | `#[test]`, `Result`-returning |
| `[integration]` | 2+ modules/crates together, real components, tmp dirs | `crates/core/tests/integration_<slug>.rs` | `#[test]` |
| `[e2e]` | through the outer entry point | `crates/server/tests/e2e_<slug>.rs`; CLI `tests/e2e_cli.rs` | axum: in-process `tower::ServiceExt::oneshot` against `pub fn router(state)`; CLI: `assert_cmd` |
| `[property]` | an invariant over an input space | `crates/core/tests/property_<module>.rs` | `proptest`; commit `*.proptest-regressions` |

- One file per level per module (each `tests/*.rs` is a crate and a link step); shared
  fixtures in `tests/common/mod.rs`.
- Test fn name = criterion id + behavior: `ac3_rejects_empty_name`.
- `[integration]` REQUIRED when a feature crosses module boundaries; `[e2e]` REQUIRED when it
  touches the entry point (≥1 happy + 1 failure path).
- Mock only at true system edges (network, clock, filesystem) via an injected trait; never
  your own types. A real socket in `[e2e]` only when the criterion is about the transport.

## Errors
- Library (`core`): no `unwrap`/`expect`/`panic`; errors are `thiserror` enums,
  `#[non_exhaustive]` when they may grow; fallible constructors return `Result`
  (parse, don't validate).
- Binaries: `fn main() -> anyhow::Result<()>`; `unwrap` is still a lint error, use `?`.

## Dependencies
Allowed (workspace-pinned): thiserror, anyhow, tokio, axum, tower, http-body-util,
proptest, assert_cmd. Adding one = a line in `docs/decisions.md` + an explicit human yes.
Agents never run `cargo add`.

## GPUI (when `crates/app` exists)
`crates/app` maps core types to elements and events to core method calls; it never computes a
domain value. All state is a core type held in an `Entity<_>`. App tests are `[e2e]` only
("event X calls core Y", "renders without panic") and use `#[gpui::test]`. Set
`default-members = ["crates/core", "crates/server"]` so plain `cargo test` skips the app;
`verify.sh` still builds it.

## Budgets (verify-enforced)
constitution ≤500 words · architecture ≤500 · spec ≤800 · spike findings ≤400 ·
agent return ≤300 tokens · implementer context ≈15k tokens (larger ⇒ split the feature).
