//! [integration] tests crossing `email` and `extract`: a raw RFC 5322 message parsed by
//! [`Envelope::parse`] and handed to [`PdfTextExtractor`]. Test fn names carry the spec
//! criterion they satisfy: `acN_<behavior>`.

mod common;

use hauz_core::email::Envelope;
use hauz_core::extract::{Chain, Error, Extractor, PdfTextExtractor, TextExtractor, merge};
use time::macros::date;

/// Boxed so any error type propagates with `?`; tests never unwrap or expect.
type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

const HTML_PDF: &[u8] = include_bytes!("fixtures/html_pdf.eml");

#[tokio::test]
async fn ac7_fake_pdf_attachment_from_eml_is_an_error() -> Result<()> {
    let envelope = Envelope::parse(HTML_PDF)?;
    let result = PdfTextExtractor.extract(&envelope).await;
    assert!(matches!(result, Err(Error::Pdf { document: 0, .. })));
    Ok(())
}

#[tokio::test]
async fn ac8_seven_bit_mime_message_with_pdf_attachment_extracts_ac1_fields() -> Result<()> {
    let lines = ["Total: 1,234.56 EUR", "Due date: 15/10/2026"];
    let pdf_bytes = common::minimal_pdf(&lines);
    let pdf_ascii = String::from_utf8(pdf_bytes)?;

    let raw = format!(
        "From: billing@example.com\r\n\
         Subject: Invoice\r\n\
         Date: Mon, 1 Jan 2024 12:00:00 +0000\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: multipart/mixed; boundary=\"b1\"\r\n\
         \r\n\
         --b1\r\n\
         Content-Type: text/plain\r\n\
         \r\n\
         Please see the attached invoice.\r\n\
         \r\n\
         --b1\r\n\
         Content-Type: application/pdf; name=\"invoice.pdf\"\r\n\
         Content-Disposition: attachment; filename=\"invoice.pdf\"\r\n\
         Content-Transfer-Encoding: 7bit\r\n\
         \r\n\
         {pdf_ascii}\r\n\
         --b1--\r\n"
    );

    let envelope = Envelope::parse(raw.as_bytes())?;
    let extraction = PdfTextExtractor.extract(&envelope).await?;

    let amount = extraction.amount.ok_or("expected amount")?;
    assert_eq!(
        amount.value,
        hauz_core::bill::Money::new(123_456, hauz_core::bill::Currency::new("EUR")?)
    );

    let due = extraction.due.ok_or("expected due")?;
    assert_eq!(due.value, date!(2026 - 10 - 15));
    Ok(())
}

/// AC9: `Chain([TextExtractor, PdfTextExtractor])` extracting a 7-bit `multipart/mixed`
/// message (text part with an amount, PDF part with a due date) equals running the two
/// extractors separately and `merge`-ing them, with both fields present in the result.
#[tokio::test]
async fn ac9_chain_equals_merge_of_separate_runs() -> Result<()> {
    let pdf_bytes = common::minimal_pdf(&["Due date: 15/10/2026"]);
    let pdf_ascii = String::from_utf8(pdf_bytes)?;

    let raw = format!(
        "From: billing@example.com\r\n\
         Subject: Invoice\r\n\
         Date: Mon, 1 Jan 2024 12:00:00 +0000\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: multipart/mixed; boundary=\"b1\"\r\n\
         \r\n\
         --b1\r\n\
         Content-Type: text/plain\r\n\
         \r\n\
         Total: 1,234.56 EUR\r\n\
         \r\n\
         --b1\r\n\
         Content-Type: application/pdf; name=\"invoice.pdf\"\r\n\
         Content-Disposition: attachment; filename=\"invoice.pdf\"\r\n\
         Content-Transfer-Encoding: 7bit\r\n\
         \r\n\
         {pdf_ascii}\r\n\
         --b1--\r\n"
    );

    let envelope = Envelope::parse(raw.as_bytes())?;

    let text = TextExtractor.extract(&envelope).await?;
    let pdf = PdfTextExtractor.extract(&envelope).await?;
    let expected = merge(vec![text, pdf]);

    let chain = Chain::new(vec![Box::new(TextExtractor), Box::new(PdfTextExtractor)]);
    assert_eq!(chain.extract(&envelope).await, Ok(expected.clone()));

    let amount = expected.amount.ok_or("expected amount")?;
    assert_eq!(
        amount.value,
        hauz_core::bill::Money::new(123_456, hauz_core::bill::Currency::new("EUR")?)
    );
    let due = expected.due.ok_or("expected due")?;
    assert_eq!(due.value, date!(2026 - 10 - 15));
    Ok(())
}
