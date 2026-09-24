//! [property] tests for the `bill` module's public API. One file per level per module.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

use hauz_core::bill::{Bill, BillDraft, BillId, BillingPeriod, Currency, Money, Status, Vendor};
use proptest::prelude::*;

/// A currency code: exactly 3 ASCII uppercase letters, built through [`Currency::new`].
fn arb_currency() -> impl Strategy<Value = Currency> {
    "[A-Z]{3}".prop_filter_map("valid currency", |raw| Currency::new(&raw).ok())
}

/// A trimmed, non-empty vendor name, built through [`Vendor::new`].
fn arb_vendor() -> impl Strategy<Value = Vendor> {
    "[A-Za-z]{1,20}".prop_filter_map("valid vendor", |raw| Vendor::new(&raw).ok())
}

/// A validated bill id, built through [`BillId::new`].
fn arb_bill_id() -> impl Strategy<Value = BillId> {
    "[!-~]{1,32}".prop_filter_map("valid bill id", |raw| BillId::new(&raw).ok())
}

/// An amount in an arbitrary currency; `Money::new` never fails.
fn arb_money() -> impl Strategy<Value = Money> {
    (any::<i64>(), arb_currency())
        .prop_map(|(minor_units, currency)| Money::new(minor_units, currency))
}

/// A calendar date within a bounded, realistic range.
fn arb_date() -> impl Strategy<Value = time::Date> {
    (2000i32..2100, 1u16..=365u16).prop_filter_map("valid date", |(year, ordinal)| {
        time::Date::from_ordinal_date(year, ordinal).ok()
    })
}

/// A validated billing period, built through [`BillingPeriod::new`].
fn arb_billing_period() -> impl Strategy<Value = BillingPeriod> {
    (arb_date(), 0i64..=60).prop_filter_map("valid period", |(start, offset)| {
        let end = start.checked_add(time::Duration::days(offset))?;
        BillingPeriod::new(start, end).ok()
    })
}

fn arb_status() -> impl Strategy<Value = Status> {
    prop_oneof![Just(Status::Extracted), Just(Status::NeedsReview)]
}

/// A valid `Bill`: `Extracted` bills always carry vendor, amount, and period; `NeedsReview`
/// bills carry an arbitrary subset (including none), built through `Bill::try_from`.
fn arb_bill() -> impl Strategy<Value = Bill> {
    (
        arb_bill_id(),
        arb_status(),
        arb_vendor(),
        arb_money(),
        arb_billing_period(),
        proptest::option::of(arb_date()),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_filter_map(
            "valid bill draft",
            |(id, status, vendor, amount, period, due, keep_vendor, keep_amount, keep_period)| {
                let extracted = status == Status::Extracted;
                let draft = BillDraft {
                    id,
                    vendor: if extracted || keep_vendor {
                        Some(vendor)
                    } else {
                        None
                    },
                    amount: if extracted || keep_amount {
                        Some(amount)
                    } else {
                        None
                    },
                    period: if extracted || keep_period {
                        Some(period)
                    } else {
                        None
                    },
                    due,
                    status,
                };
                Bill::try_from(draft).ok()
            },
        )
}

proptest! {
    #[test]
    fn ac11_checked_add_then_checked_sub_round_trips(
        minor_a in (i64::MIN / 2)..=(i64::MAX / 2),
        minor_b in (i64::MIN / 2)..=(i64::MAX / 2),
        currency in arb_currency(),
    ) {
        // Operands are drawn from half the i64 range, so neither step can overflow.
        let a = Money::new(minor_a, currency.clone());
        let b = Money::new(minor_b, currency);
        let diff = a.checked_add(&b)?.checked_sub(&b)?;
        prop_assert_eq!(diff, a);
    }

    #[test]
    fn ac12_json_round_trips(bill in arb_bill()) {
        let json = serde_json::to_string(&bill)?;
        let parsed: Bill = serde_json::from_str(&json)?;
        prop_assert_eq!(parsed, bill);
    }
}
