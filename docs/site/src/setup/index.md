# Build and run

*Verified against: `unaudited`*

## Prerequisites
- Rust at least the workspace `rust-version` in `Cargo.toml` (currently 1.98).
- SQLite needs nothing extra: it is compiled in through `sqlx`.
- Optional: poppler's `pdftoppm` on `PATH`. The LLM pass uses it to send PDFs as page
  images (see [LLM providers](llm.md)).
- Optional: Ollama, or an Anthropic or OpenAI key, for the [LLM pass](llm.md).
- Optional: a Google Cloud OAuth client, for [Gmail access](gmail.md).

## Build
```
cargo build --release --workspace
ls target/release/{hauz-server,hauz,hauz-tui,hauz-web}
```
`.claude/scripts/verify.sh` is the single verification entry point. It runs fmt, clippy
with `-D warnings`, the tests, doc budgets and these docs' [drift checks](../maintaining.md).

## The database
Every binary works on one SQLite file (WAL mode, so you will also see `-wal` and `-shm`
siblings).

| Binary | How the path is given | Default | Opens |
|---|---|---|---|
| `hauz-server` | `DATABASE_URL` (a path, optionally `sqlite://`-prefixed) | required | read-write; creates and migrates |
| `hauz` | `--db <path>` | `./hauz.db` | read-write; creates and migrates |
| `hauz-tui` | `--db <path>` | `./hauz.db` | **read-only**; never creates or migrates |
| `hauz-web` | `--db <path>` | `./hauz.db` | **read-only**; never creates or migrates |

The server should be the only writer while it runs. Stop it before running `hauz ingest`
or `hauz fetch` against the same file.

## Running each binary
```
# HTTP server (see the HTTP API reference)
HAUZ_API_TOKEN=$(openssl rand -hex 32) DATABASE_URL=./hauz.db hauz-server

# one-shot CLI (see the CLI reference)
hauz ingest bill.eml --db ./hauz.db

# terminal browser: j/k or arrows move, r toggles needs_review only, q or Esc quits
hauz-tui --db ./hauz.db

# local web admin on http://127.0.0.1:3000/admin
hauz-web --db ./hauz.db --port 3000
```
Every variable each binary reads is listed in the [Environment reference](env.md). The
binaries' arguments are in the [CLI reference](../reference/cli.md).
