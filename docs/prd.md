# PRD (draft): hauz web frontend and roadmap
<!-- DRAFT (2026-10-12), captured from the owner's description and the #45 parking lot, not yet
     worked through. This file is the persistent record of the roadmap: issues close, this stays.
     When a part is filed as a `feature` issue, link the issue number next to it; when it ships,
     mark it done. Decisions still go to docs/decisions.md, interfaces to docs/architecture.md. -->

## Goal
A minimal web frontend for hauz built on [Topcoat](https://tokio.rs/blog/2026-07-22-announcing-topcoat)
(tokio-rs, server-rendered Rust). It grows from a read-only admin browser into the main way to
work with bills and, later, to talk to agents.

## Surfaces
- **Admin view** (`/admin`): everything the operator needs.
  - Bill browser, the web analogue of `hauz-tui` — **#63, done**.
  - File-style browser for stored bills and their source documents, with scopes; storage is
    local first, S3 later.
- **User view** (`/app`): what a signed-in user sees of their own bills.
  - A ChatGPT-like interface to upload PDFs and bills, and later to chat with agents.

## Server API (from #45)
- **Bill listing:** `GET /v1/bills` with a status filter and paging — **#66**. It is the
  read base for everything below.
- **Corrections:** an HTTP write path that corrects fields of a `needs_review` bill and marks
  it reviewed. The server stays the single writer.
- **TUI edits and remote mode:** the TUI corrects bills through that endpoint, and gains a mode
  that talks to the deployed server instead of a local file.

## Identity and access
- Login, with SSO.
  - **Google SSO (from #45):** a separate OAuth client in the same Google Cloud project, with
    `openid email` scopes only. It replaces the shared API token for humans.
- OTP as an authentication factor.
- Bearer tokens with **scopes** for both authentication and authorization, used by the web
  views and the API alike. They replace today's single `HAUZ_API_TOKEN`.
- Scopes also gate the file browser and the corrections endpoint.

## Mail ingest beyond the owner (from #45)
- **Gmail push:** `users.watch` + Pub/Sub instead of the poll (#43).
- **Per-user inbound address — the planned main path for other users.** An address such as
  `bills+<user>@hauz.offthearc.dev` receives mail through Amazon SES: an MX record in the
  existing Route 53 zone, SES stores to S3, then ingest.
  - **Why:** it works with any mail provider and avoids Google's verification and yearly
    security assessment for the restricted Gmail scope.
  - **Costs:** it sees new mail only, with no backfill. Gmail auto-forwarding asks the user to
    confirm with a code sent to that address, so hauz must show them that code.
  - The owner keeps the Gmail API path (#42).
- **Multi-user Gmail:** per-user consent through the SSO screen. It needs Google verification and
  a security assessment for the restricted scope, so it ranks behind the inbound address.

## Cross-cutting
- **Branding:** all colors, fonts, radii and spacing are design tokens (CSS custom properties)
  in one theme file. Components never use literal values, so a brand is a token swap. Light and
  dark themes come from the same tokens. #63 set this up (`crates/web/src/theme.css`).
- **i18n:** every user-facing string goes through translation. The first locales are TBD
  (the owner works in Spanish and English; the bills are Colombian).

## Sequencing (tentative)
1. #63 — read-only admin bill browser; trials Topcoat and the CSS/token pipeline. **Done.**
2. #66 — `GET /v1/bills`.
3. Corrections over HTTP, then TUI edits and remote mode.
4. Auth foundation: scoped bearer tokens, then login and OTP, then SSO.
5. Upload (PDF / `.eml`) into the existing ingest pipeline.
6. User view and chat UI; agents later.
7. File-style admin; S3 storage.
8. Other users' mail: inbound SES address; Gmail push; multi-user Gmail last.
9. i18n (could move earlier, since retrofitting strings is costly).
10. Branding: apply a real brand to the tokens.

Deploy (#22) is paused while the frontend starts. The SES and push items need it.

## Open questions (to deep-dive before filing)
- Which SSO providers beyond Google? Is OTP via TOTP app or email?
- Multi-user data model: today a bill has no owner. The inbound address implies one per user.
- Can corrections ship under the single API token, or must scopes come first?
- Which agents, and what can they do (read only, or act)?
- How does the S3 layout (files, SES inbound) relate to the Litestream/Spaces plan in #22?
- When does i18n land? Before the user view, to avoid retrofitting?
