//! [integration] tests for the `store` module's public API: `SqliteStore` against a real,
//! unique tmp-file SQLite database. One file per level per module. Test fn names carry the
//! spec criterion they satisfy: `acN_<behavior>`.

mod common;

use common::{Result, TmpDbFile};
use hauz_core::store::{BillStore, Error, InsertOutcome, SqliteStore};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{ConnectOptions, Row};
use time::OffsetDateTime;

#[tokio::test]
async fn ac1_insert_then_get() -> Result<()> {
    let db = TmpDbFile::new("ac1");
    let store = SqliteStore::open(&db.path).await?;
    common::ac1_insert_then_get(&store).await
}

#[tokio::test]
async fn ac2_unknown_key_is_none() -> Result<()> {
    let db = TmpDbFile::new("ac2");
    let store = SqliteStore::open(&db.path).await?;
    common::ac2_unknown_key_is_none(&store).await
}

#[tokio::test]
async fn ac3_second_insert_under_stored_hash_is_duplicate() -> Result<()> {
    let db = TmpDbFile::new("ac3");
    let store = SqliteStore::open(&db.path).await?;
    common::ac3_second_insert_under_stored_hash_is_duplicate(&store).await
}

#[tokio::test]
async fn ac4_insert_with_known_id_under_new_hash_is_rejected() -> Result<()> {
    let db = TmpDbFile::new("ac4");
    let store = SqliteStore::open(&db.path).await?;
    common::ac4_insert_with_known_id_under_new_hash_is_rejected(&store).await
}

#[tokio::test]
async fn ac5_list_returns_insertion_order() -> Result<()> {
    let db = TmpDbFile::new("ac5");
    let store = SqliteStore::open(&db.path).await?;
    common::ac5_list_returns_insertion_order(&store).await
}

#[tokio::test]
async fn ac6_bare_needs_review_bill_round_trips() -> Result<()> {
    let db = TmpDbFile::new("ac6");
    let store = SqliteStore::open(&db.path).await?;
    common::ac6_bare_needs_review_bill_round_trips(&store).await
}

#[tokio::test]
async fn ac7_reopen_sees_rows_from_first_handle() -> Result<()> {
    let db = TmpDbFile::new("ac7");
    let first = SqliteStore::open(&db.path).await?;
    let bill = common::extracted_bill("bill-1")?;
    first.insert(&common::hash(1), &bill).await?;

    let second = SqliteStore::open(&db.path).await?;
    assert_eq!(second.get(bill.id()).await?, Some(bill));
    Ok(())
}

/// #54 AC3: `SqliteStore::replace` behaves as the in-memory store and leaves the `hash` and
/// `inserted_at` columns untouched.
#[tokio::test]
async fn ac3_replace_behaves_like_in_memory_and_keeps_hash_and_inserted_at() -> Result<()> {
    let db = TmpDbFile::new("replace-ac3");
    let store = SqliteStore::open(&db.path).await?;
    let old = common::extracted_bill("rep-a")?;
    store.insert(&common::hash(50), &old).await?;

    let mut raw = SqliteConnectOptions::new()
        .filename(&db.path)
        .connect()
        .await?;
    let columns = "SELECT hash, inserted_at FROM bills WHERE id = 'rep-a'";
    let before = sqlx::query(columns).fetch_one(&mut raw).await?;
    let hash_before: Vec<u8> = before.try_get("hash")?;
    let inserted_before: OffsetDateTime = before.try_get("inserted_at")?;

    let new = common::bare_needs_review_bill("rep-a")?;
    store.replace(&new).await?;
    let after = sqlx::query(columns).fetch_one(&mut raw).await?;
    let hash_after: Vec<u8> = after.try_get("hash")?;
    let inserted_after: OffsetDateTime = after.try_get("inserted_at")?;
    assert_eq!(hash_before, hash_after);
    assert_eq!(inserted_before, inserted_after);
    assert_eq!(store.get(old.id()).await?, Some(new));

    let same_scenario = TmpDbFile::new("replace-ac3-ac1");
    common::ac1_replace_overwrites_and_keeps_position(
        &SqliteStore::open(&same_scenario.path).await?,
    )
    .await?;
    let unknown = TmpDbFile::new("replace-ac3-ac2");
    common::ac2_replace_unknown_id_is_not_found(&SqliteStore::open(&unknown.path).await?).await
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
    let db = TmpDbFile::new("ac9");
    let _store = SqliteStore::open(&db.path).await?;

    let mut conn = SqliteConnectOptions::new()
        .filename(&db.path)
        .connect()
        .await?;
    let mode: String = sqlx::query("PRAGMA journal_mode")
        .fetch_one(&mut conn)
        .await?
        .try_get(0)?;
    assert_eq!(mode.to_lowercase(), "wal");
    Ok(())
}

