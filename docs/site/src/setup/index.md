# Build and run

*Verified against: `unaudited`*

## Prerequisites
- Rust, version `rust-version` of `Cargo.toml` or higher (now 1.98).
- SQLite: no installation is necessary. `sqlx` contains it.
- Optional: the `pdftoppm` tool of poppler, on `PATH`. The LLM pass uses it to send PDFs as
  page images (see [LLM providers](llm.md)).
- Optional: Ollama, or an Anthropic or OpenAI key, for the [LLM pass](llm.md).
- Optional: a Google Cloud OAuth client, for [Gmail access](gmail.md).

## Build
```
cargo build --release --workspace
ls target/release/{hauz-server,hauz,hauz-tui,hauz-web}
```
To verify the repository, run `.claude/scripts/verify.sh`. It runs fmt, clippy with
`-D warnings`, the tests, the document budgets and the [drift checks](../maintaining.md) of
this site.

## The database
All binaries use one SQLite file. The file uses WAL mode. Thus you also see a `-wal` file and
a `-shm` file.

| Binary | How the path is given | Default | Opens |
|---|---|---|---|
| `hauz-server` | `DATABASE_URL` (a path. The prefix `sqlite://` is optional) | required | read-write. Creates and migrates |
| `hauz` | `--db <path>` | `./hauz.db` | read-write. Creates and migrates |
| `hauz-tui` | `--db <path>` | `./hauz.db` | **read-only**. Does not create or migrate |
| `hauz-web` | `--db <path>` | `./hauz.db` | **read-only**. Does not create or migrate |

> **CAUTION:** When the server runs, only the server must write to the file. Stop the server
> before you run `hauz ingest` or `hauz fetch` on the same file.

## Run each binary
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
The [Environment reference](env.md) shows all variables of each binary. The
[CLI reference](../reference/cli.md) shows the arguments of each binary.
