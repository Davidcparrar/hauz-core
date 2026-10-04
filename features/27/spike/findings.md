# Findings — feature #27 (DIAN e-invoice zip extractor)

1. **Zip container.** All 3 corpus zips: 2 entries each, one `.xml` (prefix
   `ad<digits>`, the `AttachedDocument`) and one `.pdf` (prefix `ad<digits>` or
   `fv<digits>`, interchangeably). Every entry: compression method 8
   (deflate), general-purpose flag bit 3 (data descriptor) **unset**, bit 0
   (encryption) **unset**, no zip64 extra field, non-UTF-8-filename flag
   unset (plain ASCII names anyway). Central directory is well-formed: EOCD
   found scanning back from EOF (right at EOF, no trailing comment/bytes) in
   all 3; `zipfile.testzip()` reports no bad CRCs.

2. **UBL structure.** The `Invoice` is embedded as `<![CDATA[...]]>` inside
   `AttachedDocument/cac:Attachment/cac:ExternalReference/cbc:Description`
   (confirmed structurally and by direct XML-in-XML parse). All 3 files:
   `<?xml version="1.0" encoding="utf-8"?>` declaration (one adds
   `standalone="no"`), no BOM. Outer uses `cac:`/`cbc:` prefixes (plus `ds:`,
   `ext:`, sometimes `xades:`); the embedded `Invoice` declares a default
   namespace `xmlns="urn:oasis:...:Invoice-2"` and reuses `cac:`/`cbc:`
   prefixes for children. `LegalMonetaryTotal/PayableAmount` always carries
   `currencyID`; amount text is a plain decimal, dot separator, exactly 2
   decimals (`DDD.DD` shape) in all 3. Supplier name is identical across
   `PartyLegalEntity/RegistrationName`, `PartyTaxScheme/RegistrationName`,
   and `PartyName/Name` in every sample (and matches the outer
   `AttachedDocument/SenderParty/PartyTaxScheme/RegistrationName`) — any one
   source works. `DueDate` (top-level `Invoice/DueDate`) is present in 2/3;
   `PaymentMeans/PaymentDueDate` is present in **all 3** and is the reliable
   due-date field. `InvoicePeriod/StartDate+EndDate` is present in only 1/3
   — not reliable. All dates are `YYYY-MM-DD`. No XML entities or non-ASCII
   seen in the 3 supplier names (can't rule out elsewhere; the plucker
   decodes the 5 predefined entities regardless).

3. **Hand-rolled inflate** (std only, `features/27/spike/ref/`):
   `src/zip.rs` 91 lines (EOCD scan + central dir walk + local-entry read),
   `src/inflate.rs` 248 lines (RFC 1951: stored/fixed/dynamic Huffman, ported
   from puff.c's bit-by-bit canonical decode), `src/xml.rs` 145 lines (parent-
   chain element finder, CDATA + 5 entities, one attribute). Verified: all 6
   entries across the 3 corpus zips decode bit-exact (sha256 match vs
   Python's `zipfile`) — 6/6 pass. 30 random raw-deflate buffers (sizes
   0-200 KiB, `zlib.compressobj(level 1-9, wbits=-15)`, mixed
   random/repetitive content) — 30/30 pass. Plucker verified against a
   synthetic fixture (unit tests): nested parent-chain lookup + `&amp;`
   entity decode, and a miss case. No failures encountered.

4. **Risk.** All 3 corpus zip PDFs have a real text layer (1.3k-2.1k chars
   via `pdftotext`); `PdfTextExtractor`-style heuristics would work as a
   fallback but the XML is exact and should be preferred for these zips.

<!-- STATUS: COMPLETE -->
