//! [unit] tests for the `extract` module's public API. One file per level per module.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

mod common;

use std::collections::BTreeSet;

use hauz_core::bill::{BillingPeriod, Currency, Money, Vendor};
use hauz_core::email::{Document, Envelope, MimeType};
use hauz_core::extract::{
    Chain, Confidence, Error, Escalate, Extraction, Extractor, Field, Note, PdfTextExtractor,
    Source, Span, TextExtractor, XmlInvoiceExtractor, merge,
};
use time::macros::date;

/// Boxed so any error type propagates with `?`; tests never unwrap or expect.
type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

const DE_TOTAL: &str = include_str!("fixtures/extract/de_total.txt");
const US_TOTAL: &str = include_str!("fixtures/extract/us_total.txt");
const FR_SPACE: &str = include_str!("fixtures/extract/fr_space.txt");
const ES_TABLE: &str = include_str!("fixtures/extract/es_table.html");
const NOISE: &str = include_str!("fixtures/extract/noise.txt");
const CO_BARE_DOLLAR: &str = include_str!("fixtures/extract/co_bare_dollar.txt");

const DIAN_FULL_ZIP: &[u8] = include_bytes!("fixtures/ubl/dian_full.zip");
const DIAN_NO_PERIOD_ZIP: &[u8] = include_bytes!("fixtures/ubl/dian_no_period.zip");
const NOT_INVOICE_ZIP: &[u8] = include_bytes!("fixtures/ubl/not_invoice.zip");
const BZIP2_ZIP: &[u8] = include_bytes!("fixtures/zip/bzip2.zip");
const DIAN_FULL_XML: &str = include_str!("fixtures/ubl/dian_full.xml");

/// Wraps `bytes` as a single `application/zip` document in an otherwise-empty envelope.
fn zip_envelope(bytes: &[u8]) -> Result<Envelope> {
    Ok(Envelope {
        subject: None,
        sender: "facturacion@acme-luz.example".to_string(),
        date: None,
        text: None,
        html: None,
        documents: vec![Document {
            mime: MimeType::new("application/zip")?,
            filename: None,
            bytes: bytes.to_vec(),
        }],
    })
}

/// An otherwise-empty envelope with just a sender and, optionally, a text/html body.
fn envelope(sender: &str, text: Option<&str>, html: Option<&str>) -> Envelope {
    Envelope {
        subject: None,
        sender: sender.to_string(),
        date: None,
        text: text.map(str::to_string),
        html: html.map(str::to_string),
        documents: Vec::new(),
    }
}

#[tokio::test]
async fn ac1_german_text_anchored_fields() -> Result<()> {
    let envelope = envelope("rechnung@stadtwerke.de", Some(DE_TOTAL), None);
    let extraction = TextExtractor.extract(&envelope).await?;

    let amount = extraction.amount.ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(123_456, Currency::new("EUR")?));
    assert_eq!(&DE_TOTAL[amount.span.start..amount.span.end], "1.234,56 €");

    let issued = extraction.issued.ok_or("expected issued")?;
    assert_eq!(issued.value, date!(2026 - 09 - 24));
    assert_eq!(&DE_TOTAL[issued.span.start..issued.span.end], "24.09.2026");

    let due = extraction.due.ok_or("expected due")?;
    assert_eq!(due.value, date!(2026 - 10 - 15));
    assert_eq!(&DE_TOTAL[due.span.start..due.span.end], "15.10.2026");

    assert_eq!(extraction.period, None);

    let vendor = extraction.vendor.ok_or("expected vendor")?;
    assert_eq!(vendor.value, Vendor::new("stadtwerke.de")?);
    Ok(())
}

#[tokio::test]
async fn ac2_us_text_anchored_fields() -> Result<()> {
    let envelope = envelope("billing@example.com", Some(US_TOTAL), None);
    let extraction = TextExtractor.extract(&envelope).await?;

    let amount = extraction.amount.ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(123_456, Currency::new("USD")?));

    let issued = extraction.issued.ok_or("expected issued")?;
    assert_eq!(issued.value, date!(2026 - 09 - 24));

    let due = extraction.due.ok_or("expected due")?;
    assert_eq!(due.value, date!(2026 - 10 - 15));
    Ok(())
}

#[tokio::test]
async fn ac3_french_nbsp_grouping_no_issued() -> Result<()> {
    let envelope = envelope("factures@example.fr", Some(FR_SPACE), None);
    let extraction = TextExtractor.extract(&envelope).await?;

    let amount = extraction.amount.ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(123_456, Currency::new("EUR")?));

    let due = extraction.due.ok_or("expected due")?;
    assert_eq!(due.value, date!(2026 - 10 - 15));

    assert_eq!(extraction.issued, None);
    Ok(())
}

#[tokio::test]
async fn ac4_html_table_amount_and_due() -> Result<()> {
    let envelope = envelope("facturas@example.es", None, Some(ES_TABLE));
    let extraction = TextExtractor.extract(&envelope).await?;

    let amount = extraction.amount.ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(12_100, Currency::new("EUR")?));
    assert_eq!(amount.span.source, Source::Html);

    let due = extraction.due.ok_or("expected due")?;
    assert_eq!(due.value, date!(2026 - 10 - 15));
    Ok(())
}

