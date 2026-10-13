# CLI reference & walkthrough

*Verified against: `unaudited`*

Each usage block below is checked by `verify.sh` against the binary's own output for an
unknown flag (stderr, exit 2).

## hauz

<!-- usage:hauz -->
```text
usage: hauz ingest [--reextract] <file.eml> [--db <sqlite path>]
       hauz fetch [--db <sqlite path>] [--after <YYYY-MM-DD>] [--before <YYYY-MM-DD>]
       hauz gmail-auth
```

| Command | Does | Stdout (one line per bill) |
|---|---|---|
| `ingest <file.eml>` | runs the pipeline on one file ([sequence](../sequences/cli-ingest.md)) | `Created <id>` or `Duplicate <id>` |
| `ingest --reextract <file.eml>` | extracts again; overwrites a stored row with exactly the new result | `Created`, `Updated` or `Unchanged <id>` |
| `fetch` | lists the Gmail label and ingests each message ([sequence](../sequences/fetch-poll.md)) | `Created <id>`, `Duplicate <id>` or `Failed <gmail id>: <error>` |
| `gmail-auth` | one-time OAuth consent ([sequence](../sequences/gmail-auth.md)) | `HAUZ_GMAIL_REFRESH_TOKEN=<token>` (URL on stderr) |
| `-h`, `--help` | prints the usage on stdout | exit 0 |

- **`--db <path>`**: defaults to `./hauz.db`; created and migrated on first use. Flags and
  the positional may come in any order after the command.
- **`--after` / `--before`**: `YYYY-MM-DD` UTC days; `--after` inclusive, `--before`
  exclusive. Omit both to fetch the whole label (a backfill). A malformed day, or `--after`
  not before `--before`, is a usage error.
- **Configuration** is resolved before the database opens: LLM for `ingest` and `fetch`,
  Gmail for `fetch` and `gmail-auth` ([Environment reference](../setup/env.md)).

| Exit | Meaning |
|---|---|
| 0 | success |
| 1 | runtime error, message on stderr: unreadable file, unparseable mail, store error, bad config; for `fetch`, any message `Failed` |
| 2 | usage error, usage on stderr |

## hauz-tui

<!-- usage:hauz-tui -->
```text
usage: hauz-tui [--db <sqlite path>]
```

A read-only bill browser. It loads every bill once at start, before the terminal opens.
- **Keys:** `j`/`k` or `↓`/`↑` move the selection; `r` toggles showing only
  `needs_review` bills; `q` or `Esc` quits.
- **Exit codes:** as `hauz`. Exit 1 when the file does not exist (nothing is created), cannot
  be listed, or holds a corrupt row. An old, unmigrated database fails with `no such column`;
  run any `hauz ingest` against it once to migrate it.

## hauz-web

<!-- usage:hauz-web -->
```text
usage: hauz-web [--db <sqlite path>] [--port <n>]
```

Serves the [web admin](web.md) on `127.0.0.1:<port>` (default `3000`) from a database
opened read-only. Exit codes are as for `hauz`.

## Walkthrough
Run from the repo root. Steps 1–3 need no network and no credentials: they use the committed
synthetic fixtures and a throwaway database.
```
cargo build --release -p hauz-cli -p hauz-tui -p hauz-web
export PATH="$PWD/target/release:$PATH" DB=/tmp/hauz-try.db F=crates/cli/tests/fixtures
rm -f $DB*; unset HAUZ_LLM_PROVIDER
```
1. **ingest**, one row per email:
   ```
   hauz ingest $F/bill.eml --db $DB               # Created <id>        exit 0
   hauz ingest $F/bill.eml --db $DB               # Duplicate <id>      exit 0
   hauz ingest --reextract $F/bill.eml --db $DB   # Unchanged <id>      exit 0
   hauz ingest $F/malformed.eml --db $DB          # Error: malformed …  exit 1
   hauz ingest nope.eml --db $DB                  # Error: reading …    exit 1
   ```
2. **Usage errors** (exit 2, usage on stderr):
   ```
   hauz --help                                    # usage on stdout, exit 0
   hauz bogus
   hauz gmail-auth extra
   hauz fetch --after 2026-10-02 --before 2026-10-01
   ```
3. **Config errors** (exit 1, before anything is opened):
   ```
   env -u HAUZ_GMAIL_CLIENT_ID hauz fetch --db $DB                         # Gmail is not configured
   HAUZ_GMAIL_CLIENT_ID=x env -u HAUZ_GMAIL_CLIENT_SECRET hauz gmail-auth   # names the secret
   ```
4. **LLM pass** (optional; [LLM providers](../setup/llm.md)):
   `HAUZ_LLM_PROVIDER=ollama HAUZ_LLM_MODEL=gemma4:latest hauz ingest --reextract <file> --db $DB`.
5. **gmail-auth**, live ([Gmail access](../setup/gmail.md)): `export "$(hauz gmail-auth)"`,
   then `echo ${HAUZ_GMAIL_REFRESH_TOKEN:+set}` prints `set`.
6. **fetch**, live:
   ```
   export HAUZ_GMAIL_LABEL=bills
   hauz fetch --after 2026-10-01 --db $DB         # Created/Duplicate/Failed per message
   hauz fetch --after 2026-10-01 --db $DB         # all Duplicate: raw bytes are stable
   ```
7. **Browse:** `hauz-tui --db $DB`, or `hauz-web --db $DB` and open
   <http://127.0.0.1:3000/admin>.

Keep real bills outside the checkout (e.g. `~/hauz-corpus/`), never under
`crates/**/tests/fixtures/`.
