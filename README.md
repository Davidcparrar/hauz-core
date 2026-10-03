# hauz

Turns a bill that arrived by email (body and/or attachments) into a structured `Bill` row:
who charged what, how much, in which currency, for which period, due when. Every record is
stored durably and idempotently (the same email twice ⇒ one bill), and whatever the
extractor cannot read is stored as `needs_review`, never guessed and never dropped.
Crate map and module interfaces: `docs/architecture.md`.

## Build
Rust ≥ the workspace `rust-version` (see `Cargo.toml`). `cargo build --workspace` builds the
library (`crates/core`), the HTTP server (`crates/server`) and the CLI (`crates/cli`, binary
`hauz`). `.claude/scripts/verify.sh` is the single verification entry point (fmt, clippy with
`-D warnings`, tests, doc budgets).

## Server
```
DATABASE_URL=./hauz.db BIND_ADDR=127.0.0.1:8080 cargo run -p hauz-server
```
- `POST /v1/ingest/email` with the raw RFC 5322 bytes as the body (any `Content-Type`,
  ≤ 25 MiB): `201 {"id": …}` created, `200 {"id": …}` duplicate, `400` unparseable mail
  or attachment, `500` on a store failure.
- `GET /v1/bills/{id}`: the bill as JSON, `404` if unknown.

## CLI: replay bills against a throwaway database
`hauz ingest` runs the same pipeline as the server on one `.eml` file and prints one line,
`Created <id>` or `Duplicate <id>`. It is meant for replaying a folder of real bills while
tuning the extractor, without the server.

```
cargo build -p hauz-cli                     # binary at target/debug/hauz
hauz ingest path/to/bill.eml --db /tmp/replay.db
```
- `--db` defaults to `./hauz.db` in the current directory; the file is created on first use.
  Flag and positional may appear in any order after `ingest`.
- Exit 0 on success, 1 on a runtime error (unreadable file, unparseable mail, store error;
  message on stderr), 2 on a usage error. `hauz --help` prints the usage line.
- Optional LLM pass (server and CLI alike): set `HAUZ_LLM_PROVIDER` to `ollama`, `anthropic`
  or `openai` plus `HAUZ_LLM_MODEL`; `anthropic`/`openai` need `ANTHROPIC_API_KEY` /
  `OPENAI_API_KEY` (`*_BASE_URL` optional), `ollama` reads `OLLAMA_API_BASE_URL` (default
  `http://localhost:11434`). PDF pages are rasterized with poppler's `pdftoppm` (must be on
  `PATH`). The model runs only when the heuristics leave a bill short of `Extracted`; a
  missing or invalid variable exits 1 before the database is touched, and an unreachable
  model stores the bill `NeedsReview` with the heuristic fields rather than failing.
- Replay a whole folder and inspect what the extractor made of it (`sqlite3` is enough):
  ```
  rm -f /tmp/replay.db*
  for f in ~/hauz-corpus/raw/*.eml; do hauz ingest "$f" --db /tmp/replay.db; done
  sqlite3 -header -column /tmp/replay.db \
    'select substr(id,1,8) id, status, vendor, amount_minor, currency, period_start, period_end, due from bills'
  ```
  Re-running the loop on the same database prints `Duplicate` for every file and stores
  nothing new, so a replay is safe to repeat. Delete the `.db` file (and its `-wal`/`-shm`
  siblings) to start over.

### Keep real bills out of the repo
Real bills are private. Keep them outside the checkout (the examples above use
`~/hauz-corpus/`), never under `crates/**/tests/fixtures/`. `.gitignore` refuses
`crates/core/tests/fixtures/scanned/`, `corpus/` and every `*.db` as a second line of
defence, but the first line is the location. Committed fixtures are synthetic or hand-written.

## Development process
This repo runs on a spec-driven, test-first Claude Code harness with one human gate: the PR
merge. The Leader drafts and self-checks a spec per GitHub issue, an implementer agent
writes tests first, a reviewer agent reruns `verify.sh`, and the human reads spec, review and
diff in the PR. The pipeline and every rule: `CLAUDE.md`; project rules: `docs/constitution.md`;
one-line decisions log: `docs/decisions.md`; per-feature spec, review and spike notes:
`features/<n>/`. Template origin: [harness-sdd](https://github.com/Davidcparrar/harness-sdd).
