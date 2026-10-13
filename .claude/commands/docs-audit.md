# /docs-audit — Compare the docs site with the code, report divergence

You are the Leader. The audit finds where `docs/site/` no longer matches the code. It never
fixes a page silently: findings go to a report, and fixes go through a reviewed PR.

1. **Branch.** `git switch -c docs/audit-<YYYY-MM-DD>` off an up-to-date `main`. Note
   `sha=$(git rev-parse --short HEAD)`.
2. **Mechanical pass.** `bash .claude/scripts/docs-check.sh`. Any failure is a finding.
   Copy it into the report.
3. **Content pass.** Spawn `docs-keeper` in **audit mode** (delegation contract: objective =
   audit `docs/site` against `<sha>`; output = `docs/site-audit.md`; files = the site pages and
   the code they describe; boundaries = writes only `docs/site-audit.md`; budget ~40 calls).
   Accept only a ≤300-token return ending `<!-- STATUS: COMPLETE -->`.
4. **Report.** Read `docs/site-audit.md`. Add the mechanical findings. Commit it.
5. **Resolve.** Only when the human asked for the audit *and* its fixes (as in a sync issue),
   fix each finding in the page (or file an issue when the code is wrong rather than the
   page). Then set `*Verified against: `<sha>`*` on every page with no open finding.
   Otherwise stop after the report.
6. **PR.** `bash .claude/scripts/verify.sh`, push, then `gh pr create --title "docs: site
   audit <date>"`. The body lists findings per page and what was fixed or carried. **STOP**:
   the human merges.

A stamp means "this page matched the code at `<sha>`". A page edited after its stamp shows
up in `git log <sha>.. -- <page>`. The next audit re-checks it.
