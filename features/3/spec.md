# Spec: Email envelope: parse raw RFC 5322 message into body + Documents (#3)

## Problem
`hauz-core` cannot read an email. This adds the `email` module: `Envelope::parse(&[u8])`
turns a raw RFC 5322 message into a plain record — subject, sender address, date, text
body, HTML body — plus one `Document` (mime type, filename, decoded bytes) per attachment,
however deeply the multipart tree nests. Malformed input is an `Err`, never a panic.

## Non-goals
- No recursion into `message/rfc822` parts: a forwarded message is one `Document`
  (`message/rfc822`, raw bytes) the caller may parse again.
- No size limits, charset policy, or HTML sanitising. No `To`/`Cc`/`Message-ID`/headers
  map. No serde on the new types. No mime vocabulary (`is_pdf()`): `extract` owns that.
- No new dependencies: `mail-parser` is workspace-pinned (default features).

## Assumptions
- no spike: the only open questions were mail-parser 0.11.9 API facts, settled by
  reading the vendored source.
- [source-verified] `MessageParser::default().parse(bytes) -> Option<Message>` never
  panics (fuzzed + MIRI upstream); `None` only when no header section exists.
- [source-verified] `Message::attachments()` is a flat, document-order walk of every
  non-body part across nested multiparts, nested `message/rfc822` included; base64 and
  quoted-printable are decoded; `MessagePart::contents()` is the decoded bytes (raw
  bytes for a nested message).
- [source-verified] `body_text(0)` is the first text part, or the first HTML part
  converted to text when no `text/plain` exists; `body_html(0)` the first HTML part.
- [source-verified] `ContentType::ctype()/subtype()`; a part without `Content-Type` is
  `text/plain`. `Address::first().address()` is the addr-spec; `DateTime::is_valid()` +
  `to_timestamp()` give a UTC unix time.

## Architecture delta
- New `pub mod email`; `crates/core/Cargo.toml` gains `mail-parser = { workspace = true }`.
- Public surface (all types `Debug, Clone, PartialEq, Eq`):
  - `Error` (`thiserror`, `#[non_exhaustive]`): `Malformed` (no header section),
    `MissingSender` (no addr-spec in `From`, then `Sender`), `InvalidMimeType(String)`.
  - `MimeType`: newtype over a lowercase `type/subtype` string. `new(&str) ->
    Result<Self, Error>` (exactly one `/`, both halves non-empty, no parameters, ASCII
    lowercased), `as_str()`, `Display`, `Hash`, `TryFrom<String>`.
  - `Document { pub mime: MimeType, pub filename: Option<String>, pub bytes: Vec<u8> }`.
  - `Envelope { pub subject: Option<String>, pub sender: String, pub date:
    Option<time::OffsetDateTime>, pub text: Option<String>, pub html: Option<String>,
    pub documents: Vec<Document> }` — pub fields, like `BillDraft`, so other modules'
    tests can build one by hand.
  - `impl Envelope { pub fn parse(raw: &[u8]) -> Result<Self, Error> }`.
- Mapping: `sender` = first addr-spec of `From`, else `Sender`; `date` = `Some` only
  for a valid `Date`, as the UTC instant; `documents` = every `attachments()` part in
  order; `mime` from the part's `Content-Type` (default `text/plain`; a value
  `MimeType::new` rejects ⇒ `application/octet-stream`); `filename` =
  `attachment_name()`; `bytes` = `contents()`.
- `PROMOTES: email` → rewrite the `email` line in `docs/architecture.md`; one
  `docs/decisions.md` line: `MimeType` newtype over an enum, pub-field records.

## Test plan
Files: `tests/unit_email.rs`, `tests/property_email.rs`. Hand-written fixtures under
`tests/fixtures/` loaded with `include_bytes!`: `plain.eml`, `html_pdf.eml`,
`two_attachments.eml`, `malformed.eml`, `no_sender.eml`. Attachment payloads are small
known byte strings (e.g. `%PDF-1.4 fake`) so tests assert exact bytes.
- AC1 [unit] WHEN `plain.eml` (single-part `text/plain` with `From`, `Subject`, `Date`)
  is parsed THE SYSTEM SHALL return that subject, the bare addr-spec as `sender`, the
  `Date` as a UTC instant, the body as `text`, `html == None`, `documents` empty.
- AC2 [unit] WHEN `html_pdf.eml` (`multipart/mixed`: `multipart/alternative` {text,
  html} + base64 `application/pdf` named `invoice.pdf`) is parsed THE SYSTEM SHALL return
  both bodies and exactly one `Document` with mime `application/pdf`, filename
  `Some("invoice.pdf")`, bytes equal to the decoded payload.
- AC3 [unit] WHEN `two_attachments.eml` (HTML-only body; nested `multipart/mixed` holding
  a base64 PDF and a quoted-printable `text/csv`; an `image/png` part with no filename)
  is parsed THE SYSTEM SHALL return `html == Some`, `text == Some` (derived from the
  HTML), and three `Document`s in document order with decoded bytes, the PNG's filename
  `None`.
- AC4 [unit] WHEN `malformed.eml` (bytes with no header section) is parsed THE SYSTEM
  SHALL return `Err(Error::Malformed)`.
- AC5 [unit] WHEN `no_sender.eml` (valid headers, no `From`/`Sender` addr-spec) is parsed
  THE SYSTEM SHALL return `Err(Error::MissingSender)`.
- AC6 [unit] WHEN a message (inline bytes) has no `Subject` and an unparsable `Date` THE
  SYSTEM SHALL return `Ok` with `subject == None` and `date == None`.
- AC7 [unit] WHEN `MimeType::new` receives `"Application/PDF"` THE SYSTEM SHALL yield
  `as_str() == "application/pdf"`; WHEN it receives `""`, `"pdf"`, `"/pdf"`, `"a/b/c"`
  or `"text/plain; charset=utf-8"` THE SYSTEM SHALL return `Err(Error::InvalidMimeType)`.
- AC8 [property] FOR ALL `Vec<u8>` THE SYSTEM SHALL return from `Envelope::parse`
  (`Ok` or `Err`) without panicking.
- AC9 [property] FOR ALL bodies `b: Vec<u8>` appended after a fixed valid header block
  (`From`, `Subject`, `Content-Type: multipart/mixed; boundary=...`) THE SYSTEM SHALL
  return `Ok` with that `sender`.

<!-- GATE 1 CHECKLIST: [x] EARS, tagged, pub-only [x] no cross-module, no entry point
     [x] no [unverified] assumption; no spike, reason stated [x] non-goals exclude creep
     [x] binaries → core, PROMOTES [x] fits ~15k implementer context [x] ≤800 words -->
