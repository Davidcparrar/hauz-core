# hauz

Turns a bill that arrived by email (body and/or attachments) into a structured `Bill` row:
who charged what, how much, in which currency, for which period, due when. Every record is
stored durably and idempotently (the same email twice ⇒ one bill), and whatever the
extractor cannot read is stored as `needs_review`, never guessed and never dropped.
Crate map and module interfaces: `docs/architecture.md`.

## Build
Rust ≥ the workspace `rust-version` (see `Cargo.toml`). `cargo build --workspace` builds the
library (`crates/core`), the HTTP server (`crates/server`), the CLI (`crates/cli`, binary
`hauz`) and the terminal bill browser (`crates/tui`, binary `hauz-tui`). `.claude/scripts/verify.sh` is the single verification entry point (fmt, clippy with
`-D warnings`, tests, doc budgets).

## Server
```
DATABASE_URL=./hauz.db BIND_ADDR=127.0.0.1:8080 cargo run -p hauz-server
```
- `POST /v1/ingest/email` with the raw RFC 5322 bytes as the body (any `Content-Type`,
  ≤ 25 MiB): `201 {"id": …}` created, `200 {"id": …}` duplicate, `400` unparseable mail
  or attachment, `500` on a store failure.
- `GET /v1/bills/{id}`: the bill as JSON, `404` if unknown.

## LLM configuration (server and CLI)
Extraction is heuristic by default. Setting `HAUZ_LLM_PROVIDER` adds an LLM pass. It runs
only for a bill the heuristics leave short of `Extracted`, and its fields are merged in at
confidence ≤ 70. Plain environment variables are the only source: no config file, no `.env`.

| Variable | When | Value |
|---|---|---|
| `HAUZ_LLM_PROVIDER` | optional; unset ⇒ no LLM pass | `ollama`, `anthropic` or `openai` |
| `HAUZ_LLM_MODEL` | required once a provider is set | the provider's model id |
| `OLLAMA_API_BASE_URL` | `ollama`, optional | default `http://localhost:11434` |
| `ANTHROPIC_API_KEY` / `ANTHROPIC_BASE_URL` | `anthropic`: key required, URL optional | |
| `OPENAI_API_KEY` / `OPENAI_BASE_URL` | `openai`: key required, URL optional | |

```
# local model (run `ollama serve` first)
export HAUZ_LLM_PROVIDER=ollama HAUZ_LLM_MODEL=gemma4:latest

# hosted model
export HAUZ_LLM_PROVIDER=anthropic HAUZ_LLM_MODEL=claude-sonnet-5-5 ANTHROPIC_API_KEY=sk-ant-...

# one command only, or turn it off again
HAUZ_LLM_PROVIDER=ollama HAUZ_LLM_MODEL=gemma4:latest hauz ingest bill.eml
unset HAUZ_LLM_PROVIDER
```
- **Missing or unknown variable:** the CLI exits 1 and the server refuses to start, in both
  cases before the database is opened.
- **Model unreachable or bad JSON:** the bill is still stored, as `needs_review` with the
  heuristic fields. A down Ollama therefore looks like "the LLM found nothing".
- **PDFs:** sent as page images (up to 4 pages, 150 dpi), rasterized by poppler's `pdftoppm`,
  which must be on `PATH`.

## CLI: replay bills against a throwaway database
`hauz ingest` runs the same pipeline as the server on one `.eml` file and prints one line,
`Created <id>` or `Duplicate <id>`. It is meant for replaying a folder of real bills while
tuning the extractor, without the server.

```
cargo build --release -p hauz-cli          # binary at target/release/hauz
hauz ingest path/to/bill.eml --db /tmp/replay.db
```
- **`--db`:** defaults to `./hauz.db` in the current directory. The file is created and
  migrated on first use. Flag and positional may appear in any order after `ingest`.
- **Exit codes:** 0 on success; 1 on a runtime error (unreadable file, unparseable mail,
  store error, bad LLM config), with the message on stderr; 2 on a usage error.
  `hauz --help` prints the usage line.
- **Replaying a whole folder:**
  ```
  rm -f /tmp/replay.db*
  for f in ~/hauz-corpus/raw/*.eml; do hauz ingest "$f" --db /tmp/replay.db; done
  ```
  Re-running the loop on the same database prints `Duplicate` for every file and stores
  nothing new, so a replay is safe to repeat. To compare runs with and without the LLM,
  use two database files. Delete the `.db` file (and its `-wal`/`-shm` siblings) to start over.

## TUI: browse the stored bills
`hauz-tui` opens a database **read-only** and lists every bill. It never creates, migrates or
writes the file, so it can run beside the server or between replays.

```
cargo build --release -p hauz-tui          # binary at target/release/hauz-tui
hauz-tui --db /tmp/replay.db               # --db defaults to ./hauz.db
```
- **Layout:** the list (left) shows the newest bill first with id prefix, vendor, amount,
  issue date, due date and status; the detail pane (right) shows every field of the
  selected bill, including the full id.
- **Amounts:** shown as minor units at two decimals plus the currency code
  (`1234.56 COP`). An absent field shows `-`.
- **Keys:** `j`/`k` or `↓`/`↑` move the selection; `r` toggles showing only `needs_review`
  bills; `q` or `Esc` quits.
- **Snapshot:** bills are read once at start. Restart to see bills ingested since.
- **Exit codes:** 1 when the file does not exist (nothing is created), cannot be listed, or
  holds a corrupt row; 2 on a usage error. `hauz-tui --help` prints the usage line.
- **Old databases:** a database from before the newest migration (e.g. one written before
  the `issued` column) fails with `no such column`. Run any `hauz ingest` against it once
  to migrate it, or replay into a fresh file.

## Updating a stored bill
A stored email cannot be re-extracted yet: `ingest` sees the same hash and returns
`Duplicate` without extracting, even with the LLM now configured. A re-extract command is
issue #54. Until then, on a local test database only (never one Litestream replicates),
delete the row and ingest the email again:
```
sqlite3 /tmp/replay.db "DELETE FROM bills WHERE id = '<full id>';"   # or: WHERE status = 'needs_review'
hauz ingest path/to/bill.eml --db /tmp/replay.db
```
The full id is printed by `hauz ingest` and shown in the TUI's detail pane. Stop the server
first if it writes to the same file.

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
