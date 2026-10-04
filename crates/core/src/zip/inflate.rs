//! Hand-rolled RFC 1951 ("raw deflate") inflater: std only, ported from the structure of
//! Mark Adler's `puff.c` reference decoder. Every slice access goes through `get`, every
//! addition through `checked_add`/`checked_sub`/`checked_shl`/`checked_shr`: a malformed or
//! adversarial stream returns [`Error`], never panics. `inflate` also stops with
//! [`Error::OutputTooLarge`] the moment decompressed output would exceed the caller's
//! declared cap, so a crafted stream cannot grow without bound (no zip bomb).

use thiserror::Error as ThisError;

/// Errors from [`inflate`].
#[derive(Debug, Clone, PartialEq, Eq, ThisError)]
pub(crate) enum Error {
    #[error("unexpected end of deflate stream")]
    UnexpectedEof,
    #[error("invalid deflate block type")]
    BadBlockType,
    #[error("stored block length check failed")]
    BadStoredLen,
    #[error("invalid Huffman code")]
    BadHuffmanCode,
    #[error("invalid Huffman code lengths")]
    BadCodeLengths,
    #[error("invalid back-reference distance")]
    BadDistance,
    #[error("invalid length code")]
    BadLength,
    #[error("decompressed output exceeds the declared size")]
    OutputTooLarge,
}

/// Reads bits LSB-first from a byte slice, as DEFLATE requires.
struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bitbuf: u32,
    bitcnt: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bitbuf: 0,
            bitcnt: 0,
        }
    }

    fn bits(&mut self, n: u32) -> Result<u32, Error> {
        while self.bitcnt < n {
            let byte = *self.data.get(self.pos).ok_or(Error::UnexpectedEof)?;
            self.pos = self.pos.checked_add(1).ok_or(Error::UnexpectedEof)?;
            let shifted = u32::from(byte).checked_shl(self.bitcnt).unwrap_or(0);
            self.bitbuf |= shifted;
            self.bitcnt = self.bitcnt.checked_add(8).ok_or(Error::UnexpectedEof)?;
        }
        let mask = if n == 0 {
            0
        } else {
            1u32.checked_shl(n)
                .and_then(|v| v.checked_sub(1))
                .unwrap_or(u32::MAX)
        };
        let out = self.bitbuf & mask;
        self.bitbuf = self.bitbuf.checked_shr(n).unwrap_or(0);
        self.bitcnt = self.bitcnt.checked_sub(n).ok_or(Error::UnexpectedEof)?;
        Ok(out)
    }

    fn align_to_byte(&mut self) {
        self.bitbuf = 0;
        self.bitcnt = 0;
    }

    fn take_bytes(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.pos.checked_add(n).ok_or(Error::UnexpectedEof)?;
        let slice = self.data.get(self.pos..end).ok_or(Error::UnexpectedEof)?;
        self.pos = end;
        Ok(slice)
    }
}

/// Canonical Huffman decode table: `count[len]` codes of each length, symbols listed in
/// canonical order.
#[derive(Debug)]
struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

fn build_huffman(lengths: &[u8]) -> Result<Huffman, Error> {
    let mut count = [0u16; 16];
    for &l in lengths {
        if l == 0 {
            continue;
        }
        let idx = usize::from(l);
        let slot = count.get_mut(idx).ok_or(Error::BadCodeLengths)?;
        *slot = slot.checked_add(1).ok_or(Error::BadCodeLengths)?;
    }

    let mut offs = [0u16; 16];
    for len in 1..16usize {
        let prev_count = *count.get(len - 1).ok_or(Error::BadCodeLengths)?;
        let prev_offs = *offs.get(len - 1).ok_or(Error::BadCodeLengths)?;
        let sum = prev_offs
            .checked_add(prev_count)
            .ok_or(Error::BadCodeLengths)?;
        let slot = offs.get_mut(len).ok_or(Error::BadCodeLengths)?;
        *slot = sum;
    }

    let mut symbol = vec![0u16; lengths.len()];
    for (sym, &l) in lengths.iter().enumerate() {
        if l == 0 {
            continue;
        }
        let idx = usize::from(l);
        let off_slot = offs.get_mut(idx).ok_or(Error::BadCodeLengths)?;
        let position = usize::from(*off_slot);
        let sym_u16 = u16::try_from(sym).map_err(|_| Error::BadCodeLengths)?;
        let slot = symbol.get_mut(position).ok_or(Error::BadCodeLengths)?;
        *slot = sym_u16;
        *off_slot = off_slot.checked_add(1).ok_or(Error::BadCodeLengths)?;
    }

    Ok(Huffman { count, symbol })
}

