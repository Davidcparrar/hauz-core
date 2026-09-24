//! [integration] tests for the `store` module's public API: `SqliteStore` against a real,
//! unique tmp-file SQLite database. One file per level per module. Test fn names carry the
//! spec criterion they satisfy: `acN_<behavior>`.

mod common;

use common::Result;
use hauz_core::store::{BillStore, Error, SqliteStore};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{ConnectOptions, Row};

#[tokio::test]
async fn ac1_insert_then_get() -> Result<()> {
    let store = SqliteStore::open(&common::tmp_db_path("ac1")).await?;
    common::ac1_insert_then_get(&store).await
}

#[tokio::test]
async fn ac2_unknown_key_is_none() -> Result<()> {
    let store = SqliteStore::open(&common::tmp_db_path("ac2")).await?;
    common::ac2_unknown_key_is_none(&store).await
}

#[tokio::test]
async fn ac3_second_insert_under_stored_hash_is_duplicate() -> Result<()> {
    let store = SqliteStore::open(&common::tmp_db_path("ac3")).await?;
    common::ac3_second_insert_under_stored_hash_is_duplicate(&store).await
}

#[tokio::test]
async fn ac4_insert_with_known_id_under_new_hash_is_rejected() -> Result<()> {
    let store = SqliteStore::open(&common::tmp_db_path("ac4")).await?;
    common::ac4_insert_with_known_id_under_new_hash_is_rejected(&store).await
}

#[tokio::test]
async fn ac5_list_returns_insertion_order() -> Result<()> {
    let store = SqliteStore::open(&common::tmp_db_path("ac5")).await?;
    common::ac5_list_returns_insertion_order(&store).await
}

#[tokio::test]
async fn ac6_bare_needs_review_bill_round_trips() -> Result<()> {
    let store = SqliteStore::open(&common::tmp_db_path("ac6")).await?;
    common::ac6_bare_needs_review_bill_round_trips(&store).await
}

#[tokio::test]
async fn ac7_reopen_sees_rows_from_first_handle() -> Result<()> {
    let path = common::tmp_db_path("ac7");
    let first = SqliteStore::open(&path).await?;
    let bill = common::extracted_bill("bill-1")?;
    first.insert(&common::hash(1), &bill).await?;

    let second = SqliteStore::open(&path).await?;
    assert_eq!(second.get(bill.id()).await?, Some(bill));

    let _ = std::fs::remove_file(&path);
    Ok(())
}

#[tokio::test]
async fn ac8_open_with_missing_parent_directory_is_backend_error() -> Result<()> {
    let path = std::env::temp_dir().join("hauz-core-store-missing-dir-ac8/nested/file.sqlite3");
    let result = SqliteStore::open(&path).await;
    assert!(matches!(result, Err(Error::Backend(_))));
    Ok(())
}

#[tokio::test]
async fn ac9_journal_mode_is_wal_via_separate_connection() -> Result<()> {
    let path = common::tmp_db_path("ac9");
    let _store = SqliteStore::open(&path).await?;

    let mut conn = SqliteConnectOptions::new()
        .filename(&path)
        .connect()
        .await?;
    let mode: String = sqlx::query("PRAGMA journal_mode")
        .fetch_one(&mut conn)
        .await?
        .try_get(0)?;
    assert_eq!(mode.to_lowercase(), "wal");

    let _ = std::fs::remove_file(&path);
    Ok(())
}
