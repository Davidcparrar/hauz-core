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
