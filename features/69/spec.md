# Spec: Project documentation site + docs-keeper agent (#69)
<!-- ≤800 words, verify-enforced. Drafted and self-checked by the Leader (Gate 1); the human
     reads it in the PR (Gate 2). Design calls go under Assumptions so they are visible there. -->

## Problem
Human docs are README prose plus agent-facing `docs/architecture.md`: no diagrams, nothing
checkable against code. Step 1 of 2 (owner's split): an mdBook site documenting interfaces,
drift checks in `verify.sh`, a `docs-keeper` agent updating the site inside each PR, and
`/docs-audit`. Step 2 (#70) runs the first audit, syncs content, stamps pages.

## Non-goals
- Hosting or a publish workflow (GitHub Pages or otherwise): the site will be hosted
  privately with the deployment (#22); until then `mdbook serve docs/site`.
- Internals: no C4 L4, no private items, no per-function docs (rustdoc covers that).
- Stamping pages as verified, or running `/docs-audit` (#70).
- Any change under `crates/**`; replacing `docs/architecture.md`.
- Link checking of external URLs or anchors.

## Assumptions
- [verified] mdbook 0.5.4 + mdbook-mermaid 0.17.1 (`cargo install --locked`) build mermaid
  blocks with committed, offline JS; a missing preprocessor fails the build.
- [verified] `hauz`, `hauz-tui` and `hauz-web` each print exactly their `USAGE` on stderr
  with exit 2 for an unknown flag (`--nope`): no crate change needed.
- Design: C4 uses Mermaid `flowchart` with C4 styling, not experimental `C4Context`
  (uncontrollable layout).
- Design: `docs/architecture.md` stays the agents' source; `docs-keeper` keeps the site
  consistent with it and the code.
- Design: each page carries `*Verified against: <sha|unaudited>*`, set only by
  `/docs-audit` (later edits show in `git log <sha>.. -- page`).
- Design: drift checks are bash (`.claude/scripts/docs-check.sh`, Leader-owned harness)
  with `--selftest` proving each fails on a mutated copy. No Rust change ⇒ no implementer
  step; the reviewer still reviews.
- Design: CI installs the two pinned tools with `cargo install --locked` (cached bin).
- Design: README's CLI walkthrough is replaced by a link to the site's CLI reference.
- no spike: the two verified facts were checked directly by the Leader.

## Reference implementation
Omitted (no spike).

## Architecture delta
- No crate change. New: `docs/site/` (`book/` ignored), `.claude/scripts/docs-check.sh`
  (run by `verify.sh`), `docs-keeper` agent, `/docs-audit` command; wired into
  `feature.md` step 8, `quick.md`, `CLAUDE.md`; CI installs the tools; decision recorded.
- Pages: Overview · Setup (build, env reference, Gmail, LLM, running binaries) · C4
  L1/L2/L3 · Sequences (webhook ingest, ingest/`--reextract`, fetch + poller, gmail-auth,
  web request) · Flows (extraction, idempotency, auth) · CLI · HTTP API · Web admin.
- Not PROMOTES (no pub interface or crate boundary change).

## Test plan
- AC1 [e2e] WHEN `verify.sh` runs THE SYSTEM SHALL build `docs/site` with mdbook and fail
  verify if the build fails or a tool is missing (`ac1_site_builds`).
- AC2 [e2e] WHEN a relative `.md` link in `docs/site/src` (including `SUMMARY.md`) names a
  file that does not exist THE SYSTEM SHALL fail, naming the page and link
  (`ac2_links_resolve`).
- AC3 [e2e] WHEN the fenced block after `<!-- usage:<bin> -->` on the CLI reference differs
  from that binary's usage output, for each of `hauz`, `hauz-tui`, `hauz-web`, THE SYSTEM
  SHALL fail, naming the binary (`ac3_usage_matches`).
- AC4 [e2e] WHEN an upper-case string literal containing `_` in `crates/*/src` (an env var
  name) is absent from the env reference page THE SYSTEM SHALL fail, naming it
  (`ac4_env_documented`).
- AC5 [e2e] WHEN a route path registered in `crates/server/src` (`.route("…")`) or
  `crates/web/src` (`#[route(METHOD "…")]`) is absent from the API or web reference page
  THE SYSTEM SHALL fail, naming it (`ac5_routes_documented`).
- AC6 [e2e] WHEN a page in `docs/site/src` other than `SUMMARY.md` lacks a
  `*Verified against: …*` line THE SYSTEM SHALL fail, naming the page
  (`ac6_pages_stamped`).
- AC7 [e2e] WHEN any check above evaluates zero items THE SYSTEM SHALL fail rather than
  pass (`ac7_nonempty`), and `docs-check.sh --selftest` SHALL exit 0 only if every check
  fails on its mutated copy and passes on the real site.
- AC8 [e2e] WHEN `/feature` reaches step 8, or `/quick` changes a public interface, THE
  SYSTEM SHALL spawn `docs-keeper` (input: spec + `git diff main...HEAD`; writes only
  `docs/site/**`; returns changed pages or `NO-DOCS-CHANGE: <reason>`) before the PR, and
  `/docs-audit` SHALL write a divergence report to `docs/site-audit.md` without editing
  any page except stamps (checked by review of the agent and command files).

<!-- GATE 1 CHECKLIST (Leader self-check, before labelling `approved`):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [x] no [unverified] assumption is load-bearing; spike questions answered or carried
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens (else split into two issues)
     [x] ≤800 words -->
