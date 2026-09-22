# /feature <issue#> — Run (or resume) the feature pipeline

$ARGUMENTS is the GitHub issue number `<n>`. You are the Leader. This command is
**idempotent**: step 0 derives where the feature is and you continue from the first
incomplete step. Never redo completed work; never skip a gate that is not recorded.

0. **Derive the phase** (CLAUDE.md › State table): `gh issue view <n> --json
   title,body,labels,state`; `ls features/<n>/`; `git branch --list feat/<n>`;
   `gh pr view feat/<n> --json state --jq .state 2>/dev/null || echo NONE`. Say in one line
   where the feature is, then continue there.
1. **Read the issue.** For every `Depends on: #k` in the body, `gh issue view k --json
   state` must be CLOSED; otherwise stop and say which dependency blocks.
2. **Spike (optional).** If you or the human name open questions that prose cannot settle,
   spawn `implementer-rust` in **spike mode** with those questions (delegation contract;
   budget ~20 calls). Done ⇔ `features/<n>/spike/findings.md` ends with the marker.
   If skipped, the spec's Assumptions must say `no spike: <reason>`.
3. **Spec.** Copy `features/_template/spec.md` to `features/<n>/spec.md` and draft it
   with the human in the room. Walk the Gate-1 checklist at the bottom of the template.
4. **GATE 1.** Ask for explicit approval. Silence, "looks fine", or moving on is not
   approval; only "approved" (or equivalent) is. On rejection route by class
   (CLAUDE.md › Rejection routing) and return to step 3.
5. **Record.** `git switch -c feat/<n>` (off an up-to-date `main`), commit the spec (and
   spike findings), `gh issue edit <n> --add-label approved`.
6. **Implement.** Spawn `implementer-rust` with the delegation contract (objective = this
   spec; files = spec, constitution, architecture; boundaries = `crates/**` only; budget).
   On `SPEC-CONFLICT` stop and take it to the human (back to step 3 if the spec changes).
7. **Review.** Spawn `reviewer`. Append its verdict to `features/<n>/review.md`.
   REJECT `code-defect` → re-spawn the implementer with the REQUIRED CHANGES, ≤2 cycles
   (count `VERDICT:` lines), then escalate to the human. REJECT `spec-amendment` → amend
   the spec with the human, `--remove-label approved`, re-approve, `--add-label approved`,
   targeted re-implementation. The spec changes first; code never silently diverges.
8. **Docs delta.** If the spec says `PROMOTES`, update `docs/architecture.md` and append
   a `docs/decisions.md` line yourself, on the branch. Rerun `verify.sh` (budgets).
9. **PR.** Commit, `git push -u origin feat/<n>`, then
   `gh pr create --base main --head feat/<n> --title "feat: <title> (#<n>)" --body
   "Closes #<n>\n\nSpec: features/<n>/spec.md\nReview: features/<n>/review.md"`.
   **STOP. Gate 2 is the human merging the PR.** You cannot merge (denied) and must not
   infer it. The issue closes itself on merge; there is no bookkeeping step.
