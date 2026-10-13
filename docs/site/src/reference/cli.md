# CLI reference & walkthrough

*Verified against: `unaudited`*

`verify.sh` compares each usage block on this page with the output of its binary. The binary
gives this output on stderr, with exit 2, for an unknown flag.

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
| `ingest --reextract <file.eml>` | extracts again. Replaces a stored row with the new result | `Created`, `Updated` or `Unchanged <id>` |
| `fetch` | lists the Gmail label and ingests each message ([sequence](../sequences/fetch-poll.md)) | `Created <id>`, `Duplicate <id>` or `Failed <gmail id>: <error>` |
| `gmail-auth` | gets the OAuth consent one time ([sequence](../sequences/gmail-auth.md)) | `HAUZ_GMAIL_REFRESH_TOKEN=<token>` (URL on stderr) |
| `-h`, `--help` | writes the usage on stdout | exit 0 |

- **`--db <path>`:** the default is `./hauz.db`. `hauz` creates and migrates the file when
  it does not exist. You can put the flags and the file in any sequence after the command.
- **`--after` / `--before`:** UTC days in the format `YYYY-MM-DD`. The `--after` day is
  included. The `--before` day is not included. To fetch all of the label, do not use the
  two flags. A day with a wrong format is a usage error. An `--after` day that is not before
  the `--before` day is also a usage error.
- **Configuration:** `hauz` reads the configuration before it opens the database. `ingest`
  and `fetch` read the LLM variables. `fetch` and `gmail-auth` read the Gmail variables
  ([Environment reference](../setup/env.md)).

| Exit | Meaning |
|---|---|
| 0 | success |
| 1 | runtime error, with a message on stderr: the file is not readable, the mail does not parse, the store failed, or the configuration is not correct. For `fetch`, one or more messages `Failed` |
| 2 | usage error, usage on stderr |

## hauz-tui

<!-- usage:hauz-tui -->
```text
usage: hauz-tui [--db <sqlite path>]
```

`hauz-tui` shows the bills in the terminal. It does not change the database. It reads all
bills one time when it starts, before it opens the terminal.
- **Keys:**
  - `j`/`k` or `↓`/`↑`: move the selection.
  - `r`: show only the `needs_review` bills, or show all bills again.
  - `q` or `Esc`: stop the program.
- **Exit codes:** the same as `hauz`. Exit 1 occurs if the file does not exist, if the list
  fails, or if a row is corrupt. `hauz-tui` does not create the file.
- **Old database:** a database without the latest migration fails with `no such column`. To
  migrate it, run `hauz ingest` on it one time.

## hauz-web

<!-- usage:hauz-web -->
```text
usage: hauz-web [--db <sqlite path>] [--port <n>]
```

`hauz-web` supplies the [web admin](web.md) on `127.0.0.1:<port>` (default `3000`). It
opens the database read-only. The exit codes are the same as `hauz`.

## Walkthrough
Do these steps in the root directory of the repository. Steps 1 to 3 do not use the network
or credentials. They use the synthetic fixtures in the repository and a temporary database.
```
cargo build --release -p hauz-cli -p hauz-tui -p hauz-web
export PATH="$PWD/target/release:$PATH" DB=/tmp/hauz-try.db F=crates/cli/tests/fixtures
rm -f $DB*; unset HAUZ_LLM_PROVIDER
```
1. Ingest each email. Each email gives one row:
   ```
   hauz ingest $F/bill.eml --db $DB               # Created <id>        exit 0
   hauz ingest $F/bill.eml --db $DB               # Duplicate <id>      exit 0
   hauz ingest --reextract $F/bill.eml --db $DB   # Unchanged <id>      exit 0
   hauz ingest $F/malformed.eml --db $DB          # Error: malformed …  exit 1
   hauz ingest nope.eml --db $DB                  # Error: reading …    exit 1
   ```
2. Make usage errors. Each one gives exit 2 and the usage on stderr:
   ```
   hauz --help                                    # usage on stdout, exit 0
   hauz bogus
   hauz gmail-auth extra
   hauz fetch --after 2026-10-02 --before 2026-10-01
   ```
3. Make configuration errors. Each one gives exit 1 before `hauz` opens a file:
   ```
   env -u HAUZ_GMAIL_CLIENT_ID hauz fetch --db $DB                         # Gmail is not configured
   HAUZ_GMAIL_CLIENT_ID=x env -u HAUZ_GMAIL_CLIENT_SECRET hauz gmail-auth   # names the secret
   ```
4. Optional: use the LLM pass ([LLM providers](../setup/llm.md)):
   `HAUZ_LLM_PROVIDER=ollama HAUZ_LLM_MODEL=gemma4:latest hauz ingest --reextract <file> --db $DB`.
5. Get a Gmail token ([Gmail access](../setup/gmail.md)). Run `export "$(hauz gmail-auth)"`.
   Then make sure that `echo ${HAUZ_GMAIL_REFRESH_TOKEN:+set}` writes `set`.
6. Fetch from Gmail:
   ```
   export HAUZ_GMAIL_LABEL=bills
   hauz fetch --after 2026-10-01 --db $DB         # Created/Duplicate/Failed per message
   hauz fetch --after 2026-10-01 --db $DB         # all Duplicate: raw bytes are stable
   ```
7. Look at the bills. Run `hauz-tui --db $DB`. Or run `hauz-web --db $DB` and open
   <http://127.0.0.1:3000/admin>.

> **CAUTION:** Real bills contain private data. Keep them outside the repository, for
> example in `~/hauz-corpus/`. Do not put them in `crates/**/tests/fixtures/`.
