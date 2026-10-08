# Spec: server: background Gmail poll (#43)

## Problem
With `HAUZ_GMAIL_*` set, the server polls Gmail itself on a fixed interval in a background
task, using the webhook's extractor and store. Bad configuration stops startup; a failing poll
is logged and retried next tick while the server keeps serving.

## Non-goals
- No Pub/Sub push, no multi-user, no persisted cursor or "last seen" state.
- No catch-up after downtime longer than the window (AC3): `hauz fetch --after` backfills.
- No logging crate, no metrics, no HTTP route to trigger or inspect the poller.

## Assumptions
- no spike: only composes `mail::fetch` (#42, spiked) with documented tokio timers/spawn.
- Design call — **window**: each tick queries `label:<label> after:<UTC midnight of
  (now's UTC day − 1)>`, which covers yesterday and today. `fetch` downloads every listed message
  before the hash short-circuit, so a whole-label poll re-downloads everything; the one-day
  overlap catches mail near UTC midnight.
- Design call — **interval config** lives on `mail::Config` (it owns `HAUZ_GMAIL_*`).
  `HAUZ_GMAIL_POLL_SECS` is optional (default 1800, i.e. 30 min) and must be an integer ≥ 1. Zero is
  rejected because `tokio::time::interval` panics on a zero period. The `Error::Config`
  message becomes `missing or invalid configuration: {variable}`, matching `llm::Error::Config`.
- Design call — **logging** is an injected `FnMut(String)` sink: `main.rs` passes `eprintln!`
  and tests capture the lines. Each tick logs exactly one summary line. On success it is
  `gmail poll: <n> messages` (with counts per outcome), followed by one `gmail poll: failed <id>:
  <error>` line per recorded per-message failure. On an aborted tick it is `gmail poll failed: <error>`.
- Design call — ticks are sequential (one task, `MissedTickBehavior::Delay`), and the first
  tick runs immediately at spawn. A webhook/poll race is safe: `insert` reports `Duplicate`.
- `crates/server` gains `time` as a normal dependency (it is a dev-dependency today) and the `time`
  feature on its `tokio`. Both are already locked, so no new crate and no root `Cargo.toml` change.

## Architecture delta
- `core::mail` (PROMOTES: mail):
  - `Config::from_env` also reads `HAUZ_GMAIL_POLL_SECS`.
  - `Config::poll_interval(&self) -> std::time::Duration`.
  - `Config::poll_query(&self, now: time::OffsetDateTime) -> String` = `query` with `after` =
    `now`'s UTC date − 1 day, `before` unset.
- `crates/server` lib: `pub struct Poller` built by
  `Poller::new(source: Arc<dyn MailSource>, config: mail::Config, extractor: Arc<dyn Extractor>,
  store: Arc<dyn BillStore>)`. Its methods:
  - `async fn tick(&self, now: OffsetDateTime) -> Result<Vec<Fetched>, mail::Error>` runs
    `mail::fetch` with `config.poll_query(now)`.
  - `fn spawn(self, interval: Duration, log: impl FnMut(String) + Send + 'static) ->
    tokio::task::JoinHandle<()>` loops forever: tick at `OffsetDateTime::now_utc()`, log, wait.

  `Debug` redacts via `Config`.
- `main.rs`: when `mail::Config::from_env` returns `Some`, it builds `config.source()` and spawns a
  `Poller` with `config.poll_interval()`, sharing the webhook's `Arc` store and extractor. `Err` aborts startup.
- Docs: `mail` bullet, server entry point, Planned; a `docs/decisions.md` line.

## Test plan
- AC1 [unit] WHEN Gmail is configured and `HAUZ_GMAIL_POLL_SECS` is unset THE SYSTEM SHALL
  report `poll_interval()` = 1800 s, and WHEN it is `"45"` THE SYSTEM SHALL report 45 s.
  (`unit_mail.rs`, `ac1_poll_interval_defaults_to_1800_and_reads_env`)
- AC2 [unit] WHEN Gmail is configured and `HAUZ_GMAIL_POLL_SECS` is `"0"`, `"-5"`, `"abc"` or
  `""` THE SYSTEM SHALL return `Err(Error::Config { variable: "HAUZ_GMAIL_POLL_SECS" })`.
  (`ac2_rejects_invalid_poll_secs`)
- AC3 [unit] WHEN `poll_query` is called with `now` = 2026-10-07T00:30:00+05:00 (that is
  2026-10-06T19:30Z) and label `bills` THE SYSTEM SHALL return `label:bills after:1791158400`,
  which is 2026-10-05T00:00Z. (`ac3_poll_query_covers_yesterday_and_today_utc`)
- AC4 [e2e] WHEN `Poller::tick` runs against a fake `MailSource` that lists one valid bill
  email THE SYSTEM SHALL return one `Fetched` with `Ok(Outcome::Created(_))` and the bill is in the
  store. The source received exactly `config.poll_query(now)`. A second tick returns
  `Duplicate` for the same id. (`crates/server/tests/e2e_poll.rs`, `ac4_tick_ingests_listed_mail`)
- AC5 [e2e] WHEN a spawned `Poller` (10 ms interval) has a source whose first `list` call
  fails with `Error::Transport` THE SYSTEM SHALL log a line starting `gmail poll failed:`
  that contains the error's `Display`. The task SHALL still be running, and a later tick SHALL
  store the bill, both within a 5 s test timeout. (`ac5_failed_tick_is_logged_and_retried`)
- AC6 [e2e] WHEN a spawned `Poller` lists one valid and one malformed message THE SYSTEM
  SHALL store the valid bill and log one `gmail poll: failed <id>:` line for the malformed id, and
  the router built from the same store SHALL answer `GET /v1/bills/{id}` 200 for the stored bill.
  (`ac6_per_message_failure_is_logged_and_server_serves`)

<!-- GATE 1 CHECKLIST (Leader self-check, before labelling `approved`):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [x] no [unverified] assumption is load-bearing; spike questions answered or carried
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens (else split into two issues)
     [x] ≤800 words -->
