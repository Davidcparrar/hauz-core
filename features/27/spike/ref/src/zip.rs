//! Minimal zip reader: walks the central directory (scanning back from EOF
//! for the End Of Central Directory record) and returns decompressed
//! `(name, bytes)` pairs for each entry. std only; supports stored (0) and
//! deflate (8) methods, which is all this spike needs.

use crate::inflate;

#[derive(Debug)]
pub enum Error {
    NoEocd,
    Truncated,
    BadSignature,
    UnsupportedMethod(u16),
    Inflate(inflate::Error),
}

impl From<inflate::Error> for Error {
    fn from(e: inflate::Error) -> Self {
        Error::Inflate(e)
    }
}

const EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const CDH_SIG: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];

fn u16le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
fn u32le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// Read every entry of a zip file, decompressing stored/deflate data.
pub fn read_entries(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, Error> {
    // Scan backwards for the EOCD signature (it may be followed by a comment).
    let min_eocd = 22;
    if data.len() < min_eocd {
        return Err(Error::Truncated);
    }
    let search_start = data.len().saturating_sub(min_eocd + 65536);
    let eocd_off = (search_start..=data.len() - min_eocd)
        .rev()
        .find(|&i| data[i..i + 4] == EOCD_SIG)
        .ok_or(Error::NoEocd)?;

    let total_entries = u16le(data, eocd_off + 10) as usize;
    let cd_offset = u32le(data, eocd_off + 16) as usize;

    let mut out = Vec::with_capacity(total_entries);
    let mut pos = cd_offset;
    for _ in 0..total_entries {
        if data.len() < pos + 46 || data[pos..pos + 4] != CDH_SIG {
            return Err(Error::BadSignature);
        }
        let method = u16le(data, pos + 10);
        let comp_size = u32le(data, pos + 20) as usize;
        let name_len = u16le(data, pos + 28) as usize;
        let extra_len = u16le(data, pos + 30) as usize;
        let comment_len = u16le(data, pos + 32) as usize;
        let local_header_offset = u32le(data, pos + 42) as usize;
        let name_bytes = &data[pos + 46..pos + 46 + name_len];
        let name = String::from_utf8_lossy(name_bytes).into_owned();

        let bytes = read_local_entry(data, local_header_offset, method, comp_size)?;
        out.push((name, bytes));

        pos += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

fn read_local_entry(
    data: &[u8],
    local_off: usize,
    method: u16,
    comp_size: usize,
) -> Result<Vec<u8>, Error> {
    if data.len() < local_off + 30 {
        return Err(Error::Truncated);
    }
    let name_len = u16le(data, local_off + 26) as usize;
    let extra_len = u16le(data, local_off + 28) as usize;
    let data_start = local_off + 30 + name_len + extra_len;
    let data_end = data_start + comp_size;
    let raw = data.get(data_start..data_end).ok_or(Error::Truncated)?;
    match method {
        0 => Ok(raw.to_vec()),
        8 => Ok(inflate::inflate(raw)?),
        other => Err(Error::UnsupportedMethod(other)),
    }
}
