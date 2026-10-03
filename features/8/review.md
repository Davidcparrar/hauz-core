# Review: hauz ingest CLI (#8)

## Cycle 1 — 2026-10-02, reviewer on 8318bbd
VERDICT: REJECT (code-defect)
verify.sh: `verify: ALL GREEN`

- AC1–AC7 each have one `acN_*` fn in `crates/cli/tests/e2e_cli.rs`, all `[e2e]` via
  `assert_cmd`, asserting exit code, exact stdout, stderr content and DB state; AC3 asserts
  the db file is not created.
- `main.rs`: `#[tokio::main] async fn main() -> anyhow::Result<()>`, read-before-open,
  `process::exit(2)` on `UsageError`, no unwrap/expect/panic.
- `Cargo.toml`: `[lints] workspace = true`, `[[bin]] name = "hauz"`, workspace-pinned deps
  only; root manifest, `core`, `server` untouched.
- BLOCKER: spec declares `PROMOTES: cli` but `docs/architecture.md` and `docs/decisions.md`
  were not updated on the branch (Leader's step 8, not yet run at review time).
- Minor: `tests/common/mod.rs` `tmp_dir()` keyed on nanos alone; add pid to close the
  parallel-collision window.

REQUIRED CHANGES: (1) architecture cli entry-point line, (2) decisions #8 line, (3, optional)
pid in `tmp_dir()`.

## Cycle 2 — 2026-10-02, reviewer on c6c75b0
VERDICT: APPROVE
verify.sh: `verify: ALL GREEN` (doc budgets included; tree clean afterwards)

- `docs/architecture.md` cli line documents grammar, `./hauz.db` default, read-then-open,
  `Created|Duplicate <id>` stdout, exits 0/1/2, `-h|--help`, hand parsing, the chain over
  `SqliteStore`; each clause matches `crates/cli/src/{main,args}.rs` and the spec.
- `docs/decisions.md` has the one `#8` line matching the spec's design calls.
- `tmp_dir()` is now `hauz-cli-{pid}-{nanos}-{counter}`; only that test file changed since
  the implementation commit; `core`, `server`, root manifest untouched.
- `ac1_`…`ac7_` all `[e2e]` via `assert_cmd`, no mocks of project types.
