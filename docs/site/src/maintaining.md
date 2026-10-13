# Maintaining these docs

*Verified against: `unaudited`*

The site lives in `docs/site/` (mdBook). Build or preview it locally:
```
cargo install --locked mdbook@0.5.4 mdbook-mermaid@0.17.1
mdbook serve docs/site          # http://localhost:3000, rebuilds on save
mdbook build docs/site          # static HTML in docs/site/book/ (git-ignored)
```

## Kept in step with every PR
- **`docs-keeper` agent:** runs in `/feature` step 8 and in `/quick` when a public
  interface changes, *before* the PR opens. It reads the spec and the diff, edits only
  `docs/site/**`, and returns the pages it changed or `NO-DOCS-CHANGE: <reason>`. Its
  changes ship inside the PR the owner merges.
- **Drift checks** (`.claude/scripts/docs-check.sh`, run by `verify.sh` and CI):
  1. the book builds (mdbook + mermaid);
  2. every relative `.md` link resolves;
  3. each binary's usage text matches the [CLI reference](reference/cli.md);
  4. every environment variable the code reads is in the [Environment reference](setup/env.md);
  5. every registered HTTP route is in the [HTTP API](reference/api.md) or
     [Web admin](reference/web.md) page;
  6. every page has a `Verified against` line.

  `docs-check.sh --selftest` proves each check fails on a deliberately broken copy.

## Audited on demand
`/docs-audit` compares every page with the code at `HEAD` (routes, CLI usage, environment
variables, pub items named on pages, diagram steps). It writes a divergence report to
`docs/site-audit.md` and never fixes content silently. Only the audit sets a page's
`Verified against` line, to the commit it checked. A page edited later shows up in
`git log <sha>.. -- <page>`.
