//! [property] tests for the `ingest` module's public API: for every `Extraction` (the same
//! strategy `extract`'s property tests use), ingesting `bill_eml()` through a `Fixed`
//! extractor stores exactly the fields the extraction carried, with the status the
//! confidence/vendor/(period or issued) rule predicts. One file per level per module. Test fn names carry
//! the spec criterion they satisfy: `acN_<behavior>`.

mod common;

use std::collections::BTreeSet;

use hauz_core::bill::{BillingPeriod, Currency, Money, Status, Vendor};
use hauz_core::email::Envelope;
use hauz_core::extract::{
    Confidence, Error as ExtractError, Extraction, Extractor, Field, Note, Source, Span,
};
use hauz_core::ingest::{EXTRACTED_MIN_CONFIDENCE, Outcome, ingest};
use hauz_core::store::{BillStore, InMemoryStore};
use proptest::prelude::*;

/// A currency code sampled from a fixed list of active ISO 4217 codes.
fn arb_currency() -> impl Strategy<Value = Currency> {
    proptest::sample::select(vec!["USD", "EUR", "GBP", "COP", "JPY", "XOF", "COU", "CLF"])
        .prop_filter_map("valid currency", |raw| Currency::new(raw).ok())
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

/// Always returns a clone of the fixed extraction it was built with, ignoring the envelope.
struct Fixed(Extraction);

impl Extractor for Fixed {
    fn extract<'a>(
        &'a self,
        _envelope: &'a Envelope,
    ) -> hauz_core::BoxFuture<'a, std::result::Result<Extraction, ExtractError>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

proptest! {
    /// AC11: ingesting `bill_eml()` with `Fixed(e)` into a fresh `InMemoryStore` is always
    /// `Created`; the stored status is `Extracted` iff `e.amount.confidence >= 50 ∧ e.vendor ∧
    /// (e.period ∨ e.issued)` (AC13, #37), and the stored amount, issued, due, vendor, period equal `e`'s values either way.
    #[test]
    fn ac11_ingest_with_fixed_extraction_matches_status_rule(e in arb_extraction()) {
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let store = InMemoryStore::new();
        let raw = common::bill_eml();
        let expected = e.clone();
        let expected_status = if e
            .amount
            .as_ref()
            .is_some_and(|field| field.confidence.get() >= EXTRACTED_MIN_CONFIDENCE)
            && e.vendor.is_some()
            && (e.period.is_some() || e.issued.is_some())
        {
            Status::Extracted
        } else {
            Status::NeedsReview
        };

        let outcome = rt.block_on(ingest(&raw, &Fixed(e), &store)).expect("ingest");
        let Outcome::Created(id) = outcome else {
            panic!("expected Created, got {outcome:?}");
        };
        let bill = rt
            .block_on(store.get(&id))
            .expect("get")
            .expect("bill present");

        prop_assert_eq!(bill.status(), expected_status);
        prop_assert_eq!(bill.amount().cloned(), expected.amount.map(|f| f.value));
        prop_assert_eq!(bill.issued(), expected.issued.map(|f| f.value));
        prop_assert_eq!(bill.due(), expected.due.map(|f| f.value));
        prop_assert_eq!(bill.vendor().cloned(), expected.vendor.map(|f| f.value));
        prop_assert_eq!(bill.period().cloned(), expected.period.map(|f| f.value));
    }
}
