//! [unit] tests for the `zip` module's public API. One file per level per module.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.
//! Helper fns (no `#[test]`) are still linted, so they return `Result`/`Option` and use `?`
//! rather than indexing, `unwrap`, `expect` or `panic!` (only `#[test]` fns are exempt).

use hauz_core::zip::{self, Entry, Error, MAX_ENTRY_BYTES};

/// Boxed so any error type propagates with `?`; tests never unwrap or expect.
type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

const STORED: &[u8] = include_bytes!("fixtures/zip/stored.zip");
const SMALL: &[u8] = include_bytes!("fixtures/zip/small.zip");
const INVOICE: &[u8] = include_bytes!("fixtures/zip/invoice.zip");
const BZIP2: &[u8] = include_bytes!("fixtures/zip/bzip2.zip");
const EMPTY: &[u8] = include_bytes!("fixtures/zip/empty.zip");
const INVOICE_XML: &[u8] = include_bytes!("fixtures/zip/ad000000001.xml");
const INVOICE_PDF: &[u8] = include_bytes!("fixtures/zip/ad000000001.pdf");

const CDH_SIG: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
const LFH_SIG: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];

/// A little-endian `u16` at `off`, or `None` if `bytes` is too short.
fn u16_at(bytes: &[u8], off: usize) -> Option<u16> {
    let s = bytes.get(off..off.checked_add(2)?)?;
    Some(u16::from_le_bytes([*s.first()?, *s.get(1)?]))
}

/// Finds the offset of the central directory header whose name matches `name`.
fn find_central_header(zip: &[u8], name: &[u8]) -> Result<usize> {
    let mut i = 0usize;
    while let Some(header) = zip.get(i..i.checked_add(46).ok_or("overflow")?) {
        if header.get(0..4) == Some(&CDH_SIG[..]) {
            let name_len = usize::from(u16_at(header, 28).ok_or("short header")?);
            let end = i
                .checked_add(46)
                .and_then(|v| v.checked_add(name_len))
                .ok_or("overflow")?;
            if zip.get(i.checked_add(46).ok_or("overflow")?..end) == Some(name) {
                return Ok(i);
            }
        }
        i = i.checked_add(1).ok_or("overflow")?;
    }
    Err("central header not found".into())
}

/// Finds the offset of the local file header whose name matches `name`.
fn find_local_header(zip: &[u8], name: &[u8]) -> Result<usize> {
    let mut i = 0usize;
    while let Some(header) = zip.get(i..i.checked_add(30).ok_or("overflow")?) {
        if header.get(0..4) == Some(&LFH_SIG[..]) {
            let name_len = usize::from(u16_at(header, 26).ok_or("short header")?);
            let end = i
                .checked_add(30)
                .and_then(|v| v.checked_add(name_len))
                .ok_or("overflow")?;
            if zip.get(i.checked_add(30).ok_or("overflow")?..end) == Some(name) {
                return Ok(i);
            }
        }
        i = i.checked_add(1).ok_or("overflow")?;
    }
    Err("local header not found".into())
}

/// Returns a copy of `zip` with bit `bit` of the given entry's central-directory flag byte
/// set.
fn with_flag_bit(zip: &[u8], name: &[u8], bit: u8) -> Result<Vec<u8>> {
    let mut out = zip.to_vec();
    let header = find_central_header(&out, name)?;
    let idx = header.checked_add(8).ok_or("overflow")?;
    let slot = out.get_mut(idx).ok_or("short archive")?;
    *slot |= bit;
    Ok(out)
}

/// Returns a copy of `zip` with the central directory's uncompressed size for `name`
/// overwritten.
fn with_uncompressed_size(zip: &[u8], name: &[u8], size: u32) -> Result<Vec<u8>> {
    let mut out = zip.to_vec();
    let header = find_central_header(&out, name)?;
    let start = header.checked_add(24).ok_or("overflow")?;
    let end = start.checked_add(4).ok_or("overflow")?;
    let slot = out.get_mut(start..end).ok_or("short archive")?;
    slot.copy_from_slice(&size.to_le_bytes());
    Ok(out)
}

/// Flips one bit of the byte at `needle`'s first occurrence in `haystack`.
fn flip_one_byte(haystack: &[u8], needle: &[u8]) -> Result<Vec<u8>> {
    let mut out = haystack.to_vec();
    let pos = haystack
        .windows(needle.len())
        .position(|w| w == needle)
        .ok_or("needle not found")?;
    let slot = out.get_mut(pos).ok_or("short archive")?;
    *slot ^= 0x01;
    Ok(out)
}

