//! [unit] tests for the `ingest` module's public API: `InMemoryStore`, test-local `Fixed` and
//! `Failing` extractors. One file per level per module. Test fn names carry the spec criterion
//! they satisfy: `acN_<behavior>`.

mod common;

use std::collections::BTreeSet;

use common::Result;
use hauz_core::bill::{BillingPeriod, Currency, Money, Status, Vendor};
use hauz_core::email::Envelope;
use hauz_core::extract::{
    Confidence, Error as ExtractError, Extraction, Extractor, Field, Source, Span,
};
use hauz_core::ingest::{Error, Outcome, ingest, raw_hash};
use hauz_core::store::{BillStore, InMemoryStore};
use time::macros::date;

/// Always returns a clone of the fixed extraction it was built with, ignoring the envelope.
struct Fixed(Extraction);

impl Extractor for Fixed {
    fn extract<'a>(
        &'a self,
        _envelope: &'a Envelope,
    ) -> hauz_core::BoxFuture<'a, std::result::Result<Extraction, ExtractError>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

/// Always fails, as if an internal confidence computation went out of range.
struct Failing;

impl Extractor for Failing {
    fn extract<'a>(
        &'a self,
        _envelope: &'a Envelope,
    ) -> hauz_core::BoxFuture<'a, std::result::Result<Extraction, ExtractError>> {
        Box::pin(async move { Err(ExtractError::InvalidConfidence(101)) })
    }
}

fn field<T>(value: T, confidence: u8) -> Result<Field<T>> {
    Ok(Field {
        value,
        confidence: Confidence::new(confidence)?,
        span: Span {
            source: Source::Text,
            start: 0,
            end: 0,
        },
    })
}

/// An extraction with amount (at `amount_confidence`), due, and optionally vendor/period.
fn extraction(
    amount_confidence: u8,
    include_vendor: bool,
    include_period: bool,
) -> Result<Extraction> {
    let amount = Some(field(
        Money::new(123_456, Currency::new("EUR")?),
        amount_confidence,
    )?);
    let vendor = if include_vendor {
        Some(field(Vendor::new("acme-power.example")?, 90)?)
    } else {
        None
    };
    let period = if include_period {
        Some(field(
            BillingPeriod::new(date!(2026 - 01 - 01), date!(2026 - 01 - 31))?,
            90,
        )?)
    } else {
        None
    };
    let due = Some(field(date!(2026 - 02 - 15), 90)?);
    Ok(Extraction {
        amount,
        issued: None,
        due,
        period,
        vendor,
        notes: BTreeSet::new(),
    })
}

/// AC5: `Fixed` yields amount at 90, vendor and period ⇒ stored `Extracted` with amount, due,
/// vendor, period equal to the extraction's values.
#[tokio::test]
async fn ac5_high_confidence_full_fields_is_extracted() -> Result<()> {
    let store = InMemoryStore::new();
    let ext = extraction(90, true, true)?;
    let expected = ext.clone();
    let raw = common::bill_eml();

    let outcome = ingest(&raw, &Fixed(ext), &store).await?;
    let Outcome::Created(id) = outcome else {
        return Err(format!("expected Created, got {outcome:?}").into());
    };
    let bill = store.get(&id).await?.ok_or("missing bill")?;

    assert_eq!(bill.status(), Status::Extracted);
    assert_eq!(bill.amount().cloned(), expected.amount.map(|f| f.value));
    assert_eq!(bill.due(), expected.due.map(|f| f.value));
    assert_eq!(bill.vendor().cloned(), expected.vendor.map(|f| f.value));
    assert_eq!(bill.period().cloned(), expected.period.map(|f| f.value));
    Ok(())
}

/// AC6: `Fixed` yields amount at 49, vendor and period ⇒ stored `NeedsReview`, amount still
/// present.
#[tokio::test]
async fn ac6_low_confidence_amount_is_needs_review_but_kept() -> Result<()> {
    let store = InMemoryStore::new();
    let ext = extraction(49, true, true)?;
    let expected_amount = ext.amount.clone().map(|f| f.value);
    let raw = common::bill_eml();

    let outcome = ingest(&raw, &Fixed(ext), &store).await?;
    let Outcome::Created(id) = outcome else {
        return Err(format!("expected Created, got {outcome:?}").into());
    };
    let bill = store.get(&id).await?.ok_or("missing bill")?;

    assert_eq!(bill.status(), Status::NeedsReview);
    assert_eq!(bill.amount().cloned(), expected_amount);
    Ok(())
}

/// AC7: `Fixed` yields amount at 90 and vendor but no period ⇒ stored `NeedsReview`.
#[tokio::test]
async fn ac7_missing_period_is_needs_review() -> Result<()> {
    let store = InMemoryStore::new();
    let ext = extraction(90, true, false)?;
    let raw = common::bill_eml();

    let outcome = ingest(&raw, &Fixed(ext), &store).await?;
    let Outcome::Created(id) = outcome else {
        return Err(format!("expected Created, got {outcome:?}").into());
    };
    let bill = store.get(&id).await?.ok_or("missing bill")?;

    assert_eq!(bill.status(), Status::NeedsReview);
    Ok(())
}

/// AC8: `Failing` returns `Err(InvalidConfidence(101))` ⇒ `Err(Error::Extract(_))`, store
/// stays empty.
#[tokio::test]
async fn ac8_failing_extractor_is_extract_error_and_stores_nothing() -> Result<()> {
    let store = InMemoryStore::new();
    let raw = common::bill_eml();

    let result = ingest(&raw, &Failing, &store).await;
    assert!(matches!(result, Err(Error::Extract(_))));
    assert_eq!(store.list().await?.len(), 0);
    Ok(())
}

/// AC9: a bill already stored under `raw_hash(raw)` short-circuits to `Duplicate(that id)`
/// before the (failing) extractor is ever consulted.
#[tokio::test]
async fn ac9_duplicate_hash_short_circuits_before_extractor() -> Result<()> {
    let store = InMemoryStore::new();
    let raw = common::bill_eml();
    let hash = raw_hash(&raw);
    let existing = common::extracted_bill("existing-bill")?;
    store.insert(&hash, &existing).await?;

    let outcome = ingest(&raw, &Failing, &store).await?;
    assert_eq!(outcome, Outcome::Duplicate(existing.id().clone()));
    Ok(())
}

/// AC10: `ingest(..)`'s returned future is `Send`.
#[test]
fn ac10_future_is_send() {
    fn assert_send<T: Send>(_: &T) {}
    let store = InMemoryStore::new();
    let failing = Failing;
    let raw: &[u8] = &[];
    let fut = ingest(raw, &failing, &store);
    assert_send(&fut);
}
