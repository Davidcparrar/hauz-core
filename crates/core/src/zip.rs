//! Dependency-free zip reader: [`read`] scans back from the end of an archive for the End Of
//! Central Directory record (comment up to 64 KiB), walks the central directory in order,
//! reads each local header, inflates stored (method 0) or deflated (method 8) data, and
//! verifies CRC-32 and declared uncompressed size. Directory entries (name ending in `/`) are
//! skipped. Zip64, encryption, data descriptors (flag bit 3) and any other compression method
//! are [`Error::Unsupported`], never guessed around: the real DIAN e-invoice archives #27
//! builds on this are plain stored/deflate, no descriptor, no encryption, no zip64.

mod inflate;

use thiserror::Error as ThisError;

/// One decoded entry: its name and inflated bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The entry's name as recorded in the central directory, decoded with
    /// [`String::from_utf8_lossy`].
    pub name: String,
    /// The entry's decompressed bytes.
    pub bytes: Vec<u8>,
}

/// Errors from [`read`].
#[derive(Debug, Clone, PartialEq, Eq, ThisError)]
#[non_exhaustive]
pub enum Error {
    /// The archive is truncated, has no End Of Central Directory record, a header has the
    /// wrong signature, a deflate stream is invalid, a CRC-32 or size check failed, or a name
    /// offset points past the end of the archive.
    #[error("malformed zip archive: {reason}")]
    Malformed {
        /// A short, human-readable description of what failed.
        reason: String,
    },
    /// A feature this reader deliberately does not support: a compression method other than
    /// stored (0) or deflate (8), encryption, a data descriptor, or zip64.
    #[error("unsupported zip feature: {feature}")]
    Unsupported {
        /// The unsupported feature, e.g. `"compression method 12"`, `"encryption"`,
        /// `"data descriptor"`, `"zip64"`, or `"entry larger than MAX_ENTRY_BYTES"`.
        feature: String,
    },
}

/// The largest single entry [`read`] will inflate. The declared uncompressed size is checked
/// before inflating; inflate itself also stops once output exceeds this cap, so a crafted
/// archive cannot force unbounded allocation (no zip bomb).
pub const MAX_ENTRY_BYTES: usize = 64 << 20;

const EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const CDH_SIG: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
const LFH_SIG: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];
const MIN_EOCD: usize = 22;
const MAX_COMMENT: usize = 65536;

fn malformed(reason: impl Into<String>) -> Error {
    Error::Malformed {
        reason: reason.into(),
    }
}

fn unsupported(feature: impl Into<String>) -> Error {
    Error::Unsupported {
        feature: feature.into(),
    }
}

fn truncated() -> Error {
    malformed("truncated")
}

fn bad_signature() -> Error {
    malformed("wrong header signature")
}

/// `base + delta`, as a [`Error::Malformed`] rather than a panic on overflow.
fn offset(base: usize, delta: usize) -> Result<usize, Error> {
    base.checked_add(delta).ok_or_else(truncated)
}

fn u16le(data: &[u8], off: usize) -> Result<u16, Error> {
    let end = offset(off, 2)?;
    let slice = data.get(off..end).ok_or_else(truncated)?;
    let arr: [u8; 2] = slice.try_into().map_err(|_| truncated())?;
    Ok(u16::from_le_bytes(arr))
}

fn u32le(data: &[u8], off: usize) -> Result<u32, Error> {
    let end = offset(off, 4)?;
    let slice = data.get(off..end).ok_or_else(truncated)?;
    let arr: [u8; 4] = slice.try_into().map_err(|_| truncated())?;
    Ok(u32::from_le_bytes(arr))
}

/// Scans backward from the end of `data` for the EOCD signature, allowing for a trailing
/// comment of up to [`MAX_COMMENT`] bytes.
fn find_eocd(data: &[u8]) -> Result<usize, Error> {
    if data.len() < MIN_EOCD {
        return Err(truncated());
    }
    let search_start = data.len().saturating_sub(MIN_EOCD + MAX_COMMENT);
    let search_end = data.len() - MIN_EOCD;
    (search_start..=search_end)
        .rev()
        .find(|&i| data.get(i..i + 4) == Some(&EOCD_SIG[..]))
        .ok_or_else(|| malformed("no End Of Central Directory record found"))
}

/// Whether a central-directory extra field contains a zip64 header (id `0x0001`).
fn has_zip64_extra(extra: &[u8]) -> bool {
    let mut i = 0usize;
    loop {
        let Some(header) = extra.get(i..i.saturating_add(4)) else {
            return false;
        };
        let Ok(id) = u16le(header, 0) else {
            return false;
        };
        let Ok(size) = u16le(header, 2) else {
            return false;
        };
        if id == 0x0001 {
            return true;
        }
        let Some(next) = i
            .checked_add(4)
            .and_then(|v| v.checked_add(usize::from(size)))
        else {
            return false;
        };
        i = next;
    }
}

/// Fields read from one central directory header, plus the name and local header offset
/// needed to locate and decode the entry's data.
struct CentralEntry {
    method: u16,
    crc32: u32,
    comp_size: u32,
    uncomp_size: u32,
    name: String,
    local_header_offset: u32,
}

