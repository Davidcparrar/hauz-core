# PRD (draft): hauz web frontend
<!-- DRAFT (2026-10-12), captured from the owner's description, not yet worked through.
     This file is the persistent record of the frontend roadmap: issues close, this stays.
     When a part is filed as a `feature` issue, link the issue number next to it; when it ships,
     mark it done. Decisions still go to docs/decisions.md, interfaces to docs/architecture.md. -->

## Goal
A minimal web frontend for hauz built on [Topcoat](https://tokio.rs/blog/2026-07-22-announcing-topcoat)
(tokio-rs, server-rendered Rust). It grows from a read-only admin browser into the main way to
work with bills and, later, to talk to agents.

## Surfaces
- **Admin view** (`/admin`): everything the operator needs.
  - Bill browser, the web analogue of `hauz-tui` — **#63, first step**.
  - File-style browser for stored bills and their source documents, with scopes; storage is
    local first, S3 later.
- **User view** (`/app`): what a signed-in user sees of their own bills.
  - A ChatGPT-like interface to upload PDFs and bills, and later to chat with agents.

## Identity and access
- Login, with SSO.
- OTP as an authentication factor.
- Bearer tokens with **scopes** for both authentication and authorization, used by the web
  views and the API alike. They replace today's single `HAUZ_API_TOKEN`.
- Scopes also gate the file browser.

## Cross-cutting
- **Branding:** all colors, fonts, radii and spacing are design tokens (CSS custom properties)
  in one theme file. Components never use literal values, so a brand is a token swap. Light and
  dark themes come from the same tokens. #63 sets this up.
- **i18n:** every user-facing string goes through translation. The first locales are TBD
  (the owner works in Spanish and English; the bills are Colombian).

## Sequencing (tentative)
1. #63 — read-only admin bill browser; trials Topcoat and the CSS/token pipeline.
2. Auth foundation: scoped bearer tokens, then login and OTP, then SSO.
3. Upload (PDF / `.eml`) into the existing ingest pipeline.
4. User view and chat UI; agents later.
5. File-style admin; S3 storage.
6. i18n (could move earlier, since retrofitting strings is costly).
7. Branding: apply a real brand to the tokens.

Deploy (#22) is paused while the frontend starts.

## Open questions (to deep-dive before filing)
- Which SSO providers? Is OTP via TOTP app or email?
- Multi-user data model: today a bill has no owner.
- Which agents, and what can they do (read only, or act)?
- How does S3 layout relate to the Litestream/Spaces plan in #22?
- When does i18n land? Before the user view, to avoid retrofitting?
