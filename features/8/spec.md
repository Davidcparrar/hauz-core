# Spec: CLI: `hauz ingest <file.eml>` for local dev and replay (#8)

## Problem
Second entry point: `crates/cli`, binary `hauz`. `hauz ingest <path.eml> [--db <sqlite
path>]` reads one raw message from disk, runs the same `ingest` pipeline as the server
(`Chain([TextExtractor, PdfTextExtractor])` over a `SqliteStore`) and prints the outcome and
bill id on one line; any failure exits non-zero with a message on stderr. Purpose: replaying
a folder of real bills against a local DB while tuning the extractor, without the server.

## Non-goals
- No directory/glob/batch ingest (`hauz ingest dir/*.eml` is the shell's job), no progress
  output, no JSON output, no `hauz get`/`list` subcommands.
- No `clap` or any other new dependency: the grammar is three tokens, parsed by hand.
- No env-var configuration (`DATABASE_URL`), no `sqlite://` prefix handling, no `--db=<p>`
  form, no config file.
- No change to `crates/core` or `crates/server`; nothing new is persisted.

## Assumptions
- no spike: core's pub surface and the constitution's `assert_cmd` path settle everything.
- [verified] `assert_cmd` 2.2.2's `Command::cargo_bin("hauz")` is not deprecated, so it
  passes clippy `-D warnings`; it resolves via `CARGO_BIN_EXE_hauz` because the tests live
  in the same package as the binary.
- Design call: bin-only crate (no `lib.rs`). Its testable surface is the process
  (exit code, stdout, stderr, DB side effects); internals are `pub(crate)`.
- Design call: stdout is exactly `Created <id>` or `Duplicate <id>` (the `Outcome` variant
  name, a space, the lowercase hex id, newline) — greppable in a replay loop.
- Design call: exit 1 for runtime failures (unreadable file, `ingest::Error`, store), via
  `fn main() -> anyhow::Result<()>`; exit 2 for usage errors with the usage line on stderr.
- Design call: the file is read before the store is opened, so a bad path never creates
  an empty DB file.
- Design call: `--db` defaults to `hauz.db` in the current directory; flags and the
  positional may appear in any order after `ingest`.
- `SqliteStore::open` creates the file when missing (as the server tests rely on).
- `tokio` with the workspace features (`rt-multi-thread`, `macros`) runs the async pipeline.

## Architecture delta
New crate `crates/cli`, package `hauz-cli`, `[[bin]] name = "hauz", path = "src/main.rs"`,
`[lints] workspace = true`. Deps (`workspace = true`): `hauz-core` (path), tokio, anyhow.
Dev: assert_cmd. Root `Cargo.toml` untouched (`members = ["crates/*"]`).
- `src/args.rs` (`pub(crate)`): `enum Command { Ingest { path: PathBuf, db: PathBuf },
  Help }` and `fn parse(args: impl Iterator<Item = OsString>) -> Result<Command, UsageError>`.
  Grammar: `hauz ingest <path> [--db <path>]` (any order after `ingest`); `hauz -h|--help`.
  Anything else (no args, unknown subcommand, missing path, unknown flag, `--db` without a
  value, two positionals) is a `UsageError` carrying the offending token.
- `src/main.rs`: `#[tokio::main] async fn main() -> anyhow::Result<()>`. `UsageError` ⇒
  `eprintln!` the usage line `usage: hauz ingest <file.eml> [--db <sqlite path>]` and
  `std::process::exit(2)`. `Help` ⇒ usage on stdout, exit 0. `Ingest` ⇒ `fs::read(path)`
  (`with_context` naming the path) → `SqliteStore::open(&db)` → `ingest(&raw, &chain,
  &store)` → `println!("{variant} {id}")`. Errors propagate with `?` (exit 1, anyhow's
  `Error: …` on stderr).
- `PROMOTES: cli` → `docs/architecture.md` cli entry-point line (grammar, output, exit
  codes, default db); one `docs/decisions.md` line.

## Test plan
Files: `crates/cli/tests/e2e_cli.rs` + `tests/common/mod.rs` (`tmp_dir() -> PathBuf`, a
fresh unique directory under `std::env::temp_dir()`; `fixture(name) -> PathBuf` into
`tests/fixtures/`; `async fn ids(db) -> Vec<String>` opening `SqliteStore` and listing
ids). Fixtures `bill.eml` and `malformed.eml` copied from `crates/server/tests/fixtures/`.
Every test runs `Command::cargo_bin("hauz")` with `current_dir(tmp_dir())`; `h` below is
the lowercase hex of `hauz_core::ingest::raw_hash(bill.eml bytes)`.
- AC1 [e2e] WHEN `hauz ingest <bill.eml> --db <tmp>/a.db` runs THE SYSTEM SHALL exit 0,
  print exactly `Created <h>\n` on stdout, nothing on stderr, and `ids(a.db)` SHALL be `[h]`.
- AC2 [e2e] WHEN `hauz ingest --db <tmp>/a.db <bill.eml>` runs a second time on AC1's DB
  THE SYSTEM SHALL exit 0, print exactly `Duplicate <h>\n`, and `ids(a.db)` SHALL still be
  `[h]`.
- AC3 [e2e] WHEN the path does not exist THE SYSTEM SHALL exit 1, print nothing on stdout,
  print a stderr message containing that path, and `<tmp>/a.db` SHALL NOT exist.
- AC4 [e2e] WHEN `malformed.eml` is ingested THE SYSTEM SHALL exit 1 with non-empty stderr,
  empty stdout, and `ids(a.db)` SHALL be empty.
- AC5 [e2e] WHEN invoked with no arguments, with `frobnicate`, with `ingest` alone, or with
  `ingest <bill.eml> --verbose` THE SYSTEM SHALL exit 2 in every case, with stderr
  containing `usage: hauz ingest` and empty stdout.
- AC6 [e2e] WHEN `hauz ingest <bill.eml>` runs without `--db` THE SYSTEM SHALL exit 0,
  print `Created <h>\n`, and `<cwd>/hauz.db` SHALL exist with `ids` equal to `[h]`.
- AC7 [e2e] WHEN invoked with `--help` or `-h` THE SYSTEM SHALL exit 0 with stdout
  containing `usage: hauz ingest` and empty stderr.

<!-- GATE 1: [x] EARS, pub-only [x] e2e happy + failure [x] no [unverified] load-bearing
     [x] non-goals [x] PROMOTES [x] ~15k context [x] ≤800 words -->
