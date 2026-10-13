# Environment reference

*Verified against: `unaudited`*

Plain environment variables are the only configuration source: hauz reads no config file and
no `.env`. `verify.sh` fails if code reads a variable this table does not list.

| Variable | Used by | Required | Default | Secret |
|---|---|---|---|---|
| `HAUZ_API_TOKEN` | server | yes (non-blank, else refuses to start) | — | **yes** |
| `DATABASE_URL` | server | yes | — | no |
| `BIND_ADDR` | server | no | `127.0.0.1:8080` | no |
| `HAUZ_LLM_PROVIDER` | server, `hauz ingest`, `hauz fetch` | no; unset ⇒ no LLM pass | — | no |
| `HAUZ_LLM_MODEL` | same | once a provider is set | — | no |
| `OLLAMA_API_BASE_URL` | same, provider `ollama` | no | `http://localhost:11434` | no |
| `ANTHROPIC_API_KEY` | same, provider `anthropic` | yes for that provider | — | **yes** |
| `ANTHROPIC_BASE_URL` | same, provider `anthropic` | no | the provider's | no |
| `OPENAI_API_KEY` | same, provider `openai` | yes for that provider | — | **yes** |
| `OPENAI_BASE_URL` | same, provider `openai` | no | the provider's | no |
| `HAUZ_GMAIL_CLIENT_ID` | server poll, `hauz fetch`, `hauz gmail-auth` | no; unset ⇒ Gmail off (`fetch`, `gmail-auth` fail) | — | no |
| `HAUZ_GMAIL_CLIENT_SECRET` | same | once the client id is set | — | **yes** |
| `HAUZ_GMAIL_REFRESH_TOKEN` | server poll, `hauz fetch` | once the client id is set | — | **yes** (reads the whole mailbox) |
| `HAUZ_GMAIL_LABEL` | server poll, `hauz fetch` | once the client id is set | — | no |
| `HAUZ_GMAIL_POLL_SECS` | server poll | no (integer ≥ 1) | `1800` | no |
| `HAUZ_GMAIL_TOKEN_URL` | Gmail, `gmail-auth` | no (test override) | Google's | no |
| `HAUZ_GMAIL_API_BASE` | Gmail | no (test override) | Google's | no |
| `HAUZ_GMAIL_AUTH_URL` | `hauz gmail-auth` | no (test override) | Google's | no |

**Failure behavior:** a missing required variable or an unknown `HAUZ_LLM_PROVIDER` makes
the CLI exit 1 and the server refuse to start, in both cases **before** the database is
opened. The error names the variable, except for the server's `DATABASE_URL`, which
fails with a generic `environment variable not found`. Secrets are redacted from `Debug`
output.

Keep secrets in the environment only: never in the repo, a `.env` file or an image.
