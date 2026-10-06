//! [unit] tests for the `store` module's public API: `InMemoryStore`. One file per level per
//! module. Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

mod common;

use common::{Result, TmpDbFile};
use hauz_core::store::{BillStore, Error, InMemoryStore, SqliteStore};

#[tokio::test]
async fn ac1_insert_then_get() -> Result<()> {
    common::ac1_insert_then_get(&InMemoryStore::new()).await
}

#[tokio::test]
async fn ac2_unknown_key_is_none() -> Result<()> {
    common::ac2_unknown_key_is_none(&InMemoryStore::new()).await
}

#[tokio::test]
async fn ac3_second_insert_under_stored_hash_is_duplicate() -> Result<()> {
    common::ac3_second_insert_under_stored_hash_is_duplicate(&InMemoryStore::new()).await
}

#[tokio::test]
async fn ac4_insert_with_known_id_under_new_hash_is_rejected() -> Result<()> {
    common::ac4_insert_with_known_id_under_new_hash_is_rejected(&InMemoryStore::new()).await
}

#[tokio::test]
async fn ac5_list_returns_insertion_order() -> Result<()> {
    common::ac5_list_returns_insertion_order(&InMemoryStore::new()).await
}

#[tokio::test]
async fn ac6_bare_needs_review_bill_round_trips() -> Result<()> {
    common::ac6_bare_needs_review_bill_round_trips(&InMemoryStore::new()).await
}

#[tokio::test]
async fn ac1_replace_overwrites_and_keeps_position() -> Result<()> {
    common::ac1_replace_overwrites_and_keeps_position(&InMemoryStore::new()).await
}

#[tokio::test]
async fn ac2_replace_unknown_id_is_not_found() -> Result<()> {
    common::ac2_replace_unknown_id_is_not_found(&InMemoryStore::new()).await
}

#[tokio::test]
async fn ac1_read_only_missing_path_errors_without_creating() {
    let db = TmpDbFile::new("ro-ac1");
    let result = SqliteStore::open_read_only(&db.path).await;
    assert!(matches!(result, Err(Error::Backend(_))));
    assert!(!db.path.exists());
}

#[tokio::test]
async fn ac2_read_only_lists_both_bills_beside_open_writer() -> Result<()> {
    let db = TmpDbFile::new("ro-ac2");
    let writer = SqliteStore::open(&db.path).await?;
    writer
        .insert(&common::hash(1), &common::extracted_bill("first")?)
        .await?;
    writer
        .insert(&common::hash(2), &common::bare_needs_review_bill("second")?)
        .await?;

    let reader = SqliteStore::open_read_only(&db.path).await?;
    assert_eq!(reader.list().await?, writer.list().await?);
    assert_eq!(reader.list().await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn ac3_read_only_insert_is_rejected_and_store_unchanged() -> Result<()> {
    let db = TmpDbFile::new("ro-ac3");
    let writer = SqliteStore::open(&db.path).await?;
    writer
        .insert(&common::hash(1), &common::extracted_bill("first")?)
        .await?;
    let before = writer.list().await?;

    let reader = SqliteStore::open_read_only(&db.path).await?;
    let result = reader
        .insert(&common::hash(2), &common::extracted_bill("second")?)
        .await;
    assert!(matches!(result, Err(Error::Backend(_))));
    assert_eq!(writer.list().await?, before);
    Ok(())
}
