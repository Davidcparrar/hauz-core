//! [unit] tests for the `bill` module's public API. One file per level per module.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

use hauz_core::bill::{BillId, Currency, Error, Money, Vendor};

#[test]
fn ac1_accepts_three_ascii_uppercase_letters() -> Result<(), Error> {
    let currency = Currency::new("USD")?;
    assert_eq!(currency.as_str(), "USD");
    Ok(())
}

#[test]
fn ac1_rejects_lowercase_two_four_digits_whitespace_and_empty() {
    for raw in ["usd", "US", "USDD", "US1", "   ", ""] {
        assert_eq!(Currency::new(raw), Err(Error::InvalidCurrency), "input: {raw:?}");
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
        assert_eq!(BillId::new(raw), Err(Error::InvalidBillId), "input: {raw:?}");
    }
}