/// Inserts a raw, unvalidated row directly through `conn`, bypassing every constructor the
/// store normally routes writes through — the only way to get a corrupt row into the table.
async fn insert_raw_row(
    conn: &mut sqlx::SqliteConnection,
    id_raw: &str,
    hash_byte: u8,
    status_raw: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO bills (id, hash, status, inserted_at) VALUES (?, ?, ?, ?)")
        .bind(id_raw)
        .bind([hash_byte; 32].as_slice())
        .bind(status_raw)
        .bind(OffsetDateTime::now_utc())
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// AC10: a row with an unknown `status` reports `Error::Corrupt` carrying the row's (valid)
/// raw id from every read path, including `get`.
#[tokio::test]
async fn ac10_corrupt_status_reports_error() -> Result<()> {
    let db = TmpDbFile::new("ac10-status");
    let store = SqliteStore::open(&db.path).await?;

    let mut raw = SqliteConnectOptions::new()
        .filename(&db.path)
        .connect()
        .await?;
    insert_raw_row(&mut raw, "corrupt-status", 40, "bogus").await?;

    let id = hauz_core::bill::BillId::new("corrupt-status")?;
    let hash = common::hash(40);

    let get_err = store.get(&id).await;
    assert!(
        matches!(&get_err, Err(Error::Corrupt { id, .. }) if id == "corrupt-status"),
        "get returned {get_err:?}"
    );

    let find_err = store.find_by_hash(&hash).await;
    assert!(
        matches!(&find_err, Err(Error::Corrupt { id, .. }) if id == "corrupt-status"),
        "find_by_hash returned {find_err:?}"
    );

    let list_err = store.list().await;
    assert!(
        matches!(&list_err, Err(Error::Corrupt { id, .. }) if id == "corrupt-status"),
        "list returned {list_err:?}"
    );
    Ok(())
}

/// AC10: a row whose `id` column fails `BillId::new` (contains whitespace) reports
/// `Error::Corrupt` carrying that raw id from `find_by_hash` and `list`; `get` cannot even
/// be asked for it, since no `BillId` can represent that text.
#[tokio::test]
async fn ac10_corrupt_id_reports_error() -> Result<()> {
    let db = TmpDbFile::new("ac10-id");
    let store = SqliteStore::open(&db.path).await?;

    let mut raw = SqliteConnectOptions::new()
        .filename(&db.path)
        .connect()
        .await?;
    insert_raw_row(&mut raw, "bad id", 41, "extracted").await?;

    let hash = common::hash(41);

    let find_err = store.find_by_hash(&hash).await;
    assert!(
        matches!(&find_err, Err(Error::Corrupt { id, .. }) if id == "bad id"),
        "find_by_hash returned {find_err:?}"
    );

    let list_err = store.list().await;
    assert!(
        matches!(&list_err, Err(Error::Corrupt { id, .. }) if id == "bad id"),
        "list returned {list_err:?}"
    );
    Ok(())
}

/// Concurrent writers contend for SQLite's single write lock. `BEGIN IMMEDIATE` takes it
/// up front, so contention must be absorbed by the busy timeout, never surfaced as
/// `Error::Backend`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn store_busy_timeout_concurrent_inserts_all_succeed() -> Result<()> {
    let db = TmpDbFile::new("busy");
    let store = SqliteStore::open(&db.path).await?;

    let mut tasks = tokio::task::JoinSet::new();
    for n in 0..16u8 {
        let store = store.clone();
        tasks.spawn(async move {
            // Task errors cross a thread boundary, so they travel as `String`, not `Box<dyn Error>`.
            let bill = common::extracted_bill(&format!("bill-{n}")).map_err(|e| e.to_string())?;
            let outcome = store
                .insert(&common::hash(n), &bill)
                .await
                .map_err(|e| e.to_string())?;
            assert_eq!(outcome, InsertOutcome::Inserted(bill.id().clone()));
            Ok::<(), String>(())
        });
    }
    while let Some(joined) = tasks.join_next().await {
        joined??;
    }

    assert_eq!(store.list().await?.len(), 16);
    Ok(())
}

