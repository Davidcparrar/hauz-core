# Maintaining these docs

*Verified against: `unaudited`*

The source of the site is in `docs/site/` (mdBook). To build the site on your computer, do these steps:
```
cargo install --locked mdbook@0.5.4 mdbook-mermaid@0.17.1
mdbook serve docs/site          # http://localhost:3000, rebuilds on save
mdbook build docs/site          # static HTML in docs/site/book/ (git-ignored)
```

## Updates in each PR
- **`docs-keeper` agent:** this agent runs in `/feature` step 8. It also runs in `/quick`
  when a public interface changes. It runs *before* the PR opens. It reads the spec and the
  diff and changes only `docs/site/**`. It writes in ASD-STE100 Simplified Technical
  English. It gives the list of changed pages, or `NO-DOCS-CHANGE: <reason>`. Its changes
  are in the PR that the owner merges.
- **Drift checks:** `verify.sh` and CI run `.claude/scripts/docs-check.sh`. The script
  makes sure that:
  1. the book builds (mdbook and mermaid);
  2. each relative `.md` link goes to a file that exists;
  3. the usage text of each binary is the same as the text in the [CLI reference](reference/cli.md);
  4. each environment variable that the code reads is in the [Environment reference](setup/env.md);
  5. each HTTP route is in the [HTTP API](reference/api.md) page or the
     [Web admin](reference/web.md) page;
  6. each page has a `Verified against` line.

  `docs-check.sh --selftest` breaks a copy of the site for each check. It makes sure that
  each check finds its error.

## Audit on request
`/docs-audit` compares each page with the code at `HEAD`. It examines the routes, the CLI
usage, the environment variables, the pub items on the pages and the diagram steps. It
writes the differences in `docs/site-audit.md`. It does not correct a page without a report.

Only the audit changes the `Verified against` line of a page. The line shows the commit that
the audit examined. To find a page that changed after its audit, run
`git log <sha>.. -- <page>`.

## Style
The text of this site obeys ASD-STE100 Simplified Technical English. The `docs-keeper` agent
applies these rules:
- A procedural sentence has a maximum of 20 words. A descriptive sentence has a maximum of 25
  words. A paragraph has a maximum of 6 sentences.
- Write an instruction as a command, one instruction in each sentence.
- Use the active voice. Use a word with one meaning only.
- Put a safety statement before its step, as **WARNING** (risk to persons or data) or
  **CAUTION** (risk to equipment). Use **NOTE** only for information.

Code, commands, identifiers, diagrams and table cells keep their technical names.
