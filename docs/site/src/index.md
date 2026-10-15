# hauz

*Verified against: `unaudited`*

hauz changes a bill that you get by email into a structured **Bill**. The bill can be in
the body or in an attachment. A **Bill** has the vendor, the amount, the currency, the period
and the dates.

- **One email, one bill:** if hauz gets the same email two times, it keeps one bill. The
  key is the SHA-256 of the raw bytes.
- **No guesses:** if hauz cannot read the fields with confidence, it stores the bill as
  `needs_review`. hauz does not guess a value and does not delete the bill.
- **Three inputs:** an HTTP webhook, the CLI (`hauz ingest`, `hauz fetch`), or the Gmail
  poll of the server.

This site gives **interfaces, not internals**. It shows:
- what each binary supplies;
- the data that goes across each boundary (HTTP, CLI, environment, files, Gmail, LLM);
- the sequence of the operations.

For the module data that the agents use, read `docs/architecture.md`. For the documentation
of each item, run `cargo doc --open`.

| Binary | Crate | Role |
|---|---|---|
| `hauz-server` | `crates/server` | HTTP API (`/v1`), webhook ingest, background Gmail poll |
| `hauz` | `crates/cli` | ingest of one file, Gmail fetch, Gmail consent |
| `hauz-tui` | `crates/tui` | read-only terminal bill browser |
| `hauz-web` | `crates/web` | read-only local web admin |

Each binary is a thin shell over the `hauz-core` library (`crates/core`). The library
contains all domain rules. To start, read [Build and run](setup/index.md).

Each page shows the last commit that an audit compared it with. `unaudited` shows that no
`/docs-audit` has examined the page (see [Maintaining these docs](maintaining.md)).
