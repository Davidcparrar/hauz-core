//! Owns the pipeline from a raw RFC 5322 message to a persisted [`Bill`]: hash the raw bytes
//! for idempotency, parse the envelope, extract candidate fields, decide [`Status`], and
//! insert. The same message twice yields one row and the same id; what the extractor cannot
//! read is stored as [`Status::NeedsReview`] with every present field kept, never dropped and
//! never guessed.

use sha2::{Digest, Sha256};

use crate::bill::{Bill, BillDraft, BillId, Status};
use crate::email::Envelope;
use crate::extract::Extractor;
use crate::store::{BillStore, InsertOutcome, RawHash};
use crate::{bill, email, extract, store};

/// The minimum amount-field confidence for a bill to be stored as [`Status::Extracted`].
/// Below this floor the amount is still kept, but the bill is [`Status::NeedsReview`].
pub const EXTRACTED_MIN_CONFIDENCE: u8 = 50;

/// Errors this module can return. Library code never panics; it returns one of these.
/// `Debug` only (not `PartialEq`/`Eq`/`Clone`): [`store::Error`] wraps `sqlx::Error`, which
/// implements none of those.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The raw message could not be parsed into an [`Envelope`].
    #[error(transparent)]
    Email(#[from] email::Error),
    /// The extractor could not build a valid [`extract::Extraction`].
    #[error(transparent)]
    Extract(#[from] extract::Error),
    /// The store could not read or write a row.
    #[error(transparent)]
    Store(#[from] store::Error),
    /// The drafted fields did not form a valid [`Bill`].
    #[error(transparent)]
    Bill(#[from] bill::Error),
}

/// The result of [`ingest`]ing one raw message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A new row was stored under this id.
    Created(BillId),
    /// A row already existed for this message's hash (same bytes ingested before, or a
    /// concurrent writer won the race); this is that row's id.
    Duplicate(BillId),
}

/// The result of [`reextract`]ing one raw message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reextracted {
    /// The message was never stored; a new row was inserted under this id.
    Created(BillId),
    /// The stored row was overwritten with the new extraction.
    Updated(BillId),
    /// The new extraction equals the stored row; nothing was written.
    Unchanged(BillId),
}

/// The SHA-256 hash of `raw`, `store`'s idempotency key.
#[must_use]
pub fn raw_hash(raw: &[u8]) -> RawHash {
    let digest = Sha256::digest(raw);
    RawHash::new(digest.into())
}

/// Hex-encodes `bytes` as lowercase ASCII, two characters per byte.
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Turns a raw RFC 5322 message into a persisted [`Bill`].
///
/// In order: hashes `raw`; a row already stored under that hash short-circuits to
/// `Ok(Duplicate(existing id))` without ever calling `ex`. Otherwise parses the envelope,
/// extracts candidate fields, and drafts a bill whose id is the lowercase hex of the hash
/// (deterministic, so the store's `DuplicateId` is unreachable on this path): `Extracted`
/// when the amount field is present at confidence `>= EXTRACTED_MIN_CONFIDENCE` and vendor
/// is present and so is a period or an issue date, else `NeedsReview` keeping every present field. Inserts the
/// bill; a concurrent writer that won the race surfaces as `Duplicate`, not an error.
///
/// # Errors
/// Returns [`Error::Email`] when `raw` does not parse, [`Error::Extract`] when `ex` fails,
/// [`Error::Bill`] when the drafted fields do not form a valid `Bill`, or [`Error::Store`]
/// when the store fails to read or write.
pub async fn ingest(raw: &[u8], ex: &dyn Extractor, st: &dyn BillStore) -> Result<Outcome, Error> {
    let hash = raw_hash(raw);
    if let Some(existing) = st.find_by_hash(&hash).await? {
        return Ok(Outcome::Duplicate(existing.id().clone()));
    }

    let bill = build_bill(raw, &hash, ex).await?;

    match st.insert(&hash, &bill).await? {
        InsertOutcome::Inserted(id) => Ok(Outcome::Created(id)),
        InsertOutcome::Duplicate(id) => Ok(Outcome::Duplicate(id)),
    }
}

/// Parses `raw`, extracts with `ex`, and drafts the bill whose id is the lowercase hex of
/// `hash`: `Extracted` when the amount field is present at confidence
/// `>= EXTRACTED_MIN_CONFIDENCE` and vendor is present and so is a period or an issue date,
/// else `NeedsReview` keeping every present field.
async fn build_bill(raw: &[u8], hash: &RawHash, ex: &dyn Extractor) -> Result<Bill, Error> {
    let envelope = Envelope::parse(raw)?;
    let extraction = ex.extract(&envelope).await?;

    let id = BillId::new(&to_hex(hash.as_bytes()))?;
    let status = if extraction.is_complete(EXTRACTED_MIN_CONFIDENCE) {
        Status::Extracted
    } else {
        Status::NeedsReview
    };

    let draft = BillDraft {
        id,
        vendor: extraction.vendor.map(|field| field.value),
        amount: extraction.amount.map(|field| field.value),
        period: extraction.period.map(|field| field.value),
        issued: extraction.issued.map(|field| field.value),
        due: extraction.due.map(|field| field.value),
        status,
    };
    Ok(Bill::try_from(draft)?)
}

/// Re-runs the pipeline on `raw` and overwrites the row stored for its hash.
///
/// In order: hashes `raw`, parses, extracts and builds the bill exactly as [`ingest`] does
/// (every failure precedes any write). No row under the hash: inserts and returns `Created`
/// (an `insert` `Duplicate` from a concurrent writer falls through to the stored path). A
/// stored row equal to the new bill returns `Unchanged` without writing; otherwise the row is
/// replaced exactly (fields becoming `None` and a downgrade to `NeedsReview` included) and
/// `Updated` is returned.
///
/// # Errors
/// As [`ingest`]: [`Error::Email`], [`Error::Extract`], [`Error::Bill`], or [`Error::Store`].
pub async fn reextract(
    raw: &[u8],
    ex: &dyn Extractor,
    st: &dyn BillStore,
) -> Result<Reextracted, Error> {
    let hash = raw_hash(raw);
    let bill = build_bill(raw, &hash, ex).await?;

    let stored = match st.find_by_hash(&hash).await? {
        Some(stored) => stored,
        None => match st.insert(&hash, &bill).await? {
            InsertOutcome::Inserted(id) => return Ok(Reextracted::Created(id)),
            InsertOutcome::Duplicate(_) => st
                .find_by_hash(&hash)
                .await?
                .ok_or_else(|| store::Error::NotFound(bill.id().clone()))?,
        },
    };
    if stored == bill {
        return Ok(Reextracted::Unchanged(bill.id().clone()));
    }
    st.replace(&bill).await?;
    Ok(Reextracted::Updated(bill.id().clone()))
}