#[tokio::test]
async fn ac5_noise_picks_largest_below_anchored_confidence() -> Result<()> {
    let anchored = envelope("billing@example.com", Some(DE_TOTAL), None);
    let anchored_amount = TextExtractor
        .extract(&anchored)
        .await?
        .amount
        .ok_or("expected anchored amount")?;

    let noisy = envelope("billing@example.com", Some(NOISE), None);
    let extraction = TextExtractor.extract(&noisy).await?;
    let amount = extraction.amount.ok_or("expected amount")?;

    assert_eq!(amount.value, Money::new(50_000, Currency::new("USD")?));
    assert!(amount.confidence < anchored_amount.confidence);
    Ok(())
}

#[tokio::test]
async fn ac6_empty_body_only_vendor_and_no_at_sign_no_vendor() -> Result<()> {
    let empty_body = envelope(
        "billing@example.com",
        Some("Thanks for your business."),
        None,
    );
    let extraction = TextExtractor.extract(&empty_body).await?;

    assert_eq!(extraction.amount, None);
    assert_eq!(extraction.issued, None);
    assert_eq!(extraction.due, None);
    assert_eq!(extraction.period, None);
    assert!(extraction.vendor.is_some());

    let no_at = envelope("not-an-address", Some("Thanks for your business."), None);
    let extraction = TextExtractor.extract(&no_at).await?;
    assert_eq!(extraction.vendor, None);
    Ok(())
}

#[test]
fn ac7_confidence_bounds() {
    assert_eq!(Confidence::new(101), Err(Error::InvalidConfidence(101)));
    assert!(Confidence::new(0).is_ok());
    assert!(Confidence::new(100).is_ok());
}

#[test]
fn ac8_merge_keeps_higher_confidence_and_union_of_fields() -> Result<()> {
    let low_span = Span {
        source: Source::Text,
        start: 0,
        end: 4,
    };
    let high_span = Span {
        source: Source::Text,
        start: 10,
        end: 14,
    };

    let low = Extraction {
        amount: Some(Field {
            value: Money::new(100, Currency::new("EUR")?),
            confidence: Confidence::new(40)?,
            span: low_span,
        }),
        issued: None,
        due: Some(Field {
            value: date!(2026 - 01 - 01),
            confidence: Confidence::new(90)?,
            span: high_span,
        }),
        period: None,
        vendor: None,
        notes: BTreeSet::new(),
    };
    let high = Extraction {
        amount: Some(Field {
            value: Money::new(999, Currency::new("USD")?),
            confidence: Confidence::new(90)?,
            span: high_span,
        }),
        issued: None,
        due: None,
        period: None,
        vendor: None,
        notes: BTreeSet::new(),
    };

    let merged = merge(vec![low, high]);
    assert_eq!(
        merged.amount.ok_or("expected amount")?.value,
        Money::new(999, Currency::new("USD")?)
    );
    assert_eq!(
        merged.due.ok_or("expected due")?.value,
        date!(2026 - 01 - 01)
    );
    assert_eq!(merged.issued, None);
    Ok(())
}

// ---------------------------------------------------------------------------------------
// A bare `$` is not a currency marker (feature #28)
// ---------------------------------------------------------------------------------------

/// AC1: a bare `$` next to a numeral is not a currency marker, so `TextExtractor` yields no
/// amount at all, while the due date and sender-domain vendor are unaffected.
#[tokio::test]
async fn ac1_bare_dollar_numeral_is_not_an_amount() -> Result<()> {
    let envelope = envelope(
        "facturacion@acme-energia.example",
        Some(CO_BARE_DOLLAR),
        None,
    );
    let extraction = TextExtractor.extract(&envelope).await?;

    assert_eq!(extraction.amount, None);

    let due = extraction.due.ok_or("expected due")?;
    assert_eq!(due.value, date!(2026 - 10 - 15));
    assert_eq!(due.confidence, Confidence::new(90)?);

    let vendor = extraction.vendor.ok_or("expected vendor")?;
    assert_eq!(vendor.value, Vendor::new("acme-energia.example")?);
    Ok(())
}

/// AC2: a code glued to the sign (`COP$`, with the span starting at the code's first letter;
/// `US$`) or standing beside the numeral (`$ ... USD`) still resolves to the right currency.
#[tokio::test]
async fn ac2_glued_or_adjacent_currency_code_resolves() -> Result<()> {
    let text = "Total a pagar: COP$ 1.234.567";
    let env = envelope("billing@example.com", Some(text), None);
    let amount = TextExtractor
        .extract(&env)
        .await?
        .amount
        .ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(123_456_700, Currency::new("COP")?));
    assert_eq!(amount.confidence, Confidence::new(90)?);
    assert_eq!(&text[amount.span.start..amount.span.end], "COP$ 1.234.567");

    let text = "Amount due: US$1,234.56";
    let env = envelope("billing@example.com", Some(text), None);
    let amount = TextExtractor
        .extract(&env)
        .await?
        .amount
        .ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(123_456, Currency::new("USD")?));

    let text = "Total: $ 1,234.56 USD";
    let env = envelope("billing@example.com", Some(text), None);
    let amount = TextExtractor
        .extract(&env)
        .await?
        .amount
        .ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(123_456, Currency::new("USD")?));
    Ok(())
}

