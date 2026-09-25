//! [property] tests for the `extract` module's public API. One file per level per module.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

use hauz_core::bill::{BillingPeriod, Currency, Money, Vendor};
use hauz_core::extract::{Confidence, Extraction, Field, Source, Span, merge};
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

fn arb_extraction() -> impl Strategy<Value = Extraction> {
    (
        proptest::option::of(arb_field(arb_money())),
        proptest::option::of(arb_field(arb_date())),
        proptest::option::of(arb_field(arb_date())),
        proptest::option::of(arb_field(arb_billing_period())),
        proptest::option::of(arb_field(arb_vendor())),
    )
        .prop_map(|(amount, issued, due, period, vendor)| Extraction {
            amount,
            issued,
            due,
            period,
            vendor,
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
