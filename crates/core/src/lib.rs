//! Core library: all domain logic lives here and is tested through this crate's public API.
//! Binaries (`server`, `app`) only map I/O to calls into this crate.

/// Errors this crate can return. Library code never panics; it returns one of these.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// A name was empty after trimming.
    #[error("name must not be empty")]
    EmptyName,
}

/// A validated, non-empty greeting target. Construct only via [`Name::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name(String);

impl Name {
    /// Parse, don't validate: trims and rejects empty input once, at the boundary.
    ///
    /// # Errors
    /// Returns [`Error::EmptyName`] when `raw` is empty or whitespace.
    pub fn new(raw: &str) -> Result<Self, Error> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(Error::EmptyName);
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The validated name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Scaffold behavior so the template verifies green; replace with real domain code.
#[must_use]
pub fn greet(name: &Name) -> String {
    format!("hello, {}", name.as_str())
}