/// AC6 (#37): a bill with `issued: Some(d)` reads back that `issued` from `get`,
/// `find_by_hash` and `list`.
#[tokio::test]
async fn ac6_issued_round_trips_through_every_read_path() -> Result<()> {
    let db = TmpDbFile::new("issued-ac6");
    let store = SqliteStore::open(&db.path).await?;
    let bill = common::extracted_bill("bill-issued")?;
    let issued = bill.issued();
    assert!(issued.is_some());
    store.insert(&common::hash(60), &bill).await?;

    let got = store.get(bill.id()).await?.ok_or("get: missing")?;
    assert_eq!(got.issued(), issued);
    let found = store
        .find_by_hash(&common::hash(60))
        .await?
        .ok_or("find_by_hash: missing")?;
    assert_eq!(found.issued(), issued);
    let listed = store.list().await?;
    assert_eq!(listed.first().and_then(|b| b.issued()), issued);
    Ok(())
}

/// AC7 (#37): a file migrated with only `0001` and holding a raw row is upgraded by
/// `SqliteStore::open`; the row reads back with `issued() == None`, the rest unchanged.
#[tokio::test]
async fn ac7_open_applies_0002_to_a_0001_only_database() -> Result<()> {
    let db = TmpDbFile::new("issued-ac7");
    let dir = std::env::temp_dir().join(format!("hauz-mig-0001-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations/0001_bills.sql"),
        dir.join("0001_bills.sql"),
    )?;

    let mut conn = SqliteConnectOptions::new()
        .filename(&db.path)
        .create_if_missing(true)
        .connect()
        .await?;
    sqlx::migrate::Migrator::new(dir.as_path())
        .await?
        .run(&mut conn)
        .await?;
    sqlx::query(
        "INSERT INTO bills (id, hash, vendor, amount_minor, currency, period_start, period_end, \
         due, status, inserted_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("old-row")
    .bind([70u8; 32].as_slice())
    .bind("Acme Power")
    .bind(1_234i64)
    .bind("USD")
    .bind(time::macros::date!(2026 - 01 - 01))
    .bind(time::macros::date!(2026 - 01 - 31))
    .bind(time::macros::date!(2026 - 02 - 15))
    .bind("extracted")
    .bind(OffsetDateTime::now_utc())
    .execute(&mut conn)
    .await?;
    drop(conn);
    std::fs::remove_dir_all(&dir)?;

    let store = SqliteStore::open(&db.path).await?;
    let bill = store
        .get(&hauz_core::bill::BillId::new("old-row")?)
        .await?
        .ok_or("old row missing")?;
    assert_eq!(bill.issued(), None);
    assert_eq!(bill.vendor().map(|v| v.name()), Some("Acme Power"));
    assert_eq!(bill.amount().map(|m| m.minor_units()), Some(1_234));
    assert_eq!(bill.due(), Some(time::macros::date!(2026 - 02 - 15)));
    assert_eq!(bill.status(), hauz_core::bill::Status::Extracted);
    Ok(())
}

/// AC6 (#38): a row whose stored currency is no longer valid is `Corrupt` with that row's id
/// from `get`, `find_by_hash` and `list`.
#[tokio::test]
async fn ac6_non_iso_stored_currency_is_corrupt() -> Result<()> {
    let db = TmpDbFile::new("iso-ac6");
    drop(SqliteStore::open(&db.path).await?);
    let mut conn = SqliteConnectOptions::new()
        .filename(&db.path)
        .connect()
        .await?;
    sqlx::query(
        "INSERT INTO bills (id, hash, vendor, amount_minor, currency, period_start, period_end, \
         due, status, inserted_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("nit-row")
    .bind([80u8; 32].as_slice())
    .bind("Acme Power")
    .bind(1_234i64)
    .bind("NIT")
    .bind(time::macros::date!(2026 - 01 - 01))
    .bind(time::macros::date!(2026 - 01 - 31))
    .bind(time::macros::date!(2026 - 02 - 15))
    .bind("extracted")
    .bind(OffsetDateTime::now_utc())
    .execute(&mut conn)
    .await?;
    drop(conn);

    let store = SqliteStore::open(&db.path).await?;
    let is_nit_row = |e: &Error| matches!(e, Error::Corrupt { id, .. } if id == "nit-row");
    let got = store.get(&hauz_core::bill::BillId::new("nit-row")?).await;
    assert!(got.as_ref().err().is_some_and(is_nit_row), "get: {got:?}");
    let found = store.find_by_hash(&common::hash(80)).await;
    assert!(
        found.as_ref().err().is_some_and(is_nit_row),
        "find_by_hash: {found:?}"
    );
    let listed = store.list().await;
    assert!(
        listed.as_ref().err().is_some_and(is_nit_row),
        "list: {listed:?}"
    );
    Ok(())
}
