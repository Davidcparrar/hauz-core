//! [property] tests for the `extract` module's public API. One file per level per module.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

mod common;

use std::collections::BTreeSet;

use hauz_core::bill::{BillingPeriod, Currency, Money, Vendor};
use hauz_core::email::{Document, Envelope, MimeType};
use hauz_core::extract::{
    Confidence, Error, Escalate, Extraction, Extractor, Field, Note, Source, Span,
    XmlInvoiceExtractor, merge,
};
use proptest::prelude::*;

fn arb_currency() -> impl Strategy<Value = Currency> {
    "[A-Z]{3}".prop_filter_map("valid currency", |raw| Currency::new(&raw).ok())
}

fn arb_vendor() -> impl Strategy<Value = Vendor> {
    "[A-Za-z]{1,20}".prop_filter_map("valid vendor", |raw| Vendor::new(&raw).ok())
}

fn arb_money() -> impl Strategy<Value = Money> {
    (any::<i64>(), arb_currency())
        .prop_map(|(minor_units, currency)| Money::new(minor_units, currency))
}

fn arb_date() -> impl Strategy<Value = time::Date> {
    (2000i32..2100, 1u16..=365u16).prop_filter_map("valid date", |(year, ordinal)| {
        time::Date::from_ordinal_date(year, ordinal).ok()
    })
}

fn arb_billing_period() -> impl Strategy<Value = BillingPeriod> {
    (arb_date(), 0i64..=60).prop_filter_map("valid period", |(start, offset)| {
        let end = start.checked_add(time::Duration::days(offset))?;
        BillingPeriod::new(start, end).ok()
    })
}

fn arb_confidence() -> impl Strategy<Value = Confidence> {
    (0u8..=100).prop_filter_map("valid confidence", |raw| Confidence::new(raw).ok())
}

fn arb_source() -> impl Strategy<Value = Source> {
    prop_oneof![
        Just(Source::Text),
        Just(Source::Html),
        (0usize..5).prop_map(Source::Document),
    ]
}

fn arb_span() -> impl Strategy<Value = Span> {
    (arb_source(), 0usize..100, 0usize..100).prop_map(|(source, a, b)| {
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        Span { source, start, end }
    })
}

fn arb_field<T>(value: impl Strategy<Value = T>) -> impl Strategy<Value = Field<T>>
where
    T: core::fmt::Debug,
{
    (value, arb_confidence(), arb_span()).prop_map(|(value, confidence, span)| Field {
        value,
        confidence,
        span,
    })
}

fn arb_note() -> impl Strategy<Value = Note> {
    (0usize..5).prop_map(|document| Note::NoTextLayer { document })
}

fn arb_notes() -> impl Strategy<Value = BTreeSet<Note>> {
    proptest::collection::btree_set(arb_note(), 0..3)
}

fn arb_extraction() -> impl Strategy<Value = Extraction> {
    (
        proptest::option::of(arb_field(arb_money())),
        proptest::option::of(arb_field(arb_date())),
        proptest::option::of(arb_field(arb_date())),
        proptest::option::of(arb_field(arb_billing_period())),
        proptest::option::of(arb_field(arb_vendor())),
        arb_notes(),
    )
        .prop_map(|(amount, issued, due, period, vendor, notes)| Extraction {
            amount,
            issued,
            due,
            period,
            vendor,
            notes,
        })
}

/// A batch of extractions alongside a shuffled permutation of the same batch.
fn arb_extractions_and_shuffle() -> impl Strategy<Value = (Vec<Extraction>, Vec<Extraction>)> {
    proptest::collection::vec(arb_extraction(), 0..6).prop_flat_map(|extractions| {
        let shuffled = Just(extractions.clone()).prop_shuffle();
        (Just(extractions), shuffled)
    })
}

proptest! {
    #[test]
    fn ac9_merge_is_order_independent_idempotent_and_duplicate_safe(
        (extractions, shuffled) in arb_extractions_and_shuffle(),
    ) {
        prop_assert_eq!(merge(extractions.clone()), merge(shuffled));

        let once = merge(extractions.clone());
        prop_assert_eq!(merge(vec![once.clone()]), once);

        let doubled: Vec<Extraction> = extractions
            .iter()
            .cloned()
            .chain(extractions.iter().cloned())
            .collect();
        prop_assert_eq!(merge(doubled), merge(extractions));
    }
}

/// Always returns a clone of the fixed extraction it was built with, ignoring the envelope.
struct Fixed(Extraction);

impl Extractor for Fixed {
    fn extract<'a>(
        &'a self,
        _envelope: &'a Envelope,
    ) -> hauz_core::BoxFuture<'a, std::result::Result<Extraction, Error>> {
        let extraction = self.0.clone();
        Box::pin(async move { Ok(extraction) })
    }
}

