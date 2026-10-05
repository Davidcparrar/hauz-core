//! Shared fixtures and `&dyn BillStore` scenario fns for `store`'s [unit] and [integration]
//! tests. Each `tests/*.rs` file compiles this module separately, so a helper unused by one
//! file is still valid in the other.
#![allow(dead_code)] // not every scenario/fixture fn is used by both test files

use hauz_core::bill::{Bill, BillDraft, BillId, BillingPeriod, Currency, Money, Status, Vendor};
use hauz_core::store::{BillStore, Error, InsertOutcome, RawHash};
use time::macros::date;

pub(crate) type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

/// A raw hash filled with `byte`, distinguishable per test scenario.
pub(crate) fn hash(byte: u8) -> RawHash {
    RawHash::new([byte; 32])
}

/// A complete, `Extracted` bill with the given id.
pub(crate) fn extracted_bill(id: &str) -> Result<Bill> {
    let draft = BillDraft {
        id: BillId::new(id)?,
        vendor: Some(Vendor::new("Acme Power")?),
        amount: Some(Money::new(1_234, Currency::new("USD")?)),
        period: Some(BillingPeriod::new(
            date!(2026 - 01 - 01),
            date!(2026 - 01 - 31),
        )?),
        issued: Some(date!(2026 - 01 - 05)),
        due: Some(date!(2026 - 02 - 15)),
        status: Status::Extracted,
    };
    Ok(Bill::try_from(draft)?)
}

/// A `NeedsReview` bill with the given id and every optional field `None`.
pub(crate) fn bare_needs_review_bill(id: &str) -> Result<Bill> {
    let draft = BillDraft {
        id: BillId::new(id)?,
        vendor: None,
        amount: None,
        period: None,
        issued: None,
        due: None,
        status: Status::NeedsReview,
    };
    Ok(Bill::try_from(draft)?)
}

/// `ingest`'s AC1 fixture: a 7-bit `text/plain` message from `billing@acme-power.example`,
/// body `Total: 1,234.56 EUR` / `Due date: 15/10/2026` — so `TextExtractor` finds an anchored
/// amount and due date, plus a sender-domain vendor, but never a period (no shipped extractor
/// sets one).
pub(crate) fn bill_eml() -> Vec<u8> {
    "From: billing@acme-power.example\r\n\
     Subject: Your bill\r\n\
     Date: Mon, 1 Jan 2024 12:00:00 +0000\r\n\
     Content-Type: text/plain\r\n\
     Content-Transfer-Encoding: 7bit\r\n\
     \r\n\
     Total: 1,234.56 EUR\r\n\
     Due date: 15/10/2026\r\n"
        .as_bytes()
        .to_vec()
}

/// Standard zip/ISO-HDLC CRC-32, computed independently of the crate under test.
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
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
/// headers, central directory, and EOCD, with CRC-32 computed by this module's own `crc32`.
/// Shared by `property_zip.rs` (AC8/AC9) and `extract`'s AC5/AC10 tests (#27).
pub(crate) fn build_stored_zip(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
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

/// A unique tmp-file path for a `SqliteStore` under test, so parallel tests never collide.
fn tmp_db_path(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("hauz-core-store-{name}-{nanos}.sqlite3"))
}

/// Owns a unique tmp-file path for a `SqliteStore` under test. Its `Drop` removes the file
/// and its `-wal`/`-shm` sidecars, so cleanup happens on the test's normal return AND on an
/// early `?` — never rely on cleanup code at the end of a test fn.
pub(crate) struct TmpDbFile {
    pub(crate) path: std::path::PathBuf,
}

impl TmpDbFile {
    /// A fresh, unused path named after `name` (so parallel tests never collide).
    pub(crate) fn new(name: &str) -> Self {
        Self {
            path: tmp_db_path(name),
        }
    }

    fn sidecar(&self, suffix: &str) -> std::path::PathBuf {
        let mut os = self.path.clone().into_os_string();
        os.push(suffix);
        std::path::PathBuf::from(os)
    }
}

impl Drop for TmpDbFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.sidecar("-wal"));
        let _ = std::fs::remove_file(self.sidecar("-shm"));
    }
}

/// AC1: inserting a `Bill` under a fresh hash returns `Inserted(id)`, and `get(id)` returns an
/// equal `Bill`.
pub(crate) async fn ac1_insert_then_get(store: &dyn BillStore) -> Result<()> {
    let bill = extracted_bill("bill-1")?;
    let outcome = store.insert(&hash(1), &bill).await?;
    assert_eq!(outcome, InsertOutcome::Inserted(bill.id().clone()));
    assert_eq!(store.get(bill.id()).await?, Some(bill));
    Ok(())
}

/// AC2: `get` and `find_by_hash` return `Ok(None)` for an unknown key.
pub(crate) async fn ac2_unknown_key_is_none(store: &dyn BillStore) -> Result<()> {
    let unknown_id = BillId::new("does-not-exist")?;
    assert_eq!(store.get(&unknown_id).await?, None);
    assert_eq!(store.find_by_hash(&hash(99)).await?, None);
    Ok(())
}

/// AC3: a second insert under a stored hash returns `Duplicate(first id)` and stores nothing.
pub(crate) async fn ac3_second_insert_under_stored_hash_is_duplicate(
    store: &dyn BillStore,
) -> Result<()> {
    let first = extracted_bill("bill-1")?;
    let h = hash(2);
    store.insert(&h, &first).await?;

    let second = extracted_bill("bill-2")?;
    let outcome = store.insert(&h, &second).await?;
    assert_eq!(outcome, InsertOutcome::Duplicate(first.id().clone()));

    assert_eq!(store.list().await?.len(), 1);
    assert_eq!(store.find_by_hash(&h).await?, Some(first));
    Ok(())
}

