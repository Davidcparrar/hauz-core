# SDD Harness — Rust template

Spec-driven, test-first Claude Code harness with two human gates (spec approval, PR merge),
GitHub issues as the roadmap, and Rust rules enforced by lints instead of prose.
Simplified from [harness-sdd](https://github.com/Davidcparrar/harness-sdd): 2 agents,
3 commands, 1 script, no database, no worktrees, no hooks.

## Use
1. Copy this tree into a new repo (or `git clone` it and reset history).
2. Have the `tdd-rust` skill installed at `~/.claude/skills/tdd-rust/` — the implementer
   reads it instead of carrying a Rust overlay (the constitution + lints suffice without it). Optional:
   `cargo install cargo-nextest` (verify.sh picks it up).
3. Open Claude Code and run `/project-init` (renames `myproj-core`, adds binaries, creates
   the `feature`/`approved` labels, seeds issues).
4. `/feature <issue#>` per feature; `/quick <description>` for trivial changes.
   The pipeline and every rule: `CLAUDE.md`.

## Layout
```
CLAUDE.md                 Leader manual (the pipeline, gates, state table)
Cargo.toml / clippy.toml  workspace + the Rust rules as lints
crates/core/              library: all logic, tested via its pub API (tests/ only)
docs/                     constitution (≤500 words), architecture (≤500), decisions (1 line each)
features/<n>/             spec.md, review.md, spike/ — committed with the feature PR
.claude/agents/           implementer-rust (sonnet), reviewer (opus)
.claude/commands/         feature, quick, project-init
.claude/scripts/verify.sh fmt → clippy -D warnings → test → spike-leak grep → doc budgets
.claude/settings.json     allowlist (cargo/git/gh) + denylist (gh pr merge, force push)
```