/// An otherwise-empty envelope; `Fixed` ignores it entirely.
fn envelope() -> Envelope {
    Envelope {
        subject: None,
        sender: "billing@example.com".to_string(),
        date: None,
        text: None,
        html: None,
        documents: Vec::new(),
    }
}

proptest! {
    /// AC6: `Escalate(Fixed(a), Fixed(b), t)` returns `a` when `a.is_complete(t)`, else
    /// `merge([a, b])`.
    #[test]
    fn ac6_escalate_returns_primary_when_complete_else_merge(
        a in arb_extraction(),
        b in arb_extraction(),
        t in any::<u8>(),
    ) {
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let envelope = envelope();
        let escalate = Escalate::new(Box::new(Fixed(a.clone())), Box::new(Fixed(b.clone())), t);

        let result = rt.block_on(escalate.extract(&envelope)).expect("escalate");
        let expected = if a.is_complete(t) { a } else { merge(vec![a, b]) };

        prop_assert_eq!(result, expected);
    }
}

/// Wraps `bytes` as the body of a single `application/zip` document in an otherwise-empty
/// envelope, built via `MimeType::new` so an invalid mime would be a test bug, not a
/// production path.
fn zip_envelope(bytes: Vec<u8>) -> Result<Envelope, hauz_core::email::Error> {
    Ok(Envelope {
        subject: None,
        sender: "billing@example.com".to_string(),
        date: None,
        text: None,
        html: None,
        documents: vec![Document {
            mime: MimeType::new("application/zip")?,
            filename: None,
            bytes,
        }],
    })
}

proptest! {
    /// AC10: for any string of 0..=512 chars stored as the `.xml` entry of a one-entry
    /// stored zip given as `application/zip`, `XmlInvoiceExtractor::extract` returns `Ok`
    /// without panicking.
    #[test]
    fn ac10_arbitrary_xml_entry_never_panics(
        text in proptest::collection::vec(any::<char>(), 0..=512)
            .prop_map(|chars| chars.into_iter().collect::<String>()),
    ) {
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let archive = common::build_stored_zip(&[("entry.xml".to_string(), text.into_bytes())]);
        let envelope = zip_envelope(archive).expect("valid mime type");

        let result = rt.block_on(XmlInvoiceExtractor.extract(&envelope));
        prop_assert!(result.is_ok());
    }
}

/// Envelope of `layout.len()` documents: `true` slots hold an unsupported zip (descriptor
/// zip or `bzip2.zip`, chosen by `variant`), `false` slots a `text/plain` document.
fn unsupported_zip_envelope(layout: &[(bool, bool)]) -> Result<Envelope, hauz_core::email::Error> {
    let descriptor = common::with_data_descriptor(&common::build_stored_zip(&[(
        "a.txt".to_string(),
        b"hello".to_vec(),
    )]));
    let mut documents = Vec::new();
    for &(is_zip, variant) in layout {
        documents.push(if is_zip {
            Document {
                mime: MimeType::new("application/zip")?,
                filename: None,
                bytes: if variant {
                    descriptor.clone()
                } else {
                    BZIP2_ZIP.to_vec()
                },
            }
        } else {
            Document {
                mime: MimeType::new("text/plain")?,
                filename: None,
                bytes: b"just text".to_vec(),
            }
        });
    }
    Ok(Envelope {
        subject: None,
        sender: "billing@example.com".to_string(),
        date: None,
        text: None,
        html: None,
        documents,
    })
}

const BZIP2_ZIP: &[u8] = include_bytes!("fixtures/zip/bzip2.zip");

proptest! {
    /// AC8 (#36): for any layout of unsupported zips among `text/plain` documents,
    /// `XmlInvoiceExtractor` returns `Ok` with no fields and exactly one
    /// `UnreadableArchive { document: i }` per zip index `i`.
    #[test]
    fn ac8_unsupported_zips_yield_one_note_each(
        layout in proptest::collection::vec(any::<(bool, bool)>(), 0..=8),
    ) {
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let envelope = unsupported_zip_envelope(&layout).expect("valid mime type");

        let extraction = rt
            .block_on(XmlInvoiceExtractor.extract(&envelope))
            .expect("unsupported zips never fail");

        let expected = Extraction {
            notes: layout
                .iter()
                .enumerate()
                .filter(|(_, (is_zip, _))| *is_zip)
                .map(|(document, _)| Note::UnreadableArchive { document })
                .collect(),
            ..Extraction::default()
        };
        prop_assert_eq!(extraction, expected);
    }
}