fn decode(br: &mut BitReader<'_>, h: &Huffman) -> Result<u16, Error> {
    let mut code: i32 = 0;
    let mut first: i32 = 0;
    let mut index: i32 = 0;
    for len in 1..16usize {
        code |= i32::try_from(br.bits(1)?).map_err(|_| Error::BadHuffmanCode)?;
        let cnt = i32::from(*h.count.get(len).ok_or(Error::BadHuffmanCode)?);
        if code - first < cnt {
            let sym_index =
                usize::try_from(index + (code - first)).map_err(|_| Error::BadHuffmanCode)?;
            let sym = *h.symbol.get(sym_index).ok_or(Error::BadHuffmanCode)?;
            return Ok(sym);
        }
        index += cnt;
        first += cnt;
        first <<= 1;
        code <<= 1;
    }
    Err(Error::BadHuffmanCode)
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn fixed_tables() -> Result<(Huffman, Huffman), Error> {
    let mut lit_lens = [0u8; 288];
    for (i, l) in lit_lens.iter_mut().enumerate() {
        *l = if i < 144 {
            8
        } else if i < 256 {
            9
        } else if i < 280 {
            7
        } else {
            8
        };
    }
    let dist_lens = [5u8; 30];
    Ok((build_huffman(&lit_lens)?, build_huffman(&dist_lens)?))
}

const CLEN_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

fn dynamic_tables(br: &mut BitReader<'_>) -> Result<(Huffman, Huffman), Error> {
    let hlit = usize::try_from(br.bits(5)?).map_err(|_| Error::BadCodeLengths)?;
    let hlit = hlit.checked_add(257).ok_or(Error::BadCodeLengths)?;
    let hdist = usize::try_from(br.bits(5)?).map_err(|_| Error::BadCodeLengths)?;
    let hdist = hdist.checked_add(1).ok_or(Error::BadCodeLengths)?;
    let hclen = usize::try_from(br.bits(4)?).map_err(|_| Error::BadCodeLengths)?;
    let hclen = hclen.checked_add(4).ok_or(Error::BadCodeLengths)?;

    let mut clen_lengths = [0u8; 19];
    for &idx in CLEN_ORDER.iter().take(hclen) {
        let bits = u8::try_from(br.bits(3)?).map_err(|_| Error::BadCodeLengths)?;
        let slot = clen_lengths.get_mut(idx).ok_or(Error::BadCodeLengths)?;
        *slot = bits;
    }
    let clen_table = build_huffman(&clen_lengths)?;

    let total = hlit.checked_add(hdist).ok_or(Error::BadCodeLengths)?;
    let mut lengths: Vec<u8> = Vec::new();
    while lengths.len() < total {
        let sym = decode(br, &clen_table)?;
        match sym {
            0..=15 => {
                let value = u8::try_from(sym).map_err(|_| Error::BadCodeLengths)?;
                lengths.push(value);
            }
            16 => {
                let prev = *lengths.last().ok_or(Error::BadCodeLengths)?;
                let rep = br.bits(2)?.checked_add(3).ok_or(Error::BadCodeLengths)?;
                let rep = usize::try_from(rep).map_err(|_| Error::BadCodeLengths)?;
                lengths.extend(std::iter::repeat_n(prev, rep));
            }
            17 => {
                let rep = br.bits(3)?.checked_add(3).ok_or(Error::BadCodeLengths)?;
                let rep = usize::try_from(rep).map_err(|_| Error::BadCodeLengths)?;
                lengths.extend(std::iter::repeat_n(0u8, rep));
            }
            18 => {
                let rep = br.bits(7)?.checked_add(11).ok_or(Error::BadCodeLengths)?;
                let rep = usize::try_from(rep).map_err(|_| Error::BadCodeLengths)?;
                lengths.extend(std::iter::repeat_n(0u8, rep));
            }
            _ => return Err(Error::BadCodeLengths),
        }
    }
    if lengths.len() != total {
        return Err(Error::BadCodeLengths);
    }
    let lit_lengths = lengths.get(..hlit).ok_or(Error::BadCodeLengths)?;
    let dist_lengths = lengths.get(hlit..).ok_or(Error::BadCodeLengths)?;
    let lit_table = build_huffman(lit_lengths)?;
    let dist_table = build_huffman(dist_lengths)?;
    Ok((lit_table, dist_table))
}

/// Appends `chunk` to `out`, failing once the total would exceed `max_output`.
fn push_checked(out: &mut Vec<u8>, chunk: &[u8], max_output: usize) -> Result<(), Error> {
    let new_len = out
        .len()
        .checked_add(chunk.len())
        .ok_or(Error::OutputTooLarge)?;
    if new_len > max_output {
        return Err(Error::OutputTooLarge);
    }
    out.extend_from_slice(chunk);
    Ok(())
}

fn inflate_block(
    br: &mut BitReader<'_>,
    lit: &Huffman,
    dist: &Huffman,
    out: &mut Vec<u8>,
    max_output: usize,
) -> Result<(), Error> {
    loop {
        let sym = decode(br, lit)?;
        match sym {
            0..=255 => {
                let byte = u8::try_from(sym).map_err(|_| Error::BadHuffmanCode)?;
                push_checked(out, &[byte], max_output)?;
            }
            256 => return Ok(()),
            257..=285 => {
                let i = usize::from(sym.checked_sub(257).ok_or(Error::BadLength)?);
                let len_base = *LEN_BASE.get(i).ok_or(Error::BadLength)?;
                let len_extra = *LEN_EXTRA.get(i).ok_or(Error::BadLength)?;
                let extra = usize::try_from(br.bits(u32::from(len_extra))?)
                    .map_err(|_| Error::BadLength)?;
                let len = usize::from(len_base)
                    .checked_add(extra)
                    .ok_or(Error::BadLength)?;

                let dsym = usize::from(decode(br, dist)?);
                let dist_base = *DIST_BASE.get(dsym).ok_or(Error::BadDistance)?;
                let dist_extra = *DIST_EXTRA.get(dsym).ok_or(Error::BadDistance)?;
                let dextra = usize::try_from(br.bits(u32::from(dist_extra))?)
                    .map_err(|_| Error::BadDistance)?;
                let distance = usize::from(dist_base)
                    .checked_add(dextra)
                    .ok_or(Error::BadDistance)?;
                if distance == 0 || distance > out.len() {
                    return Err(Error::BadDistance);
                }
                let start = out.len().checked_sub(distance).ok_or(Error::BadDistance)?;
                let new_len = out.len().checked_add(len).ok_or(Error::OutputTooLarge)?;
                if new_len > max_output {
                    return Err(Error::OutputTooLarge);
                }
                for i in 0..len {
                    let idx = start.checked_add(i).ok_or(Error::BadDistance)?;
                    let byte = *out.get(idx).ok_or(Error::BadDistance)?;
                    out.push(byte);
                }
            }
            _ => return Err(Error::BadLength),
        }
    }
}

/// Inflates a raw DEFLATE (RFC 1951) stream (no zlib/gzip header or trailer), stopping with
/// [`Error::OutputTooLarge`] rather than growing past `max_output` bytes.
pub(crate) fn inflate(data: &[u8], max_output: usize) -> Result<Vec<u8>, Error> {
    let mut br = BitReader::new(data);
    let mut out = Vec::new();
    loop {
        let last = br.bits(1)? == 1;
        let btype = br.bits(2)?;
        match btype {
            0 => {
                br.align_to_byte();
                let len_bytes = br.take_bytes(4)?;
                let arr: [u8; 4] = len_bytes.try_into().map_err(|_| Error::UnexpectedEof)?;
                let [b0, b1, b2, b3] = arr;
                let len = u16::from_le_bytes([b0, b1]);
                let nlen = u16::from_le_bytes([b2, b3]);
                if len != !nlen {
                    return Err(Error::BadStoredLen);
                }
                let chunk = br.take_bytes(usize::from(len))?;
                push_checked(&mut out, chunk, max_output)?;
            }
            1 => {
                let (lit, dist) = fixed_tables()?;
                inflate_block(&mut br, &lit, &dist, &mut out, max_output)?;
            }
            2 => {
                let (lit, dist) = dynamic_tables(&mut br)?;
                inflate_block(&mut br, &lit, &dist, &mut out, max_output)?;
            }
            _ => return Err(Error::BadBlockType),
        }
        if last {
            break;
        }
    }
    Ok(out)
}
