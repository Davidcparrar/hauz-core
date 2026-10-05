# Spec: ci: run verify.sh on pull requests (#44)

## Problem
`verify.sh` runs only on the author's machine, against whatever toolchain is installed there
(today 1.99, while the workspace declares `rust-version = "1.95"`). A PR can therefore land
red, or depend on a compiler newer than the declared MSRV, with nothing on GitHub saying so.
This feature adds one GitHub Actions workflow that runs `.claude/scripts/verify.sh` on every
pull request and on every push to `main`, on the pinned toolchain, with cargo caching, so the
human sees a green or red check next to the merge button (Gate 2).

## Non-goals
- No branch protection or required-check settings (repo settings, the human's call).
- No release, deploy, coverage, audit, or multi-OS / multi-toolchain matrix.
- No `pdftoppm`, Ollama, network-facing, or secret-using job: those tests self-skip
  (`integration_llm` checks `PATH` and `HAUZ_LIVE_OLLAMA`).
- No `cargo-nextest` install: `verify.sh` falls back to `cargo test`, matching the local run.
- No change to `verify.sh` or anything under `crates/**`.

## Assumptions
- no spike: the open questions are about GitHub's runner, and this PR's own check run is
  the experiment. The PR can only be opened once that check is green.
- Owned path: `.github/workflows/verify.yml`, written by the Leader (the issue allows it; the
  `implementer-rust` agent's boundary is `crates/**` and it does Rust TDD, which does not fit
  here). The reviewer still reviews it.
- Design call: the toolchain version is **read from `Cargo.toml`'s `rust-version`** at run
  time, then `rustup toolchain install <v> --profile minimal -c rustfmt,clippy` and
  `rustup default <v>`. Cargo.toml is the only place the version lives, and the workflow
  needs no third-party toolchain action.
- Design call: caching uses `Swatinem/rust-cache@v2` (the de-facto standard; it keys on
  OS, rustc version, and `Cargo.lock`). This is a CI action, not a crate, so it needs no
  `docs/decisions.md` dependency line. It is recorded as decision #44 anyway.
- Design call: triggers are `pull_request` (any base branch) and `push` to `main`; a
  `concurrency` group per ref, with `cancel-in-progress` only for pull requests;
  `permissions: contents: read`; runner `ubuntu-latest`; job timeout 30 min.
- Design call: `CARGO_TERM_COLOR: always` and `RUSTFLAGS` left unset. Setting RUSTFLAGS would
  change the cache key and diverge from the local run. `-D warnings` already comes from
  `verify.sh`'s clippy step.
- [unverified, not load-bearing] Clippy 1.95 knows fewer lints than the local 1.99, so CI may
  be laxer than local clippy, never stricter. Fixing that drift would need a bump, which is
  out of scope.
- No sqlx compile-time macros exist (`grep query!` is empty), so no `DATABASE_URL`/`.sqlx`
  is needed. `sqlx-sqlite` bundles SQLite and rig uses rustls, so no system packages.

## Reference implementation
Omitted (no spike).

## Architecture delta
New entry point outside the crate graph: `.github/workflows/verify.yml`. No change to
`core`, no pub interface, and no new crate dependency, so no `PROMOTES`. Leader adds a
one-line `docs/decisions.md` entry (#44: CI = verify.sh, toolchain from `rust-version`,
rust-cache); `docs/architecture.md` is a crate/module map (and at 993/1000 words), so CI
is not added there.

## Test plan
<!-- Not Rust: the constitution's test levels do not apply. The level here is [ci]. Evidence
     is GitHub check runs, named in review.md, not a test fn. -->
- AC1 [ci] WHEN a pull request is opened or updated THE SYSTEM SHALL run a `verify` job that
  executes `.claude/scripts/verify.sh` from the repository root and report it as a check on
  the PR. Evidence: this PR's check run, green.
- AC2 [ci] WHEN the `verify` job runs THE SYSTEM SHALL use the toolchain equal to
  `Cargo.toml`'s `rust-version` (with rustfmt and clippy), and log `rustc --version`.
  Evidence: the job log shows `rustc 1.95.x`.
- AC3 [ci] WHEN `verify.sh` exits non-zero THE SYSTEM SHALL fail the job. Evidence: the step
  is `run: .claude/scripts/verify.sh` with the default `bash -e` shell, with no
  `continue-on-error` and no `|| true`. The reviewer checks this statically.
- AC4 [ci] WHEN a commit is pushed to `main` THE SYSTEM SHALL run the same job. Evidence:
  the `on.push.branches: [main]` trigger, checked statically; observable after merge.
- AC5 [ci] WHEN the job runs a second time on an unchanged `Cargo.lock` THE SYSTEM SHALL
  restore the cargo cache. Evidence: the rust-cache step logs a cache hit on the PR's
  second run.

<!-- GATE 1 CHECKLIST (Leader self-check, before labelling `approved`):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [x] no [unverified] assumption is load-bearing; spike questions answered or carried
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens (else split into two issues)
     [x] ≤800 words -->
