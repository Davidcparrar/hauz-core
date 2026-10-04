//! Hand-rolled RFC 1951 ("raw deflate") inflater. std only, spike-quality.
//! Ported from the structure of Mark Adler's `puff.c` reference decoder.

#[derive(Debug)]
pub enum Error {
    UnexpectedEof,
    BadBlockType,
    BadStoredLen,
    BadHuffmanCode,
    BadCodeLengths,
    BadDistance,
    BadLength,
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
        Self { data, pos: 0, bitbuf: 0, bitcnt: 0 }
    }

    fn bits(&mut self, n: u32) -> Result<u32, Error> {
        while self.bitcnt < n {
            let byte = *self.data.get(self.pos).ok_or(Error::UnexpectedEof)?;
            self.pos += 1;
            self.bitbuf |= (byte as u32) << self.bitcnt;
            self.bitcnt += 8;
        }
        let out = if n == 0 { 0 } else { self.bitbuf & ((1u32 << n) - 1) };
        self.bitbuf >>= n;
        self.bitcnt -= n;
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

/// Canonical Huffman decode table: `count[len]` codes of each length, symbols
/// listed in canonical order.
struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

fn build_huffman(lengths: &[u8]) -> Huffman {
    let mut count = [0u16; 16];
    for &l in lengths {
        count[l as usize] += 1;
    }
    count[0] = 0;
    let mut offs = [0u16; 16];
    for len in 1..16 {
        offs[len] = offs[len - 1] + count[len - 1];
    }
    let mut symbol = vec![0u16; lengths.len()];
    for (sym, &l) in lengths.iter().enumerate() {
        if l != 0 {
            symbol[offs[l as usize] as usize] = sym as u16;
            offs[l as usize] += 1;
        }
    }
    Huffman { count, symbol }
}

fn decode(br: &mut BitReader, h: &Huffman) -> Result<u16, Error> {
    let mut code: i32 = 0;
    let mut first: i32 = 0;
    let mut index: i32 = 0;
    for len in 1..16usize {
        code |= br.bits(1)? as i32;
        let cnt = h.count[len] as i32;
        if code - first < cnt {
            return Ok(h.symbol[(index + (code - first)) as usize]);
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

fn fixed_tables() -> (Huffman, Huffman) {
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
    (build_huffman(&lit_lens), build_huffman(&dist_lens))
}

const CLEN_ORDER: [usize; 19] =
    [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

fn dynamic_tables(br: &mut BitReader) -> Result<(Huffman, Huffman), Error> {
    let hlit = br.bits(5)? as usize + 257;
    let hdist = br.bits(5)? as usize + 1;
    let hclen = br.bits(4)? as usize + 4;

    let mut clen_lengths = [0u8; 19];
    for &idx in CLEN_ORDER.iter().take(hclen) {
        clen_lengths[idx] = br.bits(3)? as u8;
    }
    let clen_table = build_huffman(&clen_lengths);

    let mut lengths = Vec::with_capacity(hlit + hdist);
    while lengths.len() < hlit + hdist {
        let sym = decode(br, &clen_table)?;
        match sym {
            0..=15 => lengths.push(sym as u8),
            16 => {
                let prev = *lengths.last().ok_or(Error::BadCodeLengths)?;
                let rep = br.bits(2)? + 3;
                for _ in 0..rep {
                    lengths.push(prev);
                }
            }
            17 => {
                let rep = br.bits(3)? + 3;
                for _ in 0..rep {
                    lengths.push(0);
                }
            }
            18 => {
                let rep = br.bits(7)? + 11;
                for _ in 0..rep {
                    lengths.push(0);
                }
            }
            _ => return Err(Error::BadCodeLengths),
        }
    }
    if lengths.len() != hlit + hdist {
        return Err(Error::BadCodeLengths);
    }
    let lit_table = build_huffman(&lengths[..hlit]);
    let dist_table = build_huffman(&lengths[hlit..]);
    Ok((lit_table, dist_table))
}

fn inflate_block(
    br: &mut BitReader,
    lit: &Huffman,
    dist: &Huffman,
    out: &mut Vec<u8>,
) -> Result<(), Error> {
    loop {
        let sym = decode(br, lit)?;
        match sym {
            0..=255 => out.push(sym as u8),
            256 => return Ok(()),
            257..=285 => {
                let i = (sym - 257) as usize;
                let len = LEN_BASE[i] as usize + br.bits(LEN_EXTRA[i] as u32)? as usize;
                let dsym = decode(br, dist)? as usize;
                if dsym >= DIST_BASE.len() {
                    return Err(Error::BadDistance);
                }
                let distance =
                    DIST_BASE[dsym] as usize + br.bits(DIST_EXTRA[dsym] as u32)? as usize;
                if distance == 0 || distance > out.len() {
                    return Err(Error::BadDistance);
                }
                let start = out.len() - distance;
                for i in 0..len {
                    let byte = out[start + i];
                    out.push(byte);
                }
            }
            _ => return Err(Error::BadLength),
        }
    }
}

/// Inflate a raw DEFLATE (RFC 1951) stream: no zlib/gzip header or trailer.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut br = BitReader::new(data);
    let mut out = Vec::new();
    loop {
        let last = br.bits(1)? == 1;
        let btype = br.bits(2)?;
        match btype {
            0 => {
                br.align_to_byte();
                let len_bytes = br.take_bytes(4)?;
                let len = u16::from_le_bytes([len_bytes[0], len_bytes[1]]);
                let nlen = u16::from_le_bytes([len_bytes[2], len_bytes[3]]);
                if len != !nlen {
                    return Err(Error::BadStoredLen);
                }
                out.extend_from_slice(br.take_bytes(len as usize)?);
            }
            1 => {
                let (lit, dist) = fixed_tables();
                inflate_block(&mut br, &lit, &dist, &mut out)?;
            }
            2 => {
                let (lit, dist) = dynamic_tables(&mut br)?;
                inflate_block(&mut br, &lit, &dist, &mut out)?;
            }
            _ => return Err(Error::BadBlockType),
        }
        if last {
            break;
        }
    }
    Ok(out)
}
