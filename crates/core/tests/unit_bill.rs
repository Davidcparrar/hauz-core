//! [unit] tests for the `bill` module's public API. One file per level per module.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

use hauz_core::bill::{Currency, Error};

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
