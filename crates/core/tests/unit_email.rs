//! [unit] tests for the `email` module's public API. One file per level per module.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

use hauz_core::email::{Envelope, Error, MimeType};
use time::macros::datetime;

/// Boxed so any error type propagates with `?`; tests never unwrap or expect.
type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

const PLAIN: &[u8] = include_bytes!("fixtures/plain.eml");
const HTML_PDF: &[u8] = include_bytes!("fixtures/html_pdf.eml");
const TWO_ATTACHMENTS: &[u8] = include_bytes!("fixtures/two_attachments.eml");
const MALFORMED: &[u8] = include_bytes!("fixtures/malformed.eml");
const NO_SENDER: &[u8] = include_bytes!("fixtures/no_sender.eml");

#[test]
fn ac1_single_part_text_plain() -> Result<()> {
    let envelope = Envelope::parse(PLAIN)?;

    assert_eq!(envelope.subject, Some("Hello".to_string()));
    assert_eq!(envelope.sender, "alice@example.com");
    assert_eq!(envelope.date, Some(datetime!(2024-01-01 12:00:00 UTC)));
    assert_eq!(envelope.text, Some("Hello world\n".to_string()));
    assert_eq!(envelope.html, None);
    assert_eq!(envelope.documents, Vec::new());
    Ok(())
}

#[test]
fn ac2_multipart_alternative_plus_pdf_attachment() -> Result<()> {
    let envelope = Envelope::parse(HTML_PDF)?;

    assert!(envelope.text.is_some());
    assert!(envelope.html.is_some());
    let [doc] = envelope.documents.as_slice() else {
        return Err("expected exactly one document".into());
    };
    assert_eq!(doc.mime, MimeType::new("application/pdf")?);
    assert_eq!(doc.filename, Some("invoice.pdf".to_string()));
    assert_eq!(doc.bytes, b"%PDF-1.4 fake".to_vec());
    Ok(())
}

#[test]
fn ac3_html_only_body_with_nested_and_sibling_attachments() -> Result<()> {
    let envelope = Envelope::parse(TWO_ATTACHMENTS)?;

    assert!(envelope.html.is_some());
    assert!(envelope.text.is_some());
    let [pdf, csv, png] = envelope.documents.as_slice() else {
        return Err("expected exactly three documents".into());
    };

    assert_eq!(pdf.mime, MimeType::new("application/pdf")?);
    assert_eq!(pdf.filename, Some("doc.pdf".to_string()));
    assert_eq!(pdf.bytes, b"%PDF-1.4 fake".to_vec());

    assert_eq!(csv.mime, MimeType::new("text/csv")?);
    assert_eq!(csv.filename, Some("data.csv".to_string()));
    assert_eq!(csv.bytes, b"a,b,c\r\n1,2,3".to_vec());

    assert_eq!(png.mime, MimeType::new("image/png")?);
    assert_eq!(png.filename, None);
    assert_eq!(png.bytes, b"PNGDATA".to_vec());
    Ok(())
}

#[test]
fn ac4_no_header_section_is_malformed() {
    assert_eq!(Envelope::parse(MALFORMED), Err(Error::Malformed));
}

#[test]
fn ac5_no_from_or_sender_addr_spec_is_missing_sender() {
    assert_eq!(Envelope::parse(NO_SENDER), Err(Error::MissingSender));
}

#[test]
fn ac6_missing_subject_and_unparsable_date_are_none() -> Result<()> {
    let raw = b"From: bob@example.com\r\nDate: not-a-date\r\n\r\nBody\r\n";
    let envelope = Envelope::parse(raw)?;

    assert_eq!(envelope.subject, None);
    assert_eq!(envelope.date, None);
    Ok(())
}

#[test]
fn ac7_mime_type_new_lowercases_and_rejects_malformed_values() {
    let mime = MimeType::new("Application/PDF").expect("valid mime type");
    assert_eq!(mime.as_str(), "application/pdf");

    for raw in ["", "pdf", "/pdf", "a/b/c", "text/plain; charset=utf-8"] {
        assert!(
            matches!(MimeType::new(raw), Err(Error::InvalidMimeType(_))),
            "input: {raw:?}"
        );
    }
}
