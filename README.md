# hauz

Turns a bill that arrived by email (body and/or attachments) into a structured `Bill` row:
who charged what, how much, in which currency, for which period, due when. Every record is
stored durably and idempotently (the same email twice ⇒ one bill), and whatever the
extractor cannot read is stored as `needs_review`, never guessed and never dropped.
Crate map and module interfaces: `docs/architecture.md`.

**Documentation site** (C4 diagrams, sequences, setup, env/CLI/API reference): `docs/site/`,
an mdBook kept in step with the code by `verify.sh` drift checks. Preview it with
`cargo install --locked mdbook@0.5.4 mdbook-mermaid@0.17.1 && mdbook serve docs/site`.

## Build
Rust ≥ the workspace `rust-version` (see `Cargo.toml`). `cargo build --workspace` builds the
library (`crates/core`), the HTTP server (`crates/server`), the CLI (`crates/cli`, binary
`hauz`) and the terminal bill browser (`crates/tui`, binary `hauz-tui`). `.claude/scripts/verify.sh` is the single verification entry point (fmt, clippy with
`-D warnings`, tests, doc budgets).

## Server
```
HAUZ_API_TOKEN=$(openssl rand -hex 32) DATABASE_URL=./hauz.db BIND_ADDR=127.0.0.1:8080 cargo run -p hauz-server
```
`HAUZ_API_TOKEN` is required: the server refuses to start if it is unset or blank. Every `/v1`
request must send `Authorization: Bearer <token>`; a missing or wrong token gets
`401 {"error":"unauthorized"}`.
- `POST /v1/ingest/email` with the raw RFC 5322 bytes as the body (any `Content-Type`,
  ≤ 25 MiB): `201 {"id": …}` created, `200 {"id": …}` duplicate, `400` unparseable mail
  or attachment, `500` on a store failure.
- `GET /v1/bills/{id}`: the bill as JSON, `404` if unknown.

**Background Gmail poll:** when `HAUZ_GMAIL_CLIENT_ID` is set (see the Gmail section; all
`HAUZ_GMAIL_*` variables apply), the server also polls Gmail. It runs once at startup and then
every `HAUZ_GMAIL_POLL_SECS` seconds (default 1800). Each tick fetches the label since UTC
midnight yesterday and logs one summary line to stderr, e.g. `gmail poll: …`. A failed tick is
logged and retried next tick. Incomplete Gmail config stops startup.

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
`Created <id>` or `Duplicate <id>`. With `--reextract` it extracts again even when the email
is already stored. It overwrites that row with the new result and prints `Created`, `Updated`
or `Unchanged <id>` (see *Updating a stored bill*). It is meant for replaying a folder of real bills while
tuning the extractor, without the server.

```
cargo build --release -p hauz-cli          # binary at target/release/hauz
hauz ingest path/to/bill.eml --db /tmp/replay.db
```
- **`--db`:** defaults to `./hauz.db` in the current directory. The file is created and
  migrated on first use. Flags and positional may appear in any order after `ingest`.
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

## CLI: fetch bills from Gmail
`hauz fetch` lists the messages under one Gmail label through the Gmail API, downloads each
raw message and runs it through the same pipeline as `hauz ingest`. Access is read-only
(`gmail.readonly`); hauz never relabels, marks read or deletes mail.

```
hauz fetch --db /tmp/replay.db                                    # everything under the label
hauz fetch --after 2026-09-01 --db /tmp/replay.db                 # since 1 Sep
hauz fetch --after 2026-09-01 --before 2026-10-01 --db /tmp/replay.db   # September only
```
- **Dates:** `YYYY-MM-DD`, UTC days. `--after` is inclusive and `--before` exclusive. Either may
  be omitted, and without both the whole label is fetched (a backfill). A malformed day or
  `--after` not before `--before` is a usage error (exit 2).
- **Output:** one line per message, `Created <id>`, `Duplicate <id>` or
  `Failed <gmail id>: <error>`. Re-running over the same range prints `Duplicate`, so overlap
  is safe.