/// AC3: the bare-`$` numeral is not a candidate at all, so the unanchored `EUR` amount past
/// the anchor window wins even though it is textually smaller.
#[tokio::test]
async fn ac3_bare_dollar_skipped_unanchored_fallback_wins() -> Result<()> {
    let text = "Total: $ 999.00\nReference from a previous statement, not a charge: 12.00 EUR";
    let envelope = envelope("billing@example.com", Some(text), None);
    let amount = TextExtractor
        .extract(&envelope)
        .await?
        .amount
        .ok_or("expected amount")?;

    assert_eq!(amount.value, Money::new(1_200, Currency::new("EUR")?));
    assert_eq!(amount.confidence, Confidence::new(40)?);
    Ok(())
}

// ---------------------------------------------------------------------------------------
// PdfTextExtractor
// ---------------------------------------------------------------------------------------

const PDF: &str = "application/pdf";
const CSV: &str = "text/csv";

fn pdf_document(bytes: Vec<u8>) -> Result<Document> {
    Ok(Document {
        mime: MimeType::new(PDF)?,
        filename: None,
        bytes,
    })
}

fn other_document(bytes: Vec<u8>) -> Result<Document> {
    Ok(Document {
        mime: MimeType::new(CSV)?,
        filename: None,
        bytes,
    })
}

/// An otherwise-empty envelope with just a sender and a list of documents.
fn envelope_with_documents(sender: &str, documents: Vec<Document>) -> Envelope {
    Envelope {
        subject: None,
        sender: sender.to_string(),
        date: None,
        text: None,
        html: None,
        documents,
    }
}

#[tokio::test]
async fn ac1_pdf_attachment_anchored_fields_with_document_span() -> Result<()> {
    let lines = ["Total: 1,234.56 EUR", "Due date: 15/10/2026"];
    let pdf_bytes = common::minimal_pdf(&lines);
    let text = PdfTextExtractor::text_layer(&pdf_bytes)?.ok_or("expected a text layer")?;

    let envelope = envelope_with_documents("billing@example.com", vec![pdf_document(pdf_bytes)?]);
    let extraction = PdfTextExtractor.extract(&envelope).await?;

    let amount = extraction.amount.ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(123_456, Currency::new("EUR")?));
    assert_eq!(amount.span.source, Source::Document(0));
    assert_eq!(&text[amount.span.start..amount.span.end], "1,234.56 EUR");

    let due = extraction.due.ok_or("expected due")?;
    assert_eq!(due.value, date!(2026 - 10 - 15));
    assert_eq!(due.span.source, Source::Document(0));
    assert_eq!(&text[due.span.start..due.span.end], "15/10/2026");

    assert!(extraction.notes.is_empty());
    assert_eq!(extraction.vendor, None);
    Ok(())
}

#[tokio::test]
async fn ac2_image_only_pdf_has_no_text_layer_note() -> Result<()> {
    let pdf_bytes = common::image_only_pdf();
    assert_eq!(PdfTextExtractor::text_layer(&pdf_bytes)?, None);

    let envelope = envelope_with_documents("billing@example.com", vec![pdf_document(pdf_bytes)?]);
    let extraction = PdfTextExtractor.extract(&envelope).await?;

    assert_eq!(extraction.amount, None);
    assert_eq!(extraction.issued, None);
    assert_eq!(extraction.due, None);
    assert_eq!(extraction.period, None);
    assert_eq!(extraction.vendor, None);
    assert_eq!(
        extraction.notes,
        BTreeSet::from([Note::NoTextLayer { document: 0 }])
    );
    Ok(())
}

#[tokio::test]
async fn ac3_corrupt_pdf_inputs_fail_without_panicking() -> Result<()> {
    let lines = ["Total: 1,234.56 EUR", "Due date: 15/10/2026"];
    let full_pdf = common::minimal_pdf(&lines);
    let half_len = full_pdf.len() / 2;
    let truncated = full_pdf
        .get(..half_len)
        .ok_or("expected a prefix")?
        .to_vec();

    let cases: Vec<Vec<u8>> = vec![
        b"%PDF-1.4 fake".to_vec(),
        Vec::new(),
        truncated,
        common::pdf_missing_font(&lines),
    ];

    for bytes in cases {
        let envelope = envelope_with_documents("billing@example.com", vec![pdf_document(bytes)?]);
        let result = PdfTextExtractor.extract(&envelope).await;
        assert!(matches!(result, Err(Error::Pdf { document: 0, .. })));
    }
    Ok(())
}

#[tokio::test]
async fn ac4_mixed_documents_ignore_csv_note_image_scan_text() -> Result<()> {
    let lines = ["Total: 1,234.56 EUR", "Due date: 15/10/2026"];
    let documents = vec![
        other_document(b"a,b,c".to_vec())?,
        pdf_document(common::image_only_pdf())?,
        pdf_document(common::minimal_pdf(&lines))?,
    ];
    let envelope = envelope_with_documents("billing@example.com", documents);
    let extraction = PdfTextExtractor.extract(&envelope).await?;

    assert_eq!(
        extraction.notes,
        BTreeSet::from([Note::NoTextLayer { document: 1 }])
    );

    let amount = extraction.amount.ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(123_456, Currency::new("EUR")?));
    assert_eq!(amount.span.source, Source::Document(2));

    let due = extraction.due.ok_or("expected due")?;
    assert_eq!(due.value, date!(2026 - 10 - 15));
    assert_eq!(due.span.source, Source::Document(2));
    Ok(())
}

