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
5. **Stamp.** Set `*Verified against: `<sha>`*` on every page the report marks `OK`. That
   line is the only page edit this command makes.
6. **Resolve (only when the human asked for fixes, as in a sync issue such as #70).** Spawn
   `docs-keeper` in update mode with the report as its input; it alone edits page content.
   File an issue instead when the code, not the page, is wrong. Re-audit the fixed pages and
   stamp them. Without that request, stop after step 5.
7. **PR.** `bash .claude/scripts/verify.sh`, push, then `gh pr create --title "docs: site
   audit <date>"`. The body lists findings per page and what was fixed or carried. **STOP**:
   the human merges.

A stamp means "this page matched the code at `<sha>`". A page edited after its stamp shows
up in `git log <sha>.. -- <page>`. The next audit re-checks it.
