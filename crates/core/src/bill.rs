//! Domain vocabulary for a bill: money, vendor, billing period, and validated identifiers.
//! Every type is constructed only through a fallible constructor (or is trivially valid),
//! so an invalid value is unrepresentable elsewhere in the crate — including after
//! deserialization, which is routed through the same constructors.

use serde::{Deserialize, Serialize};

/// Errors this module can return. Library code never panics; it returns one of these.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Error {
    /// A currency code was not an active ISO 4217 code.
    #[error("currency must be an active ISO 4217 code")]
    InvalidCurrency,
    /// Two `Money` values in an arithmetic operation did not share a currency.
    #[error("currencies do not match")]
    CurrencyMismatch,
    /// A `Money` arithmetic operation overflowed `i64`.
    #[error("amount overflowed")]
    Overflow,
    /// A vendor name was empty after trimming.
    #[error("vendor name must not be empty")]
    EmptyVendor,
    /// A bill id was empty, contained whitespace or non-ASCII bytes, or exceeded 128 bytes.
    #[error("bill id must be 1-128 ASCII graphic bytes with no whitespace")]
    InvalidBillId,
    /// A billing period's end date preceded its start date.
    #[error("billing period end must not precede start")]
    InvertedPeriod,
    /// An extracted bill was missing its vendor, amount, or both billing period and issue date.
    #[error("extracted bill is missing vendor, amount, or period or issue date")]
    IncompleteBill,
}

/// The active ISO 4217 alphabetic codes (list one, as of 2026), sorted for binary search.
/// Includes funds codes and the real `X` currencies; excludes `XXX` (no currency), `XTS`
/// (testing) and withdrawn codes such as `HRK`, `ZWL`, `SLL`.
const ISO_4217: &[&str] = &[
    "AED", "AFN", "ALL", "AMD", "AOA", "ARS", "AUD", "AWG", "AZN", "BAM", "BBD", "BDT", "BHD",
    "BIF", "BMD", "BND", "BOB", "BOV", "BRL", "BSD", "BTN", "BWP", "BYN", "BZD", "CAD", "CDF",
    "CHE", "CHF", "CHW", "CLF", "CLP", "CNY", "COP", "COU", "CRC", "CUP", "CVE", "CZK", "DJF",
    "DKK", "DOP", "DZD", "EGP", "ERN", "ETB", "EUR", "FJD", "FKP", "GBP", "GEL", "GHS", "GIP",
    "GMD", "GNF", "GTQ", "GYD", "HKD", "HNL", "HTG", "HUF", "IDR", "ILS", "INR", "IQD", "IRR",
    "ISK", "JMD", "JOD", "JPY", "KES", "KGS", "KHR", "KMF", "KPW", "KRW", "KWD", "KYD", "KZT",
    "LAK", "LBP", "LKR", "LRD", "LSL", "LYD", "MAD", "MDL", "MGA", "MKD", "MMK", "MNT", "MOP",
    "MRU", "MUR", "MVR", "MWK", "MXN", "MXV", "MYR", "MZN", "NAD", "NGN", "NIO", "NOK", "NPR",
    "NZD", "OMR", "PAB", "PEN", "PGK", "PHP", "PKR", "PLN", "PYG", "QAR", "RON", "RSD", "RUB",
    "RWF", "SAR", "SBD", "SCR", "SDG", "SEK", "SGD", "SHP", "SLE", "SOS", "SRD", "SSP", "STN",
    "SVC", "SYP", "SZL", "THB", "TJS", "TMT", "TND", "TOP", "TRY", "TTD", "TWD", "TZS", "UAH",
    "UGX", "USD", "USN", "UYI", "UYU", "UYW", "UZS", "VED", "VES", "VND", "VUV", "WST", "XAF",
    "XAG", "XAU", "XCD", "XCG", "XDR", "XOF", "XPD", "XPF", "XPT", "XSU", "XUA", "YER", "ZAR",
    "ZMW", "ZWG",
];

/// A validated currency code: one of the active ISO 4217 alphabetic codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct Currency(String);

