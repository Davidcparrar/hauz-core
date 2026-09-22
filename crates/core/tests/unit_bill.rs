//! [unit] tests for the `bill` module's public API. One file per level per module.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

use hauz_core::bill::{
    Bill, BillDraft, BillId, BillingPeriod, Currency, Error, Money, Status, Vendor,
};
use time::macros::date;

/// A `BillDraft` with every optional field populated, status `Extracted`.
fn complete_draft() -> Result<BillDraft, Error> {
    Ok(BillDraft {
        id: BillId::new("bill-1")?,
        vendor: Some(Vendor::new("Acme Power")?),
        amount: Some(Money::new(1000, Currency::new("USD")?)),
        period: Some(BillingPeriod::new(
            date!(2026 - 01 - 01),
            date!(2026 - 01 - 31),
        )?),
        due: Some(date!(2026 - 02 - 15)),
        status: Status::Extracted,
    })
}

#[test]
fn ac1_accepts_three_ascii_uppercase_letters() -> Result<(), Error> {
    let currency = Currency::new("USD")?;
    assert_eq!(currency.as_str(), "USD");
    Ok(())
}

#[test]
fn ac1_rejects_lowercase_two_four_digits_whitespace_and_empty() {
    for raw in ["usd", "US", "USDD", "US1", "   ", ""] {
        assert_eq!(
            Currency::new(raw),
            Err(Error::InvalidCurrency),
            "input: {raw:?}"
        );
    }
}

#[test]
fn ac2_accepts_any_i64_unchanged() -> Result<(), Error> {
    let usd = Currency::new("USD")?;
    for minor_units in [i64::MIN, -1, 0, 1, i64::MAX] {
        let money = Money::new(minor_units, usd.clone());
        assert_eq!(money.minor_units(), minor_units);
        assert_eq!(money.currency(), &usd);
    }
    Ok(())
}

#[test]
fn ac3_checked_add_and_sub_reject_currency_mismatch() -> Result<(), Error> {
    let usd = Money::new(100, Currency::new("USD")?);
    let eur = Money::new(100, Currency::new("EUR")?);
    assert_eq!(usd.checked_add(&eur), Err(Error::CurrencyMismatch));
    assert_eq!(usd.checked_sub(&eur), Err(Error::CurrencyMismatch));
    Ok(())
}

#[test]
fn ac3_checked_add_and_sub_reject_overflow() -> Result<(), Error> {
    let usd = Currency::new("USD")?;
    let max = Money::new(i64::MAX, usd.clone());
    let one = Money::new(1, usd.clone());
    let min = Money::new(i64::MIN, usd.clone());
    assert_eq!(max.checked_add(&one), Err(Error::Overflow));
    assert_eq!(min.checked_sub(&one), Err(Error::Overflow));
    Ok(())
}

#[test]
fn ac3_checked_add_and_sub_carry_the_shared_currency() -> Result<(), Error> {
    let usd = Currency::new("USD")?;
    let a = Money::new(300, usd.clone());
    let b = Money::new(100, usd.clone());
    assert_eq!(a.checked_add(&b)?, Money::new(400, usd.clone()));
    assert_eq!(a.checked_sub(&b)?, Money::new(200, usd));
    Ok(())
}

#[test]
fn ac4_trims_surrounding_whitespace() -> Result<(), Error> {
    let vendor = Vendor::new("  Acme Power  ")?;
    assert_eq!(vendor.name(), "Acme Power");
    Ok(())
}

#[test]
fn ac4_rejects_a_name_empty_after_trimming() {
    assert_eq!(Vendor::new("   "), Err(Error::EmptyVendor));
    assert_eq!(Vendor::new(""), Err(Error::EmptyVendor));
}

#[test]
fn ac5_accepts_one_to_128_ascii_graphic_bytes() -> Result<(), Error> {
    let id = BillId::new("bill-42")?;
    assert_eq!(id.as_str(), "bill-42");
    let max = "a".repeat(128);
    assert_eq!(BillId::new(&max)?.as_str(), max);
    Ok(())
}

