# Spec: email+extract: vendor from the sender's display name (#39)

## Problem
The 2026-10-04 corpus replay stored every non-DIAN bill with the sender's domain as vendor
(`aws.com`, `github.com`, `clientescol.enel.com`, `stripe.com` for a DeepLearning.AI receipt).
The `From` display name usually names the real vendor ("Amazon Web Services", "GitHub",
"DeepLearning.AI"). `Envelope` will expose that display name, and `TextExtractor`'s
low-confidence vendor fallback will prefer it over the domain, so a document-sourced vendor
(`XmlInvoiceExtractor` at 100, the LLM) still wins the merge.

## Non-goals
- No vendor alias table or normalisation ("Enel Colombia" vs "Enel Colombia S.A. E.S.P.").
- No heuristic stripping of personal prefixes or suffixes ("<recipient>, Enel Colombia -
  Factura Digital" is stored as the vendor verbatim). A later issue if the data asks for it.
- No change to `Envelope::sender`, other extractors (LLM prompt untouched), `merge`,
  `ingest`, `store` or the binaries.
- No display name from `Reply-To` or other headers; no use of `subject` for the vendor.

## Assumptions
- no spike: mail-parser 0.11.9 already parses the display name (`Addr::name`) and decodes
  RFC 2047 encoded-words in address fields (read in `parsers/fields/address.rs`); the change
  is a field read plus a fallback rule, which prose settles.
- Design call: `sender_name` is the display name of the **same** address `sender` comes
  from (`From`, else `Sender`), so name and address never describe two different mailboxes.
- Design call: `sender_name` is trimmed; it is `None` when the address has no display name,
  when the trimmed name is empty, or when it equals `sender` ignoring ASCII case (a name that
  only repeats the address carries no vendor).
- Design call: the display-name vendor keeps the existing fallback confidence **20** and span
  `Source::Text 0..0`, as the domain fallback; XML (100) or an LLM vendor above 20 still
  wins `merge`.
- Design call: the name is passed through `Vendor::new` unchanged (beyond trimming); casing
  is kept ("DeepLearning.AI"), unlike the lowercased domain.
- Adding a pub field to `Envelope` (not `#[non_exhaustive]`) breaks struct literals in
  `crates/core/tests/*`; updating those literals with `sender_name: None` is in scope.

## Architecture delta
- `email`: `Envelope` gains `pub sender_name: Option<String>` (decoded, trimmed display name
  of the sender's address; rules above). No new error variant.
- `extract`: `TextExtractor`'s vendor = `sender_name` when `Some`, else the sender domain
  (unchanged); confidence 20 in both cases. Doc comment updated.
- `PROMOTES: email` — pub interface of `Envelope` changes → update `docs/architecture.md`
  (`email` and `TextExtractor` lines) and add a `docs/decisions.md` line.

## Test plan
Fixtures: synthetic `.eml` files under `crates/core/tests/fixtures/` with and without a
display name (one RFC 2047-encoded, e.g. `=?UTF-8?Q?Energ=C3=ADa_Acme?=`).
- AC1 [unit] WHEN `Envelope::parse` reads `From: "  Amazon Web Services " <billing@aws.example>`
  THE SYSTEM SHALL set `sender_name` to `Some("Amazon Web Services")` and `sender` to
  `billing@aws.example`.
- AC2 [unit] WHEN the `From` display name is an RFC 2047 encoded-word THE SYSTEM SHALL set
  `sender_name` to the decoded text (`"Energía Acme"`).
- AC3 [unit] WHEN `From` is a bare address, or its display name is empty/whitespace, or equals
  the address ignoring ASCII case (`"Billing@AWS.example" <billing@aws.example>`) THE SYSTEM
  SHALL set `sender_name` to `None`.
- AC4 [unit] WHEN `From` is absent and `Sender: GitHub <noreply@github.example>` is present
  THE SYSTEM SHALL set `sender` to `noreply@github.example` and `sender_name` to `Some("GitHub")`.
- AC5 [unit] WHEN `TextExtractor` runs on an envelope with `sender_name: Some("DeepLearning.AI")`
  and sender `invoice@stripe.example` THE SYSTEM SHALL return vendor `DeepLearning.AI` at
  confidence 20 with span `Text 0..0`.
- AC6 [unit] WHEN `TextExtractor` runs on an envelope with `sender_name: None` THE SYSTEM SHALL
  return the lowercased sender domain as vendor at confidence 20 (existing behaviour kept).
- AC7 [integration] WHEN the DIAN fixture message carries a `From` display name different from
  the invoice supplier and runs through `Chain([XmlInvoiceExtractor, TextExtractor])` THE
  SYSTEM SHALL return the XML supplier as vendor at confidence 100, not the display name.
- AC8 [integration] WHEN a display-name `.eml` with an anchored amount and issue date is
  ingested via `ingest` with `TextExtractor` and `InMemoryStore` THE SYSTEM SHALL store a bill
  whose vendor is the display name.
- AC9 [property] FOR ALL display names (printable strings without `"` or `\`, which would
  break the quoted-string header itself, possibly padded with spaces)
  placed in `From: "<name>" <a@b.example>` THE SYSTEM SHALL parse without error and yield a
  `sender_name` that is `None` or non-empty, trimmed, and not equal to `sender` ignoring case.

<!-- GATE 1 CHECKLIST (Leader self-check, before labelling `approved`):
     [x] every criterion is EARS-shaped, tagged, numbered, and names only pub behavior
     [x] required levels present (integration if cross-module, e2e if entry point: happy + failure)
     [x] no [unverified] assumption is load-bearing; spike questions answered or carried
     [x] non-goals actually exclude the creep this feature invites
     [x] delta respects binaries → core; PROMOTES present iff a pub interface changes
     [x] fits an implementer context of ~15k tokens (else split into two issues)
     [x] ≤800 words -->