#[tokio::test]
async fn ac5_empty_or_non_pdf_documents_yield_default_extraction() -> Result<()> {
    let empty = envelope_with_documents("billing@example.com", Vec::new());
    assert_eq!(
        PdfTextExtractor.extract(&empty).await?,
        Extraction::default()
    );

    let non_pdf = envelope_with_documents(
        "billing@example.com",
        vec![other_document(b"a,b,c".to_vec())?],
    );
    assert_eq!(
        PdfTextExtractor.extract(&non_pdf).await?,
        Extraction::default()
    );
    Ok(())
}

#[test]
fn ac6_merge_unions_notes() {
    let a = Extraction {
        notes: BTreeSet::from([Note::NoTextLayer { document: 0 }]),
        ..Extraction::default()
    };
    let a_and_b = Extraction {
        notes: BTreeSet::from([
            Note::NoTextLayer { document: 0 },
            Note::NoTextLayer { document: 1 },
        ]),
        ..Extraction::default()
    };

    let merged = merge(vec![a, a_and_b]);
    assert_eq!(
        merged.notes,
        BTreeSet::from([
            Note::NoTextLayer { document: 0 },
            Note::NoTextLayer { document: 1 },
        ])
    );
}

// ---------------------------------------------------------------------------------------
// Chain (feature #7)
// ---------------------------------------------------------------------------------------

/// Always returns a clone of the fixed extraction it was built with, ignoring the envelope.
#[derive(Clone)]
struct Fixed(Extraction);

impl Extractor for Fixed {
    fn extract<'a>(
        &'a self,
        _envelope: &'a Envelope,
    ) -> hauz_core::BoxFuture<'a, std::result::Result<Extraction, Error>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

/// Always fails, as if an internal confidence computation went out of range.
struct Failing;

impl Extractor for Failing {
    fn extract<'a>(
        &'a self,
        _envelope: &'a Envelope,
    ) -> hauz_core::BoxFuture<'a, std::result::Result<Extraction, Error>> {
        Box::pin(async move { Err(Error::InvalidConfidence(101)) })
    }
}

/// An `Extraction` with only `amount` set, at `confidence`.
fn amount_only(confidence: u8) -> Result<Extraction> {
    Ok(Extraction {
        amount: Some(Field {
            value: Money::new(123_456, Currency::new("EUR")?),
            confidence: Confidence::new(confidence)?,
            span: Span {
                source: Source::Text,
                start: 0,
                end: 4,
            },
        }),
        ..Extraction::default()
    })
}

/// An `Extraction` with `amount` and `vendor` set, at `confidence`.
fn amount_and_vendor(confidence: u8) -> Result<Extraction> {
    Ok(Extraction {
        vendor: Some(Field {
            value: Vendor::new("acme.example")?,
            confidence: Confidence::new(confidence)?,
            span: Span {
                source: Source::Text,
                start: 10,
                end: 14,
            },
        }),
        ..amount_only(confidence)?
    })
}

/// AC7: `Chain` of two `Fixed` extractors (one yields amount at 60, the other vendor and
/// amount at 40) extracts returns `merge(vec![a, b])`.
#[tokio::test]
async fn ac7_chain_merges_two_fixed_extractors() -> Result<()> {
    let envelope = envelope("billing@example.com", None, None);
    let a = amount_only(60)?;
    let b = amount_and_vendor(40)?;

    let chain = Chain::new(vec![Box::new(Fixed(a.clone())), Box::new(Fixed(b.clone()))]);
    let result = chain.extract(&envelope).await?;

    assert_eq!(result, merge(vec![a, b]));
    Ok(())
}

/// AC8: when any extractor in a `Chain` returns `Err`, `Chain::extract` returns that `Err`.
#[tokio::test]
async fn ac8_chain_propagates_first_err() -> Result<()> {
    let envelope = envelope("billing@example.com", None, None);
    let chain = Chain::new(vec![
        Box::new(Fixed(Extraction::default())),
        Box::new(Failing),
    ]);

    let result = chain.extract(&envelope).await;
    assert_eq!(result, Err(Error::InvalidConfidence(101)));
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Extraction::is_complete and Escalate (feature #21)
// ---------------------------------------------------------------------------------------

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A `Fixed` extractor that also counts how many times `extract` was called, via a shared
/// `AtomicUsize` the test can still read after the extractor is moved into a
/// `Box<dyn Extractor>`.
struct Counting(Extraction, Arc<AtomicUsize>);

impl Extractor for Counting {
    fn extract<'a>(
        &'a self,
        _envelope: &'a Envelope,
    ) -> hauz_core::BoxFuture<'a, std::result::Result<Extraction, Error>> {
        self.1.fetch_add(1, Ordering::SeqCst);
        let extraction = self.0.clone();
        Box::pin(async move { Ok(extraction) })
    }
}

/// A field for `period`, at `confidence`, so `is_complete`/`Escalate` tests can build a
/// complete extraction.
fn period_field(confidence: u8) -> Result<Field<BillingPeriod>> {
    Ok(Field {
        value: BillingPeriod::new(date!(2026 - 01 - 01), date!(2026 - 01 - 31))?,
        confidence: Confidence::new(confidence)?,
        span: Span {
            source: Source::Text,
            start: 0,
            end: 4,
        },
    })
}

