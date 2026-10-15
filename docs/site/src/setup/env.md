# Environment reference

*Verified against: `unaudited`*

hauz reads its configuration only from environment variables. It does not read a
configuration file or a `.env` file. If the code reads a variable that this table does not
show, `verify.sh` fails.

| Variable | Used by | Required | Default | Secret |
|---|---|---|---|---|
| `HAUZ_API_TOKEN` | server | yes (not blank, else the server does not start) | — | **yes** |
| `DATABASE_URL` | server | yes | — | no |
| `BIND_ADDR` | server | no | `127.0.0.1:8080` | no |
| `HAUZ_LLM_PROVIDER` | server, `hauz ingest`, `hauz fetch` | no. Not set: no LLM pass | — | no |
| `HAUZ_LLM_MODEL` | same | once a provider is set | — | no |
| `OLLAMA_API_BASE_URL` | same, provider `ollama` | no | `http://localhost:11434` | no |
| `ANTHROPIC_API_KEY` | same, provider `anthropic` | yes for that provider | — | **yes** |
| `ANTHROPIC_BASE_URL` | same, provider `anthropic` | no | the provider's | no |
| `OPENAI_API_KEY` | same, provider `openai` | yes for that provider | — | **yes** |
| `OPENAI_BASE_URL` | same, provider `openai` | no | the provider's | no |
| `HAUZ_GMAIL_CLIENT_ID` | server poll, `hauz fetch`, `hauz gmail-auth` | no. Not set: no Gmail (`fetch` and `gmail-auth` fail) | — | no |
| `HAUZ_GMAIL_CLIENT_SECRET` | same | once the client id is set | — | **yes** |
| `HAUZ_GMAIL_REFRESH_TOKEN` | server poll, `hauz fetch` | once the client id is set | — | **yes** (reads the whole mailbox) |
| `HAUZ_GMAIL_LABEL` | server poll, `hauz fetch` | once the client id is set | — | no |
| `HAUZ_GMAIL_POLL_SECS` | server poll | no (integer ≥ 1) | `1800` | no |
| `HAUZ_GMAIL_TOKEN_URL` | Gmail, `gmail-auth` | no (test override) | Google's | no |
| `HAUZ_GMAIL_API_BASE` | Gmail | no (test override) | Google's | no |
| `HAUZ_GMAIL_AUTH_URL` | `hauz gmail-auth` | no (test override) | Google's | no |

**Errors:** a required variable can be missing, or `HAUZ_LLM_PROVIDER` can have an unknown
value. Then the CLI gives exit 1 and the server does not start. In the two cases, this occurs
**before** hauz opens the database. The error message gives the name of the variable. One
exception: a missing `DATABASE_URL` gives only `environment variable not found`. The `Debug`
output does not show secrets.

> **WARNING:** Keep the secrets only in the environment. Do not put them in the repository,
> in a `.env` file or in an image. Other persons can then use your accounts.
