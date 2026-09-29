# Feature 5 spike findings

**Q1 — corrupt input.** (a) fake header, (b) empty, (c) 64 garbage bytes ->
`Err(PdfError(Parse(InvalidFileHeader)))`. (d) valid PDF truncated at 50%/90% ->
`Err(PdfError(Xref(Start)))`. (e) `startxref` offset corrupted -> same. (f) font
resource removed from `/Resources` while the content stream still does
`/F1 12 Tf` -> **PANIC** `"Font"` (caught only via `catch_unwind`). Root cause:
`pdf-extract-0.12.1/src/lib.rs:204`, `obj.expect(&key)` inside
`FromOptObj::from_opt_obj` -- an unconditional `Option::expect` on any missing
expected resource key, unreachable from outside the crate. **Verdict: `Err` is
NOT sufficient.** A `core` wrapper MUST call `extract_text_from_mem` inside
`std::panic::catch_unwind(AssertUnwindSafe(..))` and map a caught panic to an
error variant, or one malformed-but-well-formed attachment aborts the caller.
This must be explicit in the "corrupt PDF => error, no panic" criterion.

**Q2 — `minimal_pdf` fixture** (`ref/src/lib.rs::minimal_pdf`, std-only, 12
lines; shares `build_pdf`/`assemble`). Exact output for
`["Total: 1,234.56 EUR", "Due date: 15/10/2026"]`:
`"\n\nTotal: 1,234.56 EUR\nDue date: 15/10/2026"` -- two leading `\n` (output
preamble), lines joined by a single `\n`, no trailing newline/form-feed, no
spurious inner spaces. A wrong-but-numeric xref entry offset (+3 bytes) did
NOT error -- it returned `Ok("")`: lopdf silently loses data instead of
failing loudly. Assumption "off-by-a-few-bytes => Err" is KILLED; "no text
layer" and "corrupt input" are distinct signals, not interchangeable. Byte
`\x80` (WinAnsi Euro) through base-14 Helvetica decoded as NUL, not the euro
sign -- fixtures should use ASCII `"EUR"`, never the byte-0x80 glyph.

**Q3 — no text layer.** Empty content stream, rectangle-only content, and a
real 1x1 uncompressed image XObject (`/Im1 Do`) all return `Ok("")`.
`text.trim().is_empty()` is a correct and sufficient test.

**Q4 — encrypted PDF** (source-verified only; a real RC4/AES `/Encrypt`
fixture is >5 calls, skipped per budget). `maybe_decrypt` tries
`doc.decrypt("")` and on failure returns
`OutputError::PdfError(lopdf::Error::Decryption(lopdf::encryption::DecryptionError::IncorrectPassword))`.
Matching that nested variant is the cheap signal for "encrypted, non-empty
password." New unknown: unverified at runtime -- implementer should add one
assertion once a fixture exists, or accept source evidence.

**Q5 — build sanity.** `pdf-extract = "0.12"` (-> lopdf 0.42, getrandom 0.4.3)
builds clean on native Linux/current toolchain: no `wasm_js`/`getrandom`
warnings or errors, clean build ~4.8s.

`ref/`: `ref/src/lib.rs` (127 lines) holds only `minimal_pdf`, `build_pdf`,
`image_only_pdf` and their shared byte-offset/xref assembly -- the reusable
core for spec fixtures. Full spike crate (all Q1-Q3 test cases, panic
capture, xref-corruption helpers) is the sibling `spike-5` crate at
`features/5/spike/` (`src/lib.rs`, `tests/spike.rs`).
<!-- STATUS: COMPLETE -->
