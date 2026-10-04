# Spec: zip: std-only reader for stored and deflate archives (#32)

## Problem
DIAN e-invoices arrive as one `application/zip` attachment (UBL XML + PDF) that no module can
open, so those bills land `NeedsReview`. This feature adds `core::zip`, a dependency-free
reader turning an archive's bytes into entries (name + inflated bytes, CRC-checked). #27
builds the UBL extractor on it; no bill behaviour changes here.

## Non-goals
- No zip64, encryption, data descriptors (flag bit 3), methods other than 0/8, multi-disk or
  writing: each is `Unsupported`, never guessed around.
- No `Extractor`; `extract`, `ingest` and the binaries are untouched (#27).
- No new dependency: `flate2` is in the tree via `lopdf`, but declaring it is a dependency
  decision; the spike proved a std-only inflate suffices.
- Corpus bytes never enter the repo; fixtures come from `features/32/make_fixtures.py`.

## Assumptions
- [spike-verified] (`features/27/spike/findings.md` Q1/Q3) the real archives are method 8,
  no descriptor/encryption/zip64, EOCD at EOF; the std-only inflate in
  `features/27/spike/ref/src/inflate.rs` decodes 6/6 corpus entries and 30/30 random
  zlib buffers bit-exactly.
- [spike-verified] fixtures: `small.zip` = one fixed-Huffman block; `invoice.zip` = a
  dynamic-Huffman `.xml` entry plus a `.pdf` whose deflate stream is a stored block;
  `stored.zip` = method 0 with a directory entry and an empty file; `bzip2.zip` = method 12;
  `empty.zip` = 22 bytes, zero entries.
- Design call: `pub struct Entry { pub name: String, pub bytes: Vec<u8> }` (pub-field record
  like `email::Document`; `Debug, Clone, PartialEq, Eq`). `pub fn read(archive: &[u8]) ->
  Result<Vec<Entry>, Error>`: scans back for the EOCD (comment ≤ 64 KiB), walks the central
  directory in order, reads each local header, inflates, verifies CRC-32 and uncompressed
  size. Directory entries (name ends in `/`) are skipped. Names are `from_utf8_lossy`.
- Design call: `#[non_exhaustive] Error { Malformed { reason: String }, Unsupported {
  feature: String } }` (`thiserror`, `Clone, Eq`). `Malformed`: no EOCD, truncated, wrong
  signature, invalid deflate stream, CRC or size mismatch, name offset past EOF.
  `Unsupported`: `"compression method <n>"`, `"encryption"`, `"data descriptor"`,
  `"zip64"` (0xFFFFFFFF sizes/offsets or the extra field 0x0001), `"entry larger than
  MAX_ENTRY_BYTES"`.
- Design call: `pub const MAX_ENTRY_BYTES: usize = 64 << 20`; the declared uncompressed size
  is checked before inflating and inflate stops with `Malformed` once output exceeds it (no
  zip bomb). Offset arithmetic is checked (`checked_add`, `get`); AC9 forbids overflow panics.
- Design call: layout `crates/core/src/zip.rs` (container) + private `crates/core/src/zip/
  inflate.rs` (RFC 1951, ported from the spike ref with `get`-based access and a
  `Result`-returning bit reader); `lib.rs` gains `pub mod zip`.

## Reference implementation
`features/27/spike/ref/src/zip.rs` (91 lines) and `inflate.rs` (248 lines). Illustrative:
they index slices freely; reimplement under the lints.

## Architecture delta
- `lib.rs`: `pub mod zip`. New module `zip`: `Entry`, `read`, `Error`, `MAX_ENTRY_BYTES`.
- No manifest change, no binary change. `PROMOTES: zip` → `docs/architecture.md` module
  line + one `docs/decisions.md` line (Leader, step 8).

## Test plan
Files: `crates/core/tests/unit_zip.rs` (AC1–AC7), `crates/core/tests/property_zip.rs`
(AC8–AC9). Fixtures: run `python3 features/32/make_fixtures.py` from the repo root once; it
writes `crates/core/tests/fixtures/zip/{stored,small,invoice,bzip2,empty}.zip` plus the
plain `ad000000001.xml` / `ad000000001.pdf` that `invoice.zip`'s entries equal. Commit them.
No integration/e2e: one module, no entry point.
- AC1 [unit] WHEN `read` is given `stored.zip` THE SYSTEM SHALL return exactly
  `[folder/hello.txt = b"hello, zip\n", empty.txt = b""]` in that order (the directory entry
  skipped).
- AC2 [unit] WHEN `read` is given `small.zip` THE SYSTEM SHALL return `[a.txt =
  b"abcabcabcabc\n"]` (fixed Huffman block).
- AC3 [unit] WHEN `read` is given `invoice.zip` THE SYSTEM SHALL return
  `ad000000001.xml` and `ad000000001.pdf` whose bytes equal the plain fixture files (dynamic
  Huffman and stored deflate blocks).
- AC4 [unit] WHEN `read` is given `empty.zip` THE SYSTEM SHALL return `Ok(vec![])`.
- AC5 [unit] WHEN `read` is given `bzip2.zip` THE SYSTEM SHALL return `Unsupported { feature:
  "compression method 12" }`; WHEN the central-directory flag byte of `stored.zip` has bit 0
  set THE SYSTEM SHALL return `Unsupported { feature: "encryption" }`; with bit 3 set,
  `Unsupported { feature: "data descriptor" }`.
- AC6 [unit] WHEN `read` is given `invoice.zip` minus its last 10 bytes THE SYSTEM SHALL
  return `Malformed`; WHEN given an empty slice, `Malformed`; WHEN one byte of
  `stored.zip`'s `hello, zip` payload is flipped, `Malformed` (CRC); WHEN one byte inside
  `invoice.zip`'s `.xml` compressed data is flipped, `Malformed` (CRC or stream), never a
  panic.
- AC7 [unit] WHEN the central directory of `small.zip` declares an uncompressed size above
  `MAX_ENTRY_BYTES` THE SYSTEM SHALL return `Unsupported` without inflating.
- AC8 [property] FOR ALL archives of 0..=4 entries (ASCII names 1..=16 chars, payloads
  0..=2048 bytes) built stored by the test's own writer (local headers, central directory,
  EOCD, CRC-32 computed by the test) THE SYSTEM SHALL return the same names and bytes in
  order.
- AC9 [property] FOR ALL byte vectors of length 0..=4096 THE SYSTEM SHALL return `Ok` or
  `Err` from `read` without panicking (fuzzes the EOCD scan, header parsing and inflate).

<!-- GATE 1: all seven boxes ticked (no integration/e2e: one module, no entry point) -->
