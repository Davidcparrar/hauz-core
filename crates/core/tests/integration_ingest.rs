//! [integration] tests for the `ingest` module's public API: the real `TextExtractor` and a
//! real `SqliteStore` on a unique tmp-file database. One file per level per module. Test fn
//! names carry the spec criterion they satisfy: `acN_<behavior>`.

mod common;

use common::{Result, TmpDbFile};
use hauz_core::bill::{Currency, Money, Status, Vendor};
use hauz_core::extract::TextExtractor;
use hauz_core::ingest::{Error, Outcome, ingest, raw_hash};
use hauz_core::store::{BillStore, SqliteStore};
use time::macros::date;

const PLAIN: &[u8] = include_bytes!("fixtures/plain.eml");
const MALFORMED: &[u8] = include_bytes!("fixtures/malformed.eml");

/// Lowercase hex encoding, test-local (the crate's own encoder is private).
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// AC1: `bill_eml()` ingested into an empty store returns `Created(id)` whose id is the
/// lowercase hex of `raw_hash(raw)`, stored as `NeedsReview` with amount, due, and vendor set,
/// no period.
#[tokio::test]
async fn ac1_bill_eml_into_empty_store_is_created_needs_review() -> Result<()> {
    let db = TmpDbFile::new("ingest-ac1");
    let store = SqliteStore::open(&db.path).await?;
    let raw = common::bill_eml();

    let outcome = ingest(&raw, &TextExtractor, &store).await?;
    let Outcome::Created(id) = outcome else {
        return Err(format!("expected Created, got {outcome:?}").into());
    };
    assert_eq!(id.as_str(), to_hex(raw_hash(&raw).as_bytes()).as_str());

    let bill = store.get(&id).await?.ok_or("missing bill")?;
    assert_eq!(bill.status(), Status::NeedsReview);
    assert_eq!(
        bill.amount(),
        Some(&Money::new(123_456, Currency::new("EUR")?))
    );
    assert_eq!(bill.due(), Some(date!(2026 - 10 - 15)));
    assert_eq!(bill.vendor(), Some(&Vendor::new("acme-power.example")?));
    assert_eq!(bill.period(), None);
    Ok(())
}

/// AC2: the same bytes ingested again return `Duplicate(id)` with AC1's id, and `list` still
/// has length 1.
#[tokio::test]
async fn ac2_same_bytes_twice_is_duplicate_and_list_len_one() -> Result<()> {
    let db = TmpDbFile::new("ingest-ac2");
    let store = SqliteStore::open(&db.path).await?;
    let raw = common::bill_eml();

    let first = ingest(&raw, &TextExtractor, &store).await?;
    let Outcome::Created(id) = first else {
        return Err(format!("expected Created, got {first:?}").into());
    };

    let second = ingest(&raw, &TextExtractor, &store).await?;
    assert_eq!(second, Outcome::Duplicate(id));
    assert_eq!(store.list().await?.len(), 1);
    Ok(())
}

/// AC3: a message with no amount and no date is `Created(_)`, stored `NeedsReview` with
/// amount and due both `None`.
#[tokio::test]
async fn ac3_no_amount_no_date_is_created_needs_review_with_nones() -> Result<()> {
    let db = TmpDbFile::new("ingest-ac3");
    let store = SqliteStore::open(&db.path).await?;

    let outcome = ingest(PLAIN, &TextExtractor, &store).await?;
    let Outcome::Created(id) = outcome else {
        return Err(format!("expected Created, got {outcome:?}").into());
    };
    let bill = store.get(&id).await?.ok_or("missing bill")?;
    assert_eq!(bill.status(), Status::NeedsReview);
    assert_eq!(bill.amount(), None);
    assert_eq!(bill.due(), None);
    Ok(())
}

/// AC4: a malformed message returns `Err(Error::Email(_))` and stores nothing, on the first
/// call and again on a second.
#[tokio::test]
async fn ac4_malformed_message_is_email_error_and_list_stays_empty() -> Result<()> {
    let db = TmpDbFile::new("ingest-ac4");
    let store = SqliteStore::open(&db.path).await?;

    let first = ingest(MALFORMED, &TextExtractor, &store).await;
    assert!(matches!(first, Err(Error::Email(_))));
    assert_eq!(store.list().await?.len(), 0);

    let second = ingest(MALFORMED, &TextExtractor, &store).await;
    assert!(matches!(second, Err(Error::Email(_))));
    assert_eq!(store.list().await?.len(), 0);
    Ok(())
}
