# Review: std-only zip reader for stored and deflate archives (#32)

## Cycle 1 — 2026-10-03, reviewer on 5604911
VERDICT: APPROVE
verify.sh: `verify: ALL GREEN`

- AC1–AC7 as `ac1_stored_archive_skips_directory_entry`, `ac2_fixed_huffman_block`,
  `ac3_dynamic_huffman_and_stored_deflate_block`, `ac4_empty_archive_has_no_entries`,
  `ac5_unsupported_method_encryption_and_data_descriptor`, `ac6_malformed_never_panics`,
  `ac7_declared_size_above_max_is_unsupported_without_inflating` in
  `crates/core/tests/unit_zip.rs`; AC8 `ac8_stored_archive_round_trips`, AC9
  `ac9_read_never_panics` in `property_zip.rs` (own CRC-32 and stored-zip writer). Levels
  and assertions match the spec: exact `Entry` vectors, exact `Unsupported` strings.
- Pub surface is exactly `zip::{read, Entry, Error, MAX_ENTRY_BYTES}`; `inflate` is
  `pub(crate)`; `Error` is `#[non_exhaustive]`; no `unwrap`/`expect`/`panic`/indexing in
  library code, no `#[allow]`, no manifest change. EOCD scan (≤64 KiB comment), `/`-suffix
  skip, lossy names, CRC + size verification, cap before and during inflate all correct.
- Independent check outside the repo: 12 payloads × deflate levels 0/1/6/9 plus a
  3000-byte-comment archive decode bit-exactly; 200k mutated/truncated archives in debug
  and release gave 0 panics.
- Non-blocking: AC9's uniform random bytes rarely contain an EOCD signature, so a future
  spec could seed mutations from a valid archive; `build_huffman` skips puff's
  over-subscription check (a crafted table mis-decodes until the CRC rejects it); the spec's
  non-goal claiming multi-disk ⇒ `Unsupported` had no code path — spec wording fixed.
