//! [unit] tests for the `extract` module's public API. One file per level per module.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

use hauz_core::bill::{Currency, Money, Vendor};
use hauz_core::email::Envelope;
use hauz_core::extract::{
    Confidence, Error, Extraction, Extractor, Field, Source, Span, TextExtractor, merge,
};
use time::macros::date;

/// Boxed so any error type propagates with `?`; tests never unwrap or expect.
type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

const DE_TOTAL: &str = include_str!("fixtures/extract/de_total.txt");
const US_TOTAL: &str = include_str!("fixtures/extract/us_total.txt");
const FR_SPACE: &str = include_str!("fixtures/extract/fr_space.txt");
const ES_TABLE: &str = include_str!("fixtures/extract/es_table.html");
const NOISE: &str = include_str!("fixtures/extract/noise.txt");

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

#[test]
fn ac1_german_text_anchored_fields() -> Result<()> {
    let envelope = envelope("rechnung@stadtwerke.de", Some(DE_TOTAL), None);
    let extraction = TextExtractor.extract(&envelope)?;

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

#[test]
fn ac2_us_text_anchored_fields() -> Result<()> {
    let envelope = envelope("billing@example.com", Some(US_TOTAL), None);
    let extraction = TextExtractor.extract(&envelope)?;

    let amount = extraction.amount.ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(123_456, Currency::new("USD")?));

    let issued = extraction.issued.ok_or("expected issued")?;
    assert_eq!(issued.value, date!(2026 - 09 - 24));

    let due = extraction.due.ok_or("expected due")?;
    assert_eq!(due.value, date!(2026 - 10 - 15));
    Ok(())
}

#[test]
fn ac3_french_nbsp_grouping_no_issued() -> Result<()> {
    let envelope = envelope("factures@example.fr", Some(FR_SPACE), None);
    let extraction = TextExtractor.extract(&envelope)?;

    let amount = extraction.amount.ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(123_456, Currency::new("EUR")?));

    let due = extraction.due.ok_or("expected due")?;
    assert_eq!(due.value, date!(2026 - 10 - 15));

    assert_eq!(extraction.issued, None);
    Ok(())
}

#[test]
fn ac4_html_table_amount_and_due() -> Result<()> {
    let envelope = envelope("facturas@example.es", None, Some(ES_TABLE));
    let extraction = TextExtractor.extract(&envelope)?;

    let amount = extraction.amount.ok_or("expected amount")?;
    assert_eq!(amount.value, Money::new(12_100, Currency::new("EUR")?));
    assert_eq!(amount.span.source, Source::Html);

    let due = extraction.due.ok_or("expected due")?;
    assert_eq!(due.value, date!(2026 - 10 - 15));
    Ok(())
}

#[test]
fn ac5_noise_picks_largest_below_anchored_confidence() -> Result<()> {
    let anchored = envelope("billing@example.com", Some(DE_TOTAL), None);
    let anchored_amount = TextExtractor
        .extract(&anchored)?
        .amount
        .ok_or("expected anchored amount")?;

    let noisy = envelope("billing@example.com", Some(NOISE), None);
    let extraction = TextExtractor.extract(&noisy)?;
    let amount = extraction.amount.ok_or("expected amount")?;

    assert_eq!(amount.value, Money::new(50_000, Currency::new("USD")?));
    assert!(amount.confidence < anchored_amount.confidence);
    Ok(())
}

#[test]
fn ac6_empty_body_only_vendor_and_no_at_sign_no_vendor() -> Result<()> {
    let empty_body = envelope(
        "billing@example.com",
        Some("Thanks for your business."),
        None,
    );
    let extraction = TextExtractor.extract(&empty_body)?;

    assert_eq!(extraction.amount, None);
    assert_eq!(extraction.issued, None);
    assert_eq!(extraction.due, None);
    assert_eq!(extraction.period, None);
    assert!(extraction.vendor.is_some());

    let no_at = envelope("not-an-address", Some("Thanks for your business."), None);
    let extraction = TextExtractor.extract(&no_at)?;
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
