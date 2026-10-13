# hauz

*Verified against: `unaudited`*

hauz turns a bill that arrived by email (body and/or attachments) into a structured **Bill**:
who charged what, how much, in which currency, for which period, due when.

- **Idempotent:** the same email twice is one bill (keyed by the SHA-256 of the raw bytes).
- **Honest:** a bill whose fields cannot be read with confidence is stored as
  `needs_review`, never guessed and never dropped.
- **Three ways in:** an HTTP webhook, a one-shot CLI (`hauz ingest`, `hauz fetch`), or the
  server's background Gmail poll.

This site documents **interfaces, not internals**: what each binary exposes, what crosses
each boundary (HTTP, CLI, environment, files, Gmail, LLM), and in what order things happen.
For module-level detail aimed at contributors' agents see `docs/architecture.md`; for
item-level API docs run `cargo doc --open`.

| Binary | Crate | Role |
|---|---|---|
| `hauz-server` | `crates/server` | HTTP API (`/v1`), webhook ingest, background Gmail poll |
| `hauz` | `crates/cli` | one-shot ingest, Gmail fetch, Gmail consent |
| `hauz-tui` | `crates/tui` | read-only terminal bill browser |
| `hauz-web` | `crates/web` | read-only local web admin |

All four are thin shells over the `hauz-core` library (`crates/core`), which holds every
domain rule. Start with [Build and run](setup/index.md).

Each page states the commit it was last verified against; `unaudited` means no
`/docs-audit` has checked it yet (see [Maintaining these docs](maintaining.md)).