/// Flips one byte inside the local entry's compressed data.
fn flip_compressed_byte(zip: &[u8], name: &[u8]) -> Result<Vec<u8>> {
    let mut out = zip.to_vec();
    let header = find_local_header(&out, name)?;
    let header_bytes = out
        .get(header..header.checked_add(30).ok_or("overflow")?)
        .ok_or("short archive")?;
    let name_len = usize::from(u16_at(header_bytes, 26).ok_or("short header")?);
    let extra_len = usize::from(u16_at(header_bytes, 28).ok_or("short header")?);
    let data_start = header
        .checked_add(30)
        .and_then(|v| v.checked_add(name_len))
        .and_then(|v| v.checked_add(extra_len))
        .ok_or("overflow")?;
    let slot = out.get_mut(data_start).ok_or("short archive")?;
    *slot ^= 0x01;
    Ok(out)
}

fn entry(name: &str, bytes: &[u8]) -> Entry {
    Entry {
        name: name.to_string(),
        bytes: bytes.to_vec(),
    }
}

#[test]
fn ac1_stored_archive_skips_directory_entry() -> Result<()> {
    let entries = zip::read(STORED)?;
    assert_eq!(
        entries,
        vec![
            entry("folder/hello.txt", b"hello, zip\n"),
            entry("empty.txt", b"")
        ]
    );
    Ok(())
}

#[test]
fn ac2_fixed_huffman_block() -> Result<()> {
    let entries = zip::read(SMALL)?;
    assert_eq!(entries, vec![entry("a.txt", b"abcabcabcabc\n")]);
    Ok(())
}

#[test]
fn ac3_dynamic_huffman_and_stored_deflate_block() -> Result<()> {
    let entries = zip::read(INVOICE)?;
    let xml = entries
        .iter()
        .find(|e| e.name == "ad000000001.xml")
        .ok_or("missing xml entry")?;
    let pdf = entries
        .iter()
        .find(|e| e.name == "ad000000001.pdf")
        .ok_or("missing pdf entry")?;
    assert_eq!(xml.bytes, INVOICE_XML);
    assert_eq!(pdf.bytes, INVOICE_PDF);
    Ok(())
}

#[test]
fn ac4_empty_archive_has_no_entries() -> Result<()> {
    assert_eq!(zip::read(EMPTY)?, Vec::new());
    Ok(())
}

#[test]
fn ac5_unsupported_method_encryption_and_data_descriptor() -> Result<()> {
    assert_eq!(
        zip::read(BZIP2),
        Err(Error::Unsupported {
            feature: "compression method 12".to_string()
        })
    );

    let encrypted = with_flag_bit(STORED, b"folder/hello.txt", 0x01)?;
    assert_eq!(
        zip::read(&encrypted),
        Err(Error::Unsupported {
            feature: "encryption".to_string()
        })
    );

    let descriptor = with_flag_bit(STORED, b"folder/hello.txt", 0x08)?;
    assert_eq!(
        zip::read(&descriptor),
        Err(Error::Unsupported {
            feature: "data descriptor".to_string()
        })
    );
    Ok(())
}

#[test]
fn ac6_malformed_never_panics() -> Result<()> {
    let short_len = INVOICE.len().checked_sub(10).ok_or("fixture too small")?;
    let truncated_eocd = INVOICE.get(..short_len).ok_or("fixture too small")?;
    assert!(matches!(
        zip::read(truncated_eocd),
        Err(Error::Malformed { .. })
    ));

    assert!(matches!(zip::read(&[]), Err(Error::Malformed { .. })));

    let bad_crc = flip_one_byte(STORED, b"hello, zip\n")?;
    assert!(matches!(zip::read(&bad_crc), Err(Error::Malformed { .. })));

    let bad_stream = flip_compressed_byte(INVOICE, b"ad000000001.xml")?;
    assert!(matches!(
        zip::read(&bad_stream),
        Err(Error::Malformed { .. })
    ));
    Ok(())
}

#[test]
fn ac7_declared_size_above_max_is_unsupported_without_inflating() -> Result<()> {
    let oversized = u32::try_from(MAX_ENTRY_BYTES)
        .ok()
        .and_then(|v| v.checked_add(1))
        .ok_or("MAX_ENTRY_BYTES does not fit in u32")?;
    let huge = with_uncompressed_size(SMALL, b"a.txt", oversized)?;
    assert_eq!(
        zip::read(&huge),
        Err(Error::Unsupported {
            feature: "entry larger than MAX_ENTRY_BYTES".to_string()
        })
    );
    Ok(())
}
