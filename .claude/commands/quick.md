# /quick <issue# | description> — Trivial change, no subagents

You (the Leader, main session) implement this yourself. Merge is the only gate, so the
eligibility guard is what keeps `/quick` from swallowing `/feature`.

1. **Eligibility** — all must hold, otherwise say "this is a /feature" and stop:
   - no new or changed `pub` item in `crates/core/src/**`
   - no new dependency, no root `Cargo.toml` change
   - one EARS criterion describes it; diff will be ≲80 lines
2. **Branch:** `git switch -c quick/<slug>` off an up-to-date `main`.
3. **Red:** write one test `fn <slug>_<behavior>()` in the right `tests/<level>_*.rs`
   file (constitution › Test levels); run it focused; it must fail for the right reason.
4. **Green:** minimal change; focused run passes.
5. `cargo fmt --all`, then `bash .claude/scripts/verify.sh` — all green.
6. **PR:** commit, `git push -u origin quick/<slug>`,
   `gh pr create --base main --head quick/<slug> --title "fix: <slug>" --body "<the EARS
   criterion>\n\nTest: <test fn name>\n\nCloses #<n>"` (omit `Closes` if no issue).
   **STOP.** The human merges.

If any step reveals hidden scope (a type must change, a second module is involved, the
test needs a mock), stop, revert to `main`, and say it is a `/feature`.