/// Parses the central directory header at `pos`, returning it and the offset of the next
/// header.
fn parse_central_entry(data: &[u8], pos: usize) -> Result<(CentralEntry, usize), Error> {
    let sig_end = offset(pos, 4)?;
    if data.get(pos..sig_end) != Some(&CDH_SIG[..]) {
        return Err(bad_signature());
    }
    let flags = u16le(data, offset(pos, 8)?)?;
    if flags & 0x0001 != 0 {
        return Err(unsupported("encryption"));
    }
    if flags & 0x0008 != 0 {
        return Err(unsupported("data descriptor"));
    }
    let method = u16le(data, offset(pos, 10)?)?;
    let crc32 = u32le(data, offset(pos, 16)?)?;
    let comp_size = u32le(data, offset(pos, 20)?)?;
    let uncomp_size = u32le(data, offset(pos, 24)?)?;
    let name_len = usize::from(u16le(data, offset(pos, 28)?)?);
    let extra_len = usize::from(u16le(data, offset(pos, 30)?)?);
    let comment_len = usize::from(u16le(data, offset(pos, 32)?)?);
    let local_header_offset = u32le(data, offset(pos, 42)?)?;

    let name_start = offset(pos, 46)?;
    let name_end = offset(name_start, name_len)?;
    let name_bytes = data.get(name_start..name_end).ok_or_else(truncated)?;
    let name = String::from_utf8_lossy(name_bytes).into_owned();

    let extra_end = offset(name_end, extra_len)?;
    let extra = data.get(name_end..extra_end).ok_or_else(truncated)?;

    if comp_size == u32::MAX
        || uncomp_size == u32::MAX
        || local_header_offset == u32::MAX
        || has_zip64_extra(extra)
    {
        return Err(unsupported("zip64"));
    }

    let next_pos = offset(extra_end, comment_len)?;

    Ok((
        CentralEntry {
            method,
            crc32,
            comp_size,
            uncomp_size,
            name,
            local_header_offset,
        },
        next_pos,
    ))
}

/// Locates an entry's compressed data from its local header and decodes it, checking CRC-32
/// and uncompressed size against the central directory's values.
fn read_local_entry(
    data: &[u8],
    local_off: usize,
    method: u16,
    comp_size: usize,
    uncomp_size: usize,
    expected_crc: u32,
) -> Result<Vec<u8>, Error> {
    let sig_end = offset(local_off, 4)?;
    if data.get(local_off..sig_end) != Some(&LFH_SIG[..]) {
        return Err(bad_signature());
    }
    let name_len = usize::from(u16le(data, offset(local_off, 26)?)?);
    let extra_len = usize::from(u16le(data, offset(local_off, 28)?)?);
    let header_end = offset(local_off, 30)?;
    let data_start = offset(offset(header_end, name_len)?, extra_len)?;
    let data_end = offset(data_start, comp_size)?;
    let raw = data.get(data_start..data_end).ok_or_else(truncated)?;

    let bytes = match method {
        0 => raw.to_vec(),
        8 => inflate::inflate(raw, uncomp_size)
            .map_err(|e| malformed(format!("invalid deflate stream: {e}")))?,
        other => return Err(unsupported(format!("compression method {other}"))),
    };

    if bytes.len() != uncomp_size {
        return Err(malformed("uncompressed size mismatch"));
    }
    if crc32(&bytes) != expected_crc {
        return Err(malformed("CRC-32 mismatch"));
    }
    Ok(bytes)
}

/// Standard zip/ISO-HDLC CRC-32.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Reads every entry of a zip archive, decompressing stored and deflated data.
///
/// # Errors
/// [`Error::Malformed`] when the bytes are truncated, lack an EOCD record, a header has the
/// wrong signature, a deflate stream is invalid, a CRC-32 or size check fails, or a name/data
/// offset points past the end of the archive. [`Error::Unsupported`] for encryption, a data
/// descriptor, zip64, a compression method other than stored/deflate, or a declared
/// uncompressed size above [`MAX_ENTRY_BYTES`] (checked before inflating).
pub fn read(archive: &[u8]) -> Result<Vec<Entry>, Error> {
    let eocd_off = find_eocd(archive)?;
    let total_entries = usize::from(u16le(archive, offset(eocd_off, 10)?)?);
    let cd_offset_raw = u32le(archive, offset(eocd_off, 16)?)?;
    let mut pos = usize::try_from(cd_offset_raw).map_err(|_| truncated())?;

    let mut out = Vec::new();
    for _ in 0..total_entries {
        let (entry, next_pos) = parse_central_entry(archive, pos)?;
        pos = next_pos;

        if entry.name.ends_with('/') {
            continue;
        }

        let uncomp_size = usize::try_from(entry.uncomp_size).map_err(|_| truncated())?;
        if uncomp_size > MAX_ENTRY_BYTES {
            return Err(unsupported("entry larger than MAX_ENTRY_BYTES"));
        }
        let comp_size = usize::try_from(entry.comp_size).map_err(|_| truncated())?;
        let local_off = usize::try_from(entry.local_header_offset).map_err(|_| truncated())?;

        let bytes = read_local_entry(
            archive,
            local_off,
            entry.method,
            comp_size,
            uncomp_size,
            entry.crc32,
        )?;
        out.push(Entry {
            name: entry.name,
            bytes,
        });
    }
    Ok(out)
}
