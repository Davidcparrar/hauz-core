# /project-init — One-time project layer (greenfield or brownfield)

Conversational, AskUserQuestion-driven; the human is in the room. The two hard gates live
in `/feature`. Done when `verify.sh` is green and the seed issues exist.

1. **Brownfield only** (code already exists): spawn 2–3 read-only Explore agents in ONE
   message on disjoint slices (entry points & flows / crate & module boundaries /
   conventions, tooling, tests). Persist each return to `docs/onboarding/agent-<n>.md`
   (gitignored scratch). Describe what IS; do not fix it.
2. **Architecture** — with the human, fill `docs/architecture.md`: purpose, crate map,
   modules of `core` (earned only when interface ≪ implementation and testable alone),
   entry points. ≤500 words.
3. **Constitution** — edit `docs/constitution.md` only where this project differs
   (allowed dependencies, extra levels, GPUI section if an app is planned). ≤500 words.
4. **Workspace** — rename `myproj-core` to `<project>-core` (`crates/core/Cargo.toml` and
   `use myproj_core` in `crates/core/tests/`); add the binaries the project needs, each
   with `[lints] workspace = true` and `main() -> anyhow::Result<()>`. A server is a lib
   exposing `pub fn router(state) -> axum::Router` plus a thin `main.rs` so `[e2e]` runs
   in-process. Run `bash .claude/scripts/verify.sh`.
5. **GitHub** — confirm `main` is the default branch (`gh repo view --json
   defaultBranchRef`; `Closes #n` only fires on the default branch). Create the labels
   once: `gh label create feature --color 1D76DB --description "harness feature" --force`
   and `gh label create approved --color 0E8A16 --description "Gate 1: spec approved"
   --force`.
6. **Seed issues** — one issue per feature sized to a single `/feature` pass:
   `gh issue create --label feature --title "<title>" --body "<one paragraph>\n\nDepends
   on: #k"` (one `Depends on:` line per blocker; producers before consumers). Show the
   human `gh issue list --label feature --json number,title,body` and adjust.
7. Commit docs + workspace on `main`.