/// An extraction with amount (at `confidence`), vendor, and period all set.
fn complete(confidence: u8) -> Result<Extraction> {
    Ok(Extraction {
        period: Some(period_field(90)?),
        ..amount_and_vendor(confidence)?
    })
}

/// AC1: `is_complete(50)` is true for amount at 50 (plus vendor and period), false at 49, and
/// false when vendor is absent, or period and issued are both absent.
#[test]
fn ac1_is_complete_at_threshold_below_and_missing_fields() -> Result<()> {
    assert!(complete(50)?.is_complete(50));
    assert!(!complete(49)?.is_complete(50));

    let mut no_vendor = complete(90)?;
    no_vendor.vendor = None;
    assert!(!no_vendor.is_complete(50));

    let mut no_period_no_issued = complete(90)?;
    no_period_no_issued.period = None;
    assert!(!no_period_no_issued.is_complete(50));
    Ok(())
}

/// A field for `issued`, so `is_complete` tests can complete an extraction without a period.
fn issued_field() -> Result<Field<time::Date>> {
    Ok(Field {
        value: date!(2026 - 01 - 05),
        confidence: Confidence::new(90)?,
        span: Span {
            source: Source::Text,
            start: 0,
            end: 4,
        },
    })
}

/// AC4 (#37): amount at 50, vendor and issued with no period is complete; neither period
/// nor issued, or amount at 49, is not.
#[test]
fn ac4_is_complete_with_issued_and_no_period() -> Result<()> {
    let with_issued = Extraction {
        issued: Some(issued_field()?),
        ..amount_and_vendor(50)?
    };
    assert!(with_issued.is_complete(50));

    assert!(!amount_and_vendor(50)?.is_complete(50));

    let below = Extraction {
        issued: Some(issued_field()?),
        ..amount_and_vendor(49)?
    };
    assert!(!below.is_complete(50));
    Ok(())
}

/// AC2: `Escalate`'s primary returns a complete extraction ⇒ returned unchanged, secondary
/// never called (count stays 0).
#[tokio::test]
async fn ac2_escalate_returns_complete_primary_without_calling_secondary() -> Result<()> {
    let envelope = envelope("billing@example.com", None, None);
    let primary = complete(90)?;
    let calls = Arc::new(AtomicUsize::new(0));
    let escalate = Escalate::new(
        Box::new(Fixed(primary.clone())),
        Box::new(Counting(Extraction::default(), calls.clone())),
        50,
    );

    let result = escalate.extract(&envelope).await?;
    assert_eq!(result, primary);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    Ok(())
}

/// AC3: primary amount-only at 80, secondary amount at 60 plus vendor, period, and a
/// `NoTextLayer` note ⇒ secondary called exactly once, result keeps the amount at 80 and the
/// secondary's vendor, period, and note.
#[tokio::test]
async fn ac3_escalate_calls_secondary_once_and_merges_when_primary_incomplete() -> Result<()> {
    let envelope = envelope("billing@example.com", None, None);
    let primary = amount_only(80)?;
    let mut secondary = amount_and_vendor(60)?;
    secondary.period = Some(period_field(60)?);
    secondary.notes.insert(Note::NoTextLayer { document: 0 });

    let calls = Arc::new(AtomicUsize::new(0));
    let escalate = Escalate::new(
        Box::new(Fixed(primary.clone())),
        Box::new(Counting(secondary.clone(), calls.clone())),
        50,
    );

    let result = escalate.extract(&envelope).await?;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(result, merge(vec![primary.clone(), secondary.clone()]));
    assert_eq!(result.amount, primary.amount);
    assert_eq!(result.vendor, secondary.vendor);
    assert_eq!(result.period, secondary.period);
    assert_eq!(result.notes, secondary.notes);
    Ok(())
}

