//! [property] tests for the `zip` module's public API. One file per level per module.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

use hauz_core::zip::{self, Entry};
use proptest::prelude::*;

/// Standard zip/ISO-HDLC CRC-32, computed independently of the crate under test.
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

/// Builds a minimal, well-formed zip archive (method 0, stored) from `entries`: local
/// headers, central directory, and EOCD, with CRC-32 computed by this test's own `crc32`.
fn build_stored_zip(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut offsets = Vec::new();

    for (name, data) in entries {
        offsets.push(u32::try_from(out.len()).unwrap_or(u32::MAX));
        let crc = crc32(data);
        out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        out.extend_from_slice(&0u16.to_le_bytes()); // mod date
        out.extend_from_slice(&crc.to_le_bytes());
        let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&len.to_le_bytes()); // comp size
        out.extend_from_slice(&len.to_le_bytes()); // uncomp size
        let name_len = u16::try_from(name.len()).unwrap_or(u16::MAX);
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
    }

    let mut central = Vec::new();
    for ((name, data), &local_offset) in entries.iter().zip(offsets.iter()) {
        let crc = crc32(data);
        central.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central.extend_from_slice(&0u16.to_le_bytes()); // flags
        central.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        central.extend_from_slice(&0u16.to_le_bytes()); // mod time
        central.extend_from_slice(&0u16.to_le_bytes()); // mod date
        central.extend_from_slice(&crc.to_le_bytes());
        let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
        central.extend_from_slice(&len.to_le_bytes());
        central.extend_from_slice(&len.to_le_bytes());
        let name_len = u16::try_from(name.len()).unwrap_or(u16::MAX);
        central.extend_from_slice(&name_len.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra len
        central.extend_from_slice(&0u16.to_le_bytes()); // comment len
        central.extend_from_slice(&0u16.to_le_bytes()); // disk number start
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&local_offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }

    let cd_offset = u32::try_from(out.len()).unwrap_or(u32::MAX);
    let cd_size = u32::try_from(central.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&central);

    out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
    out.extend_from_slice(&0u16.to_le_bytes()); // disk number
    out.extend_from_slice(&0u16.to_le_bytes()); // disk with central directory
    let count = u16::try_from(entries.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&count.to_le_bytes()); // entries on this disk
    out.extend_from_slice(&count.to_le_bytes()); // total entries
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment len

    out
}

/// 0..=4 entries with short ASCII names and small payloads.
fn arb_entries() -> impl Strategy<Value = Vec<(String, Vec<u8>)>> {
    proptest::collection::vec(
        (
            "[A-Za-z0-9]{1,16}",
            proptest::collection::vec(any::<u8>(), 0..=2048),
        ),
        0..=4,
    )
}

proptest! {
    #[test]
    fn ac8_stored_archive_round_trips(entries in arb_entries()) {
        let archive = build_stored_zip(&entries);
        let result = zip::read(&archive).expect("a well-formed stored archive always reads");
        let expected: Vec<Entry> = entries
            .into_iter()
            .map(|(name, bytes)| Entry { name, bytes })
            .collect();
        prop_assert_eq!(result, expected);
    }

    #[test]
    fn ac9_read_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..=4096)) {
        let _ = zip::read(&bytes);
    }
}
