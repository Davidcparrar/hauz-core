//! Shared fixtures and `&dyn BillStore` scenario fns for `store`'s [unit] and [integration]
//! tests. Each `tests/*.rs` file compiles this module separately, so a helper unused by one
//! file is still valid in the other.
#![allow(dead_code)] // not every scenario/fixture fn is used by both test files

use hauz_core::bill::{Bill, BillDraft, BillId, BillingPeriod, Currency, Money, Status, Vendor};
use hauz_core::store::{BillStore, Error, InsertOutcome, RawHash};
use time::macros::date;

pub(crate) type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

/// A raw hash filled with `byte`, distinguishable per test scenario.
pub(crate) fn hash(byte: u8) -> RawHash {
    RawHash::new([byte; 32])
}

/// A complete, `Extracted` bill with the given id.
pub(crate) fn extracted_bill(id: &str) -> Result<Bill> {
    let draft = BillDraft {
        id: BillId::new(id)?,
        vendor: Some(Vendor::new("Acme Power")?),
        amount: Some(Money::new(1_234, Currency::new("USD")?)),
        period: Some(BillingPeriod::new(
            date!(2026 - 01 - 01),
            date!(2026 - 01 - 31),
        )?),
        due: Some(date!(2026 - 02 - 15)),
        status: Status::Extracted,
    };
    Ok(Bill::try_from(draft)?)
}

/// A `NeedsReview` bill with the given id and every optional field `None`.
pub(crate) fn bare_needs_review_bill(id: &str) -> Result<Bill> {
    let draft = BillDraft {
        id: BillId::new(id)?,
        vendor: None,
        amount: None,
        period: None,
        due: None,
        status: Status::NeedsReview,
    };
    Ok(Bill::try_from(draft)?)
}

/// A unique tmp-file path for a `SqliteStore` under test, so parallel tests never collide.
pub(crate) fn tmp_db_path(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("hauz-core-store-{name}-{nanos}.sqlite3"))
}

/// AC1: inserting a `Bill` under a fresh hash returns `Inserted(id)`, and `get(id)` returns an
/// equal `Bill`.
pub(crate) async fn ac1_insert_then_get(store: &dyn BillStore) -> Result<()> {
    let bill = extracted_bill("bill-1")?;
    let outcome = store.insert(&hash(1), &bill).await?;
    assert_eq!(outcome, InsertOutcome::Inserted(bill.id().clone()));
    assert_eq!(store.get(bill.id()).await?, Some(bill));
    Ok(())
}

/// AC2: `get` and `find_by_hash` return `Ok(None)` for an unknown key.
pub(crate) async fn ac2_unknown_key_is_none(store: &dyn BillStore) -> Result<()> {
    let unknown_id = BillId::new("does-not-exist")?;
    assert_eq!(store.get(&unknown_id).await?, None);
    assert_eq!(store.find_by_hash(&hash(99)).await?, None);
    Ok(())
}

/// AC3: a second insert under a stored hash returns `Duplicate(first id)` and stores nothing.
pub(crate) async fn ac3_second_insert_under_stored_hash_is_duplicate(
    store: &dyn BillStore,
) -> Result<()> {
    let first = extracted_bill("bill-1")?;
    let h = hash(2);
    store.insert(&h, &first).await?;

    let second = extracted_bill("bill-2")?;
    let outcome = store.insert(&h, &second).await?;
    assert_eq!(outcome, InsertOutcome::Duplicate(first.id().clone()));

    assert_eq!(store.list().await?.len(), 1);
    assert_eq!(store.find_by_hash(&h).await?, Some(first));
    Ok(())
}

/// AC4: inserting a `Bill` whose id exists under another hash returns `Error::DuplicateId` and
/// stores nothing.
pub(crate) async fn ac4_insert_with_known_id_under_new_hash_is_rejected(
    store: &dyn BillStore,
) -> Result<()> {
    let bill = extracted_bill("bill-1")?;
    store.insert(&hash(3), &bill).await?;

    let result = store.insert(&hash(4), &bill).await;
    assert!(matches!(result, Err(Error::DuplicateId(ref id)) if *id == *bill.id()));
    assert_eq!(store.list().await?.len(), 1);
    Ok(())
}

/// AC5: `list` returns bills in insertion order, regardless of id lexical order.
pub(crate) async fn ac5_list_returns_insertion_order(store: &dyn BillStore) -> Result<()> {
    let first = extracted_bill("zzz")?;
    let second = extracted_bill("mmm")?;
    let third = extracted_bill("aaa")?;
    store.insert(&hash(10), &first).await?;
    store.insert(&hash(11), &second).await?;
    store.insert(&hash(12), &third).await?;

    let ids: Vec<_> = store
        .list()
        .await?
        .into_iter()
        .map(|b| b.id().clone())
        .collect();
    assert_eq!(
        ids,
        vec![first.id().clone(), second.id().clone(), third.id().clone()]
    );
    Ok(())
}

/// AC6: a `NeedsReview` bill with every optional field `None` round-trips unchanged.
pub(crate) async fn ac6_bare_needs_review_bill_round_trips(store: &dyn BillStore) -> Result<()> {
    let bill = bare_needs_review_bill("bill-bare")?;
    store.insert(&hash(20), &bill).await?;
    assert_eq!(store.get(bill.id()).await?, Some(bill));
    Ok(())
}
