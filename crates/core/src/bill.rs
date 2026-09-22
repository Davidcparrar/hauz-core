//! Domain vocabulary for a bill: money, vendor, billing period, and validated identifiers.
//! Every type is constructed only through a fallible constructor (or is trivially valid),
//! so an invalid value is unrepresentable elsewhere in the crate — including after
//! deserialization, which is routed through the same constructors.

use serde::{Deserialize, Serialize};

/// Errors this module can return. Library code never panics; it returns one of these.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Error {
    /// A currency code was not exactly 3 ASCII uppercase letters.
    #[error("currency must be exactly 3 ASCII uppercase letters")]
    InvalidCurrency,
    /// Two `Money` values in an arithmetic operation did not share a currency.
    #[error("currencies do not match")]
    CurrencyMismatch,
    /// A `Money` arithmetic operation overflowed `i64`.
    #[error("amount overflowed")]
    Overflow,
}

/// A validated ISO-4217-shaped currency code: exactly 3 ASCII uppercase letters.
/// Shape only — not checked against the ISO-4217 list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct Currency(String);

impl Currency {
    /// Parse, don't validate: accepts exactly 3 ASCII uppercase letters.
    ///
    /// # Errors
    /// Returns [`Error::InvalidCurrency`] otherwise.
    pub fn new(raw: &str) -> Result<Self, Error> {
        if raw.chars().count() == 3 && raw.chars().all(|c| c.is_ascii_uppercase()) {
            Ok(Self(raw.to_owned()))
        } else {
            Err(Error::InvalidCurrency)
        }
    }

    /// The validated currency code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Currency {
    type Error = Error;

    fn try_from(raw: String) -> Result<Self, Error> {
        Self::new(&raw)
    }
}

/// An amount of money as integer minor units (e.g. cents) plus its currency. Any `i64` is a
/// valid amount; only arithmetic between two `Money` values can fail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    minor_units: i64,
    currency: Currency,
}

impl Money {
    /// Any `i64` (negative, zero, or `i64::MAX`) is a valid amount of minor units.
    #[must_use]
    pub fn new(minor_units: i64, currency: Currency) -> Self {
        Self { minor_units, currency }
    }

    /// The amount, in minor units, unchanged from construction.
    #[must_use]
    pub fn minor_units(&self) -> i64 {
        self.minor_units
    }

    /// The currency this amount is denominated in.
    #[must_use]
    pub fn currency(&self) -> &Currency {
        &self.currency
    }

    /// Adds two amounts.
    ///
    /// # Errors
    /// Returns [`Error::CurrencyMismatch`] when `self` and `other` differ in currency, or
    /// [`Error::Overflow`] when the sum overflows `i64`.
    pub fn checked_add(&self, other: &Money) -> Result<Money, Error> {
        if self.currency != other.currency {
            return Err(Error::CurrencyMismatch);
        }
        self.minor_units
            .checked_add(other.minor_units)
            .map(|minor_units| Money::new(minor_units, self.currency.clone()))
            .ok_or(Error::Overflow)
    }

    /// Subtracts two amounts.
    ///
    /// # Errors
    /// Returns [`Error::CurrencyMismatch`] when `self` and `other` differ in currency, or
    /// [`Error::Overflow`] when the difference overflows `i64`.
    pub fn checked_sub(&self, other: &Money) -> Result<Money, Error> {
        if self.currency != other.currency {
            return Err(Error::CurrencyMismatch);
        }
        self.minor_units
            .checked_sub(other.minor_units)
            .map(|minor_units| Money::new(minor_units, self.currency.clone()))
            .ok_or(Error::Overflow)
    }
}