impl Currency {
    /// Parse, don't validate: accepts only an active ISO 4217 alphabetic code.
    ///
    /// # Errors
    /// Returns [`Error::InvalidCurrency`] when `raw` is not in the active list (wrong case
    /// or length, a made-up code, `XXX`, `XTS`, or a withdrawn code).
    pub fn new(raw: &str) -> Result<Self, Error> {
        if ISO_4217.binary_search(&raw).is_ok() {
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
        Self {
            minor_units,
            currency,
        }
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

/// A validated, trimmed, non-empty vendor name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct Vendor(String);

impl Vendor {
    /// Parse, don't validate: trims surrounding whitespace and rejects an empty result.
    ///
    /// # Errors
    /// Returns [`Error::EmptyVendor`] when the trimmed name is empty.
    pub fn new(raw: &str) -> Result<Self, Error> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(Error::EmptyVendor);
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The validated, trimmed vendor name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Vendor {
    type Error = Error;

    fn try_from(raw: String) -> Result<Self, Error> {
        Self::new(&raw)
    }
}

/// A validated bill identifier: 1-128 bytes of ASCII graphic characters, no whitespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct BillId(String);

impl BillId {
    /// Parse, don't validate.
    ///
    /// # Errors
    /// Returns [`Error::InvalidBillId`] when `raw` is empty, contains whitespace or
    /// non-ASCII bytes, or exceeds 128 bytes.
    pub fn new(raw: &str) -> Result<Self, Error> {
        let valid =
            !raw.is_empty() && raw.len() <= 128 && raw.chars().all(|c| c.is_ascii_graphic());
        if valid {
            Ok(Self(raw.to_owned()))
        } else {
            Err(Error::InvalidBillId)
        }
    }

    /// The validated id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for BillId {
    type Error = Error;

    fn try_from(raw: String) -> Result<Self, Error> {
        Self::new(&raw)
    }
}

/// A validated billing period: an inclusive `[start, end]` date range where `end >= start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "BillingPeriodRaw")]
pub struct BillingPeriod {
    start: time::Date,
    end: time::Date,
}

/// The unvalidated shape a [`BillingPeriod`] is deserialized from, before the ordering
/// invariant is checked.
#[derive(Deserialize)]
struct BillingPeriodRaw {
    start: time::Date,
    end: time::Date,
}

impl BillingPeriod {
    /// Parse, don't validate.
    ///
    /// # Errors
    /// Returns [`Error::InvertedPeriod`] when `end` precedes `start`.
    pub fn new(start: time::Date, end: time::Date) -> Result<Self, Error> {
        if end < start {
            return Err(Error::InvertedPeriod);
        }
        Ok(Self { start, end })
    }

    /// The first day of the period.
    #[must_use]
    pub fn start(&self) -> time::Date {
        self.start
    }

    /// The last day of the period.
    #[must_use]
    pub fn end(&self) -> time::Date {
        self.end
    }
}

impl TryFrom<BillingPeriodRaw> for BillingPeriod {
    type Error = Error;

    fn try_from(raw: BillingPeriodRaw) -> Result<Self, Error> {
        Self::new(raw.start, raw.end)
    }
}

/// Whether a bill's fields came from automated extraction or still need a human to fill
/// them in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// All required fields (vendor, amount, and period or issue date) are present.
    Extracted,
    /// One or more fields could not be extracted and need a human to fill them in.
    NeedsReview,
}

/// The unvalidated shape a [`Bill`] is built from. Every field is already its own validated
/// type; only the *combination* — which fields [`Status::Extracted`] requires — can still be
/// wrong, so that check lives in [`Bill`]'s `TryFrom` impl.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillDraft {
    /// The bill's identifier.
    pub id: BillId,
    /// The vendor, when known.
    pub vendor: Option<Vendor>,
    /// The amount, when known.
    pub amount: Option<Money>,
    /// The billing period, when known.
    pub period: Option<BillingPeriod>,
    /// The issue date, when known.
    pub issued: Option<time::Date>,
    /// The due date, when known.
    pub due: Option<time::Date>,
    /// Whether this draft's fields were fully extracted or still need review.
    pub status: Status,
}

/// A bill: complete when [`Status::Extracted`], possibly partial when
/// [`Status::NeedsReview`]. Construct only via `TryFrom<BillDraft>` — including through
/// deserialization, which routes through the same check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "BillDraft", into = "BillDraft")]
pub struct Bill {
    id: BillId,
    vendor: Option<Vendor>,
    amount: Option<Money>,
    period: Option<BillingPeriod>,
    issued: Option<time::Date>,
    due: Option<time::Date>,
    status: Status,
}

impl Bill {
    /// The bill's identifier.
    #[must_use]
    pub fn id(&self) -> &BillId {
        &self.id
    }

    /// The vendor, when known.
    #[must_use]
    pub fn vendor(&self) -> Option<&Vendor> {
        self.vendor.as_ref()
    }

    /// The amount, when known.
    #[must_use]
    pub fn amount(&self) -> Option<&Money> {
        self.amount.as_ref()
    }

    /// The billing period, when known.
    #[must_use]
    pub fn period(&self) -> Option<&BillingPeriod> {
        self.period.as_ref()
    }

    /// The issue date, when known.
    #[must_use]
    pub fn issued(&self) -> Option<time::Date> {
        self.issued
    }

    /// The due date, when known.
    #[must_use]
    pub fn due(&self) -> Option<time::Date> {
        self.due
    }

    /// Whether this bill's fields were fully extracted or still need review.
    #[must_use]
    pub fn status(&self) -> Status {
        self.status
    }
}

impl TryFrom<BillDraft> for Bill {
    type Error = Error;

    /// # Errors
    /// Returns [`Error::IncompleteBill`] when `draft.status` is [`Status::Extracted`] and
    /// `vendor` or `amount` is `None`, or both `period` and `issued` are.
    fn try_from(draft: BillDraft) -> Result<Self, Error> {
        if draft.status == Status::Extracted
            && (draft.vendor.is_none()
                || draft.amount.is_none()
                || (draft.period.is_none() && draft.issued.is_none()))
        {
            return Err(Error::IncompleteBill);
        }
        Ok(Self {
            id: draft.id,
            vendor: draft.vendor,
            amount: draft.amount,
            period: draft.period,
            issued: draft.issued,
            due: draft.due,
            status: draft.status,
        })
    }
}

impl From<Bill> for BillDraft {
    fn from(bill: Bill) -> Self {
        Self {
            id: bill.id,
            vendor: bill.vendor,
            amount: bill.amount,
            period: bill.period,
            issued: bill.issued,
            due: bill.due,
            status: bill.status,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ISO_4217;

    #[test]
    fn table_is_sorted_unique_and_well_shaped() {
        assert!(ISO_4217.windows(2).all(|w| w[0] < w[1]));
        assert!(
            ISO_4217
                .iter()
                .all(|c| c.len() == 3 && c.bytes().all(|b| b.is_ascii_uppercase()))
        );
    }
}