- **Errors:** an unparseable message or attachment is reported as `Failed` and the run
  continues. An auth, network or store error stops the run. Bills stored before the stop
  stay stored. Exit 1 if anything failed, 0 otherwise.

| Variable | When | Value |
|---|---|---|
| `HAUZ_GMAIL_CLIENT_ID` | `fetch`, `gmail-auth`, server poll | OAuth client id (Desktop app) |
| `HAUZ_GMAIL_CLIENT_SECRET` | same | that client's secret |
| `HAUZ_GMAIL_REFRESH_TOKEN` | `fetch`, server poll | from `hauz gmail-auth`; can read the **whole** mailbox |
| `HAUZ_GMAIL_LABEL` | `fetch`, server poll | label name as Gmail search writes it, e.g. `bills` |
| `HAUZ_GMAIL_POLL_SECS` | server poll, optional | integer ≥ 1, default 1800 |
| `HAUZ_GMAIL_TOKEN_URL` / `HAUZ_GMAIL_API_BASE` / `HAUZ_GMAIL_AUTH_URL` | optional | endpoint overrides (tests) |

**One-time Google Cloud setup:**
1. In the Google Cloud project, enable the Gmail API.
2. Create an OAuth client of type **Desktop app**.
3. Set the OAuth consent screen to **In production**. In Testing mode, refresh tokens for
   this scope expire after 7 days. For personal use, accept the unverified-app warning.
4. In Gmail, create the label (e.g. `bills`) and a filter that applies it to bill senders.
   Label older bills by hand to backfill them.

**Getting a refresh token:** `hauz gmail-auth` runs Google's one-time consent:
```
export HAUZ_GMAIL_CLIENT_ID=<CLIENT_ID> HAUZ_GMAIL_CLIENT_SECRET=<CLIENT_SECRET>
export "$(hauz gmail-auth)"        # sets HAUZ_GMAIL_REFRESH_TOKEN in this shell
```
1. It prints a consent URL on stderr. Open it in a browser on the same machine and approve
   read-only Gmail access.
2. Google redirects the browser to a temporary listener on `127.0.0.1:<random port>`. The tab
   shows `Consent received. You can close this tab.`
3. The command prints exactly one line on stdout, `HAUZ_GMAIL_REFRESH_TOKEN=<token>`, and
   exits 0.

What can go wrong:
- **Exit 1:** a missing client id or secret (stderr names the variable), declined consent
  (`access_denied`), a mismatched `state`, or a token-endpoint error.
- **No refresh token:** if Google answers without one, the command fails instead of printing
  an empty line. The consent always asks for one (`access_type=offline`, `prompt=consent`).
- **No timeout:** the command waits for the redirect until you press Ctrl-C.

Keep the token in the environment only: never put it in the repo, a `.env` file or the image.
Revoke it at myaccount.google.com/permissions.

**First real run:** fetch the same range twice. The second run must print only `Duplicate`,
which confirms Gmail returns the same raw bytes every time. Use the throwaway database
recipe from the section above.

## CLI walkthrough: try every command
Moved to the documentation site: *CLI reference & walkthrough*
(`docs/site/src/reference/cli.md`), where `verify.sh` checks the usage text against the
binaries.

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
`ingest` sees an already-stored email's hash and returns `Duplicate` without extracting. That
holds even after you turn on the LLM or improve an extractor. To re-run extraction on a stored
email, use `--reextract`:
```
hauz ingest --reextract path/to/bill.eml --db /tmp/replay.db
```
- **Output:** `Updated <id>` when the result changed. The row now holds exactly the new result
  (fields and status), including downgrades to `needs_review` and emptied fields; nothing is
  merged with the old row.
- **No change:** `Unchanged <id>` when the result is identical, with no write.
- **New email:** `Created <id>` when it was never stored.
- **Kept fields:** id, hash, insertion time and list order stay the same.

Stop the server first if it writes to the same file.

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
one-line decisions log: `docs/decisions.md`; agreed-but-unfiled future work: `docs/backlog.md`; frontend roadmap (draft PRD): `docs/prd.md`; per-feature spec, review and spike notes:
`features/<n>/`. Template origin: [harness-sdd](https://github.com/Davidcparrar/harness-sdd).
