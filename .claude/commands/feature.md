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
   yourself from the issue, architecture and core's pub surface. Design calls you make
   go in Assumptions so the human sees them in the PR.
4. **GATE 1 (self-check).** Walk the checklist at the bottom of the template and fix the
   spec until every box holds. Do not ask the human and do not wait: the spec is reviewed
   in the PR. On a failing box route by class (CLAUDE.md › Rejection routing), return to 3.
5. **Record.** `git switch -c feat/<n>` (off an up-to-date `main`), commit the spec (and
   spike findings), `gh issue edit <n> --add-label approved`.
6. **Implement.** Spawn `implementer-rust` with the delegation contract (objective = this
   spec; files = spec, constitution, architecture; boundaries = `crates/**` only; budget).
   On `SPEC-CONFLICT` resolve it yourself: amend the spec (back to step 3/4), note the
   change for the PR body, re-spawn.
7. **Review.** Spawn `reviewer`. Append its verdict to `features/<n>/review.md`.
   REJECT `code-defect` → re-spawn the implementer with the REQUIRED CHANGES, ≤2 cycles
   (count `VERDICT:` lines), then stop and report. REJECT `spec-amendment` → amend the
   spec, re-walk the Gate-1 checklist, targeted re-implementation, and list the amendment
   in the PR body. The spec changes first; code never silently diverges.
8. **Docs delta.** If the spec says `PROMOTES`, update `docs/architecture.md` and append
   a `docs/decisions.md` line yourself, on the branch. Rerun `verify.sh` (budgets).
9. **PR.** Commit, `git push -u origin feat/<n>`, then
   `gh pr create --base main --head feat/<n> --title "feat: <title> (#<n>)" --body
   "Closes #<n>\n\nSpec: features/<n>/spec.md\nReview: features/<n>/review.md"`, plus a
   **Design calls** section listing the spec's Assumptions the human has not seen and any
   spec amendment made after Gate 1 — the PR is their only checkpoint.
   **STOP. Gate 2 is the human merging the PR.** You cannot merge (denied) and must not
   infer it. PR comments route per CLAUDE.md › Rejection routing (Gate 2). The issue
   closes itself on merge; there is no bookkeeping step.