/// AC4: the primary's `Err` propagates without ever calling the secondary; when the primary
/// is incomplete, the secondary's `Err` propagates too.
#[tokio::test]
async fn ac4_escalate_propagates_primary_err_without_calling_secondary() -> Result<()> {
    let envelope = envelope("billing@example.com", None, None);
    let calls = Arc::new(AtomicUsize::new(0));
    let escalate = Escalate::new(
        Box::new(Failing),
        Box::new(Counting(Extraction::default(), calls.clone())),
        50,
    );

    let result = escalate.extract(&envelope).await;
    assert_eq!(result, Err(Error::InvalidConfidence(101)));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn ac4_escalate_propagates_secondary_err_when_primary_incomplete() -> Result<()> {
    let envelope = envelope("billing@example.com", None, None);
    let escalate = Escalate::new(Box::new(Fixed(amount_only(40)?)), Box::new(Failing), 50);

    let result = escalate.extract(&envelope).await;
    assert_eq!(result, Err(Error::InvalidConfidence(101)));
    Ok(())
}

// -----------------------------------------------------------------------------------------
// XmlInvoiceExtractor (#27): exact fields from DIAN e-invoice zip attachments.
// -----------------------------------------------------------------------------------------

/// AC1: `dian_full.zip` as `application/zip` yields the AC1 fields at confidence 100, each
/// span landing on the matching element body in the entry's text, notes empty.
#[tokio::test]
async fn ac1_dian_full_zip_exact_fields() -> Result<()> {
    let envelope = zip_envelope(DIAN_FULL_ZIP)?;
    let extraction = XmlInvoiceExtractor.extract(&envelope).await?;

    let amount = extraction.amount.ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(18_435_000, Currency::new("COP")?));
    assert_eq!(amount.confidence.get(), 100);
    assert_eq!(amount.span.source, Source::Document(0));
    assert_eq!(
        &DIAN_FULL_XML[amount.span.start..amount.span.end],
        "184350.00"
    );

    let vendor = extraction.vendor.ok_or("expected vendor")?;
    assert_eq!(vendor.value, Vendor::new("Acme & Luz S.A.S. E.S.P.")?);
    assert_eq!(vendor.confidence.get(), 100);
    assert_eq!(
        &DIAN_FULL_XML[vendor.span.start..vendor.span.end],
        "Acme &amp; Luz S.A.S. E.S.P."
    );

    let issued = extraction.issued.ok_or("expected issued")?;
    assert_eq!(issued.value, date!(2026 - 09 - 01));
    assert_eq!(
        &DIAN_FULL_XML[issued.span.start..issued.span.end],
        "2026-09-01"
    );

    let due = extraction.due.ok_or("expected due")?;
    assert_eq!(due.value, date!(2026 - 09 - 25));
    assert_eq!(&DIAN_FULL_XML[due.span.start..due.span.end], "2026-09-25");

    let period = extraction.period.ok_or("expected period")?;
    assert_eq!(period.value.start(), date!(2026 - 08 - 01));
    assert_eq!(period.value.end(), date!(2026 - 08 - 31));
    assert_eq!(period.span.source, Source::Document(0));
    let period_text = &DIAN_FULL_XML[period.span.start..period.span.end];
    assert!(period_text.contains("2026-08-01"));
    assert!(period_text.contains("2026-08-31"));

    assert!(extraction.notes.is_empty());
    Ok(())
}

/// AC5 (#37), keeping #27 AC2 fields: `dian_no_period.zip` yields USD 99.50, the legal-entity vendor, issued/due, no
/// period; `is_complete(50)` is true (issued stands in for the period, #37 AC5).
#[tokio::test]
async fn ac5_dian_no_period_zip_is_complete_with_issued() -> Result<()> {
    let envelope = zip_envelope(DIAN_NO_PERIOD_ZIP)?;
    let extraction = XmlInvoiceExtractor.extract(&envelope).await?;
    assert!(extraction.is_complete(50));
    assert_eq!(extraction.period, None);

    let amount = extraction.amount.ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(9_950, Currency::new("USD")?));

    let vendor = extraction.vendor.ok_or("expected vendor")?;
    assert_eq!(vendor.value, Vendor::new("Gas Natural Ejemplo S.A.")?);

    let issued = extraction.issued.ok_or("expected issued")?;
    assert_eq!(issued.value, date!(2026 - 09 - 10));

    let due = extraction.due.ok_or("expected due")?;
    assert_eq!(due.value, date!(2026 - 09 - 30));
    Ok(())
}

/// AC3: no zip document, or the only zip's root is `CreditNote` (`not_invoice.zip`), yields
/// `Extraction::default()`.
#[tokio::test]
async fn ac3_no_invoice_root_is_default() -> Result<()> {
    let no_zip = Envelope {
        subject: None,
        sender: "facturacion@acme-luz.example".to_string(),
        date: None,
        text: Some("no attachment here".to_string()),
        html: None,
        documents: Vec::new(),
    };
    assert_eq!(
        XmlInvoiceExtractor.extract(&no_zip).await?,
        Extraction::default()
    );

    let only_credit_note = zip_envelope(NOT_INVOICE_ZIP)?;
    assert_eq!(
        XmlInvoiceExtractor.extract(&only_credit_note).await?,
        Extraction::default()
    );
    Ok(())
}

/// AC4: a `text/plain` document 0 plus `dian_full.zip` minus its last 10 bytes at document 1
/// is `Err(Error::Zip { document: 1, .. })`.
#[tokio::test]
async fn ac4_corrupt_zip_attachment_is_zip_error() -> Result<()> {
    let truncated = &DIAN_FULL_ZIP[..DIAN_FULL_ZIP.len() - 10];
    let envelope = Envelope {
        subject: None,
        sender: "facturacion@acme-luz.example".to_string(),
        date: None,
        text: None,
        html: None,
        documents: vec![
            Document {
                mime: MimeType::new("text/plain")?,
                filename: None,
                bytes: b"not a zip".to_vec(),
            },
            Document {
                mime: MimeType::new("application/zip")?,
                filename: None,
                bytes: truncated.to_vec(),
            },
        ],
    };

    let result = XmlInvoiceExtractor.extract(&envelope).await;
    assert!(matches!(
        result,
        Err(Error::Zip {
            document: 1,
            source: hauz_core::zip::Error::Malformed { .. }
        })
    ));
    Ok(())
}

/// Builds a one-entry stored zip from `DIAN_FULL_XML` with `old` replaced by `new` (asserting
/// the replacement actually matched something, so a typo in the test fails loudly).
fn mutated_dian_zip(old: &str, new: &str) -> Result<Vec<u8>> {
    assert!(
        DIAN_FULL_XML.contains(old),
        "fixture no longer contains {old:?}"
    );
    let mutated = DIAN_FULL_XML.replace(old, new);
    Ok(common::build_stored_zip(&[(
        "ad09012345678900001.xml".to_string(),
        mutated.into_bytes(),
    )]))
}