#[test]
fn ac5_rejects_empty_whitespace_non_ascii_or_over_128_bytes() {
    let too_long = "a".repeat(129);
    for raw in ["", "has space", "café", too_long.as_str()] {
        assert_eq!(
            BillId::new(raw),
            Err(Error::InvalidBillId),
            "input: {raw:?}"
        );
    }
}

#[test]
fn ac6_accepts_end_equal_to_start_as_one_day_period() -> Result<(), Error> {
    let day = date!(2026 - 01 - 15);
    let period = BillingPeriod::new(day, day)?;
    assert_eq!(period.start(), day);
    assert_eq!(period.end(), day);
    Ok(())
}

#[test]
fn ac6_rejects_end_before_start() {
    let start = date!(2026 - 01 - 15);
    let end = date!(2026 - 01 - 14);
    assert_eq!(BillingPeriod::new(start, end), Err(Error::InvertedPeriod));
}

#[test]
fn ac7_extracted_bill_missing_vendor_amount_or_period_is_incomplete() -> Result<(), Error> {
    let mut missing_vendor = complete_draft()?;
    missing_vendor.vendor = None;
    assert_eq!(Bill::try_from(missing_vendor), Err(Error::IncompleteBill));

    let mut missing_amount = complete_draft()?;
    missing_amount.amount = None;
    assert_eq!(Bill::try_from(missing_amount), Err(Error::IncompleteBill));

    let mut missing_period = complete_draft()?;
    missing_period.period = None;
    assert_eq!(Bill::try_from(missing_period), Err(Error::IncompleteBill));
    Ok(())
}

#[test]
fn ac7_extracted_bill_with_vendor_amount_and_period_is_accepted_and_due_may_be_none()
-> Result<(), Error> {
    let mut draft = complete_draft()?;
    draft.due = None;
    let bill = Bill::try_from(draft)?;
    assert_eq!(bill.due(), None);
    assert!(bill.vendor().is_some());
    assert!(bill.amount().is_some());
    assert!(bill.period().is_some());
    Ok(())
}

#[test]
fn ac8_needs_review_bill_accepts_any_subset_of_optional_fields() -> Result<(), Error> {
    let mut none_set = complete_draft()?;
    none_set.status = Status::NeedsReview;
    none_set.vendor = None;
    none_set.amount = None;
    none_set.period = None;
    none_set.due = None;
    let bill = Bill::try_from(none_set)?;
    assert_eq!(bill.status(), Status::NeedsReview);
    assert_eq!(bill.vendor(), None);

    let mut some_set = complete_draft()?;
    some_set.status = Status::NeedsReview;
    some_set.amount = None;
    let bill = Bill::try_from(some_set)?;
    assert!(bill.vendor().is_some());
    assert_eq!(bill.amount(), None);
    Ok(())
}

#[test]
fn ac9_deserialize_fails_on_invalid_currency() {
    let json = r#""usd""#;
    assert!(serde_json::from_str::<Currency>(json).is_err());
}

#[test]
fn ac9_deserialize_fails_on_inverted_period() {
    let json = r#"{"start":"2026-01-15","end":"2026-01-14"}"#;
    assert!(serde_json::from_str::<BillingPeriod>(json).is_err());
}

#[test]
fn ac9_deserialize_fails_on_extracted_bill_missing_amount() {
    let json = r#"{
        "id":"bill-1",
        "vendor":"Acme Power",
        "period":{"start":"2026-01-01","end":"2026-01-31"},
        "status":"extracted"
    }"#;
    assert!(serde_json::from_str::<Bill>(json).is_err());
}

#[test]
fn ac10_status_serializes_snake_case_and_reads_back() -> Result<(), serde_json::Error> {
    assert_eq!(serde_json::to_string(&Status::Extracted)?, r#""extracted""#);
    assert_eq!(
        serde_json::to_string(&Status::NeedsReview)?,
        r#""needs_review""#
    );
    assert_eq!(
        serde_json::from_str::<Status>(r#""extracted""#)?,
        Status::Extracted
    );
    assert_eq!(
        serde_json::from_str::<Status>(r#""needs_review""#)?,
        Status::NeedsReview
    );
    Ok(())
}