/// AC4: inserting a `Bill` whose id exists under another hash returns `Error::DuplicateId` and
/// stores nothing.
pub(crate) async fn ac4_insert_with_known_id_under_new_hash_is_rejected(
    store: &dyn BillStore,
) -> Result<()> {
    let bill = extracted_bill("bill-1")?;
    store.insert(&hash(3), &bill).await?;

    let result = store.insert(&hash(4), &bill).await;
    assert!(matches!(result, Err(Error::DuplicateId(ref id)) if *id == *bill.id()));
    assert_eq!(store.list().await?.len(), 1);
    Ok(())
}

/// AC5: `list` returns bills in insertion order, regardless of id lexical order.
pub(crate) async fn ac5_list_returns_insertion_order(store: &dyn BillStore) -> Result<()> {
    let first = extracted_bill("zzz")?;
    let second = extracted_bill("mmm")?;
    let third = extracted_bill("aaa")?;
    store.insert(&hash(10), &first).await?;
    store.insert(&hash(11), &second).await?;
    store.insert(&hash(12), &third).await?;

    let ids: Vec<_> = store
        .list()
        .await?
        .into_iter()
        .map(|b| b.id().clone())
        .collect();
    assert_eq!(
        ids,
        vec![first.id().clone(), second.id().clone(), third.id().clone()]
    );
    Ok(())
}

/// AC6: a `NeedsReview` bill with every optional field `None` round-trips unchanged.
pub(crate) async fn ac6_bare_needs_review_bill_round_trips(store: &dyn BillStore) -> Result<()> {
    let bill = bare_needs_review_bill("bill-bare")?;
    store.insert(&hash(20), &bill).await?;
    assert_eq!(store.get(bill.id()).await?, Some(bill));
    Ok(())
}

// -----------------------------------------------------------------------------------------
// PDF fixture builders for `extract`'s `PdfTextExtractor` tests (std only, no dependency on
// `pdf-extract` or any PDF library): a minimal PDF-1.4, single page, base-14 Helvetica `/F1`,
// one `Tj` per line, with a hand-computed xref table so the file is byte-exact and valid.
// -----------------------------------------------------------------------------------------

/// Escapes `(`, `)` and `\` for use inside a PDF literal string `(...)`.
fn pdf_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '(' | ')' | '\\' => escaped.push('\\'),
            _ => {}
        }
        escaped.push(c);
    }
    escaped
}

/// The page content stream: `Tj` for each of `lines`, 16pt line spacing, top-down.
fn pdf_content_stream(lines: &[&str]) -> Vec<u8> {
    let mut stream = String::from("BT /F1 12 Tf 72 720 Td ");
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            stream.push_str("0 -16 Td ");
        }
        stream.push('(');
        stream.push_str(&pdf_escape(line));
        stream.push_str(") Tj ");
    }
    stream.push_str("ET");
    stream.into_bytes()
}

/// Assembles a well-formed single-page PDF-1.4 from a page `/Resources` dictionary body and a
/// content stream, laying out objects `1..=4` (catalog, pages, page, contents) plus any
/// `extra_objects` (e.g. a font or image XObject), with a correct xref table.
fn pdf_from_objects(resources: &str, content: &[u8], extra_objects: Vec<Vec<u8>>) -> Vec<u8> {
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources {resources} /Contents 4 0 R >>"
        )
        .into_bytes(),
    ];
    let mut contents_object = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    contents_object.extend_from_slice(content);
    contents_object.extend_from_slice(b"\nendstream");
    objects.push(contents_object);
    objects.extend(extra_objects);

    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = pdf.len();
    let entry_count = objects.len() + 1;
    pdf.extend_from_slice(format!("xref\n0 {entry_count}\n0000000000 65535 f \n").as_bytes());
    for offset in &offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size {entry_count} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF")
            .as_bytes(),
    );
    pdf
}

/// A single Helvetica base-14 font object (`/F1`), referenced by object number 5.
fn helvetica_font_object() -> Vec<u8> {
    b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec()
}

/// A valid, minimal single-page PDF-1.4: one `Tj` per line of `lines`, base-14 Helvetica
/// `/F1`, top-down at 16pt line spacing. Its text layer extracts to the lines joined by `\n`.
pub(crate) fn minimal_pdf(lines: &[&str]) -> Vec<u8> {
    let content = pdf_content_stream(lines);
    pdf_from_objects(
        "<< /Font << /F1 5 0 R >> >>",
        &content,
        vec![helvetica_font_object()],
    )
}

/// A valid PDF whose page draws a tiny image XObject and has no text at all: `pdf-extract`
/// returns `Ok("")` for it (no text layer, not an error).
pub(crate) fn image_only_pdf() -> Vec<u8> {
    let mut image_object =
        b"<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 >>\nstream\n"
            .to_vec();
    image_object.push(0x80);
    image_object.extend_from_slice(b"\nendstream");
    pdf_from_objects(
        "<< /Font << /F1 5 0 R >> /XObject << /Im1 6 0 R >> >>",
        b"q 100 0 0 100 0 0 cm /Im1 Do Q",
        vec![helvetica_font_object(), image_object],
    )
}

/// The same page and content stream as [`minimal_pdf`], except `/Resources` omits `/Font`
/// entirely while the content stream still selects `/F1 12 Tf`: `pdf-extract` panics looking
/// up a font it was never told about.
pub(crate) fn pdf_missing_font(lines: &[&str]) -> Vec<u8> {
    let content = pdf_content_stream(lines);
    pdf_from_objects("<< >>", &content, Vec::new())
}