/// AC5: a grammar/constructor failure on one field omits only that field, keeping the rest.
#[tokio::test]
async fn ac5_malformed_field_values_omit_only_that_field() -> Result<()> {
    // Ungrammatical amount (thousands separator): amount omitted, rest kept.
    let bad_amount = mutated_dian_zip(
        r#"<cbc:PayableAmount currencyID="COP">184350.00</cbc:PayableAmount>"#,
        r#"<cbc:PayableAmount currencyID="COP">1,234.56</cbc:PayableAmount>"#,
    )?;
    let extraction = XmlInvoiceExtractor
        .extract(&zip_envelope(&bad_amount)?)
        .await?;
    assert_eq!(extraction.amount, None);
    assert!(extraction.vendor.is_some());
    assert!(extraction.issued.is_some());
    assert!(extraction.due.is_some());
    assert!(extraction.period.is_some());

    // Lowercase currencyID fails `Currency::new`: amount omitted, rest kept.
    let bad_currency = mutated_dian_zip(
        r#"<cbc:PayableAmount currencyID="COP">184350.00</cbc:PayableAmount>"#,
        r#"<cbc:PayableAmount currencyID="cop">184350.00</cbc:PayableAmount>"#,
    )?;
    let extraction = XmlInvoiceExtractor
        .extract(&zip_envelope(&bad_currency)?)
        .await?;
    assert_eq!(extraction.amount, None);
    assert!(extraction.vendor.is_some());
    assert!(extraction.period.is_some());

    // DMY IssueDate fails the leading-YYYY-MM-DD grammar: issued omitted, rest kept.
    let bad_issued = mutated_dian_zip(
        "<cbc:IssueDate>2026-09-01</cbc:IssueDate>",
        "<cbc:IssueDate>01/09/2026</cbc:IssueDate>",
    )?;
    let extraction = XmlInvoiceExtractor
        .extract(&zip_envelope(&bad_issued)?)
        .await?;
    assert_eq!(extraction.issued, None);
    assert!(extraction.amount.is_some());
    assert!(extraction.vendor.is_some());
    assert!(extraction.due.is_some());
    assert!(extraction.period.is_some());

    // EndDate before StartDate fails `BillingPeriod::new`: period omitted, rest kept.
    let inverted_period = mutated_dian_zip(
        "<cac:InvoicePeriod><cbc:StartDate>2026-08-01</cbc:StartDate><cbc:EndDate>2026-08-31</cbc:EndDate></cac:InvoicePeriod>",
        "<cac:InvoicePeriod><cbc:StartDate>2026-08-31</cbc:StartDate><cbc:EndDate>2026-08-01</cbc:EndDate></cac:InvoicePeriod>",
    )?;
    let extraction = XmlInvoiceExtractor
        .extract(&zip_envelope(&inverted_period)?)
        .await?;
    assert_eq!(extraction.period, None);
    assert!(extraction.amount.is_some());
    assert!(extraction.vendor.is_some());
    assert!(extraction.issued.is_some());
    assert!(extraction.due.is_some());
    Ok(())
}

/// Wraps `invoice` in an `AttachedDocument` CDATA, followed by `outer`, as a one-entry zip
/// envelope.
fn wrapped_invoice_envelope(invoice: &str, outer: &str) -> Result<Envelope> {
    let xml = format!(
        "<AttachedDocument xmlns:cbc=\"c\"><cac:Attachment><![CDATA[{invoice}]]></cac:Attachment>{outer}</AttachedDocument>"
    );
    let bytes = common::build_stored_zip(&[("a.xml".to_string(), xml.into_bytes())]);
    zip_envelope(&bytes)
}

/// AC2: a customer's `PartyTaxScheme/RegistrationName` after a supplier that only has
/// `PartyLegalEntity` never becomes the vendor.
#[tokio::test]
async fn ac2_customer_registration_name_is_not_vendor() -> Result<()> {
    let invoice = "<Invoice><cac:AccountingSupplierParty><cac:Party><cac:PartyLegalEntity>\
        <cbc:RegistrationName>Supplier SA</cbc:RegistrationName></cac:PartyLegalEntity></cac:Party>\
        </cac:AccountingSupplierParty><cac:AccountingCustomerParty><cac:Party><cac:PartyTaxScheme>\
        <cbc:RegistrationName>Customer SA</cbc:RegistrationName></cac:PartyTaxScheme></cac:Party>\
        </cac:AccountingCustomerParty></Invoice>";
    let envelope = wrapped_invoice_envelope(invoice, "")?;

    let extraction = XmlInvoiceExtractor.extract(&envelope).await?;
    let vendor = extraction.vendor.ok_or("expected vendor")?;
    assert_eq!(vendor.value, Vendor::new("Supplier SA")?);
    Ok(())
}

/// AC2: a `DueDate` outside the `Invoice` (in the wrapping `AttachedDocument`) is ignored.
#[tokio::test]
async fn ac2_outer_due_date_is_ignored() -> Result<()> {
    let invoice = "<Invoice><cbc:IssueDate>2026-01-02</cbc:IssueDate></Invoice>";
    let envelope = wrapped_invoice_envelope(invoice, "<cbc:DueDate>2026-02-03</cbc:DueDate>")?;

    let extraction = XmlInvoiceExtractor.extract(&envelope).await?;
    assert!(extraction.due.is_none());
    Ok(())
}

/// Quick #35: a DIAN zip sent as `application/octet-stream` with a `.zip` filename (any
/// case) is read like `application/zip`; without that filename it stays ignored.
#[tokio::test]
async fn octet_stream_zip_with_zip_filename_is_read() -> Result<()> {
    let mut envelope = zip_envelope(DIAN_FULL_ZIP)?;
    let document = envelope
        .documents
        .first_mut()
        .ok_or("expected a document")?;
    document.mime = MimeType::new("application/octet-stream")?;
    document.filename = Some("z0900219834000260159F826.ZIP".to_string());
    let extraction = XmlInvoiceExtractor.extract(&envelope).await?;
    let amount = extraction.amount.ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(18_435_000, Currency::new("COP")?));

    let document = envelope
        .documents
        .first_mut()
        .ok_or("expected a document")?;
    document.filename = Some("z0900219834000260159f826.bin".to_string());
    assert_eq!(
        XmlInvoiceExtractor.extract(&envelope).await?,
        Extraction::default()
    );
    Ok(())
}

// -----------------------------------------------------------------------------------------
// XmlInvoiceExtractor (#36): an unsupported zip degrades to a note, a malformed one still fails.
// -----------------------------------------------------------------------------------------

/// A `Document` of `bytes` typed `application/zip`.
fn zip_document(bytes: &[u8]) -> Result<Document> {
    Ok(Document {
        mime: MimeType::new("application/zip")?,
        filename: None,
        bytes: bytes.to_vec(),
    })
}

/// An otherwise-empty envelope holding exactly `documents`.
fn documents_envelope(documents: Vec<Document>) -> Envelope {
    Envelope {
        subject: None,
        sender: "facturacion@acme-luz.example".to_string(),
        date: None,
        text: None,
        html: None,
        documents,
    }
}

/// A stored zip with data-descriptor flag bit 3 set: `zip::read` answers `Unsupported`.
fn descriptor_zip() -> Vec<u8> {
    let archive = common::build_stored_zip(&[("a.txt".to_string(), b"hello".to_vec())]);
    common::with_data_descriptor(&archive)
}

/// AC1 (#36): document 0 is `bzip2.zip` as `application/zip`: `Ok`, no fields, notes exactly
/// `{UnreadableArchive { document: 0 }}`.
#[tokio::test]
async fn ac1_bzip2_zip_degrades_to_unreadable_archive_note() -> Result<()> {
    let envelope = documents_envelope(vec![zip_document(BZIP2_ZIP)?]);
    let extraction = XmlInvoiceExtractor.extract(&envelope).await?;

    let expected = Extraction {
        notes: BTreeSet::from([Note::UnreadableArchive { document: 0 }]),
        ..Extraction::default()
    };
    assert_eq!(extraction, expected);
    Ok(())
}

/// AC2 (#36): document 0 is a test-built descriptor zip and document 1 is `dian_full.zip`:
/// the #27 AC1 fields plus notes `{UnreadableArchive { document: 0 }}`.
#[tokio::test]
async fn ac2_descriptor_zip_beside_dian_zip_keeps_dian_fields() -> Result<()> {
    let envelope = documents_envelope(vec![
        zip_document(&descriptor_zip())?,
        zip_document(DIAN_FULL_ZIP)?,
    ]);
    let extraction = XmlInvoiceExtractor.extract(&envelope).await?;

    let amount = extraction.amount.ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(18_435_000, Currency::new("COP")?));
    assert_eq!(amount.span.source, Source::Document(1));
    let vendor = extraction.vendor.ok_or("expected vendor")?;
    assert_eq!(vendor.value, Vendor::new("Acme & Luz S.A.S. E.S.P.")?);
    assert_eq!(
        extraction.issued.ok_or("expected issued")?.value,
        date!(2026 - 09 - 01)
    );
    assert_eq!(
        extraction.due.ok_or("expected due")?.value,
        date!(2026 - 09 - 25)
    );
    let period = extraction.period.ok_or("expected period")?;
    assert_eq!(period.value.start(), date!(2026 - 08 - 01));
    assert_eq!(period.value.end(), date!(2026 - 08 - 31));
    assert_eq!(
        extraction.notes,
        BTreeSet::from([Note::UnreadableArchive { document: 0 }])
    );
    Ok(())
}

/// AC3 (#36): an unsupported zip at document 0 does not mask a malformed one at document 1:
/// `Err(Error::Zip { document: 1, source: Malformed { .. } })`.
#[tokio::test]
async fn ac3_malformed_zip_after_unsupported_is_still_zip_error() -> Result<()> {
    let truncated = &DIAN_FULL_ZIP[..DIAN_FULL_ZIP.len() - 10];
    let envelope = documents_envelope(vec![zip_document(BZIP2_ZIP)?, zip_document(truncated)?]);

    let result = XmlInvoiceExtractor.extract(&envelope).await;
    assert!(matches!(
        result,
        Err(Error::Zip {
            document: 1,
            source: hauz_core::zip::Error::Malformed { .. }
        })
    ));
    Ok(())
}
