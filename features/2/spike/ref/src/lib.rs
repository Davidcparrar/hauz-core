//! Spike for feature #2: proves sqlx 0.9 connection setup, type round-trips, and a
//! dyn-compatible async `BillStore` trait, without `async-trait`. Throwaway; never shipped.

use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use hauz_core::bill::{Bill, BillDraft, BillId, BillingPeriod, Currency, Money, Status, Vendor};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

/// Errors this spike's store can return.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The underlying sqlx call failed.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] sqlx::Error),
    /// An embedded migration failed to apply.
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// A row's domain fields did not reconstruct into a valid `Bill`.
    #[error("stored row is not a valid bill: {0}")]
    InvalidRow(#[from] hauz_core::bill::Error),
}

/// Q1: open (or create) a WAL-mode SQLite file database and run embedded migrations.
///
/// # Errors
/// Propagates any sqlx connection or migration error.
pub async fn open(path: &std::path::Path) -> Result<SqlitePool, Error> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new().connect_with(options).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/// The dyn-compatible async store trait shape from Q3: no `async-trait`, boxed futures.
pub trait BillStore: Send + Sync {
    /// Inserts `bill` keyed by `hash`; a second insert with the same hash is a no-op and
    /// returns the id already stored.
    fn insert<'a>(
        &'a self,
        hash: &'a [u8; 32],
        bill: &'a Bill,
    ) -> Pin<Box<dyn Future<Output = Result<BillId, Error>> + Send + 'a>>;

    /// Looks a bill up by its id.
    fn get<'a>(
        &'a self,
        id: &'a BillId,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bill>, Error>> + Send + 'a>>;

    /// Looks a bill up by its content hash.
    fn find_by_hash<'a>(
        &'a self,
        hash: &'a [u8; 32],
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bill>, Error>> + Send + 'a>>;

    /// Lists every stored bill.
    fn list<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<Vec<Bill>, Error>> + Send + 'a>>;
}

/// A `SqlitePool`-backed `BillStore`.
#[derive(Debug, Clone)]
pub struct SqliteBillStore {
    pool: SqlitePool,
}

impl SqliteBillStore {
    /// Wraps an already-open, already-migrated pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn row_to_bill(row: &sqlx::sqlite::SqliteRow) -> Result<Bill, Error> {
    let id: String = row.try_get("id")?;
    let vendor: Option<String> = row.try_get("vendor")?;
    let amount_minor: Option<i64> = row.try_get("amount_minor")?;
    let currency: Option<String> = row.try_get("currency")?;
    let period_start: Option<time::Date> = row.try_get("period_start")?;
    let period_end: Option<time::Date> = row.try_get("period_end")?;
    let due: Option<time::Date> = row.try_get("due")?;
    let status: String = row.try_get("status")?;

    let draft = BillDraft {
        id: BillId::new(&id)?,
        vendor: vendor.map(|v| Vendor::new(&v)).transpose()?,
        amount: match (amount_minor, currency) {
            (Some(m), Some(c)) => Some(Money::new(m, Currency::new(&c)?)),
            _ => None,
        },
        period: match (period_start, period_end) {
            (Some(s), Some(e)) => Some(BillingPeriod::new(s, e)?),
            _ => None,
        },
        due,
        status: if status == "extracted" {
            Status::Extracted
        } else {
            Status::NeedsReview
        },
    };
    Ok(Bill::try_from(draft)?)
}

impl BillStore for SqliteBillStore {
    fn insert<'a>(
        &'a self,
        hash: &'a [u8; 32],
        bill: &'a Bill,
    ) -> Pin<Box<dyn Future<Output = Result<BillId, Error>> + Send + 'a>> {
        Box::pin(async move {
            let (currency, amount_minor) = bill
                .amount()
                .map(|m| (Some(m.currency().as_str().to_owned()), Some(m.minor_units())))
                .unwrap_or((None, None));
            let (period_start, period_end) = bill
                .period()
                .map(|p| (Some(p.start()), Some(p.end())))
                .unwrap_or((None, None));
            let status = match bill.status() {
                Status::Extracted => "extracted",
                Status::NeedsReview => "needs_review",
            };

            sqlx::query(
                "INSERT INTO bills \
                 (id, hash, vendor, amount_minor, currency, period_start, period_end, due, status) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(hash) DO NOTHING",
            )
            .bind(bill.id().as_str())
            .bind(hash.as_slice())
            .bind(bill.vendor().map(|v| v.name()))
            .bind(amount_minor)
            .bind(currency)
            .bind(period_start)
            .bind(period_end)
            .bind(bill.due())
            .bind(status)
            .execute(&self.pool)
            .await?;

            let row = sqlx::query("SELECT id FROM bills WHERE hash = ?")
                .bind(hash.as_slice())
                .fetch_one(&self.pool)
                .await?;
            let id: String = row.try_get("id")?;
            Ok(BillId::new(&id)?)
        })
    }

    fn get<'a>(
        &'a self,
        id: &'a BillId,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bill>, Error>> + Send + 'a>> {
        Box::pin(async move {
            let row = sqlx::query("SELECT * FROM bills WHERE id = ?")
                .bind(id.as_str())
                .fetch_optional(&self.pool)
                .await?;
            row.as_ref().map(row_to_bill).transpose()
        })
    }

    fn find_by_hash<'a>(
        &'a self,
        hash: &'a [u8; 32],
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bill>, Error>> + Send + 'a>> {
        Box::pin(async move {
            let row = sqlx::query("SELECT * FROM bills WHERE hash = ?")
                .bind(hash.as_slice())
                .fetch_optional(&self.pool)
                .await?;
            row.as_ref().map(row_to_bill).transpose()
        })
    }

    fn list<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<Vec<Bill>, Error>> + Send + 'a>> {
        Box::pin(async move {
            let rows = sqlx::query("SELECT * FROM bills").fetch_all(&self.pool).await?;
            rows.iter().map(row_to_bill).collect()
        })
    }
}

/// An in-memory `BillStore`, the fake other modules use at their system edge.
#[derive(Debug, Default)]
pub struct MemoryBillStore {
    rows: Mutex<Vec<([u8; 32], Bill)>>,
}

impl BillStore for MemoryBillStore {
    fn insert<'a>(
        &'a self,
        hash: &'a [u8; 32],
        bill: &'a Bill,
    ) -> Pin<Box<dyn Future<Output = Result<BillId, Error>> + Send + 'a>> {
        Box::pin(async move {
            // A `Mutex` is a fine fake at this system edge; no `.await` is held across the lock.
            let mut rows = self.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some((_, existing)) = rows.iter().find(|(h, _)| h == hash) {
                return Ok(existing.id().clone());
            }
            rows.push((*hash, bill.clone()));
            Ok(bill.id().clone())
        })
    }

    fn get<'a>(
        &'a self,
        id: &'a BillId,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bill>, Error>> + Send + 'a>> {
        Box::pin(async move {
            let rows = self.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            Ok(rows.iter().find(|(_, b)| b.id() == id).map(|(_, b)| b.clone()))
        })
    }

    fn find_by_hash<'a>(
        &'a self,
        hash: &'a [u8; 32],
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bill>, Error>> + Send + 'a>> {
        Box::pin(async move {
            let rows = self.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            Ok(rows.iter().find(|(h, _)| h == hash).map(|(_, b)| b.clone()))
        })
    }

    fn list<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<Vec<Bill>, Error>> + Send + 'a>> {
        Box::pin(async move {
            let rows = self.rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            Ok(rows.iter().map(|(_, b)| b.clone()).collect())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{open, BillStore, Error, MemoryBillStore, SqliteBillStore};
    use hauz_core::bill::{BillDraft, BillId, BillingPeriod, Currency, Money, Status, Vendor};
    use sqlx::Row;

    fn tmp_db_path(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("spike2-{name}-{nanos}.sqlite3"))
    }

    fn sample_bill(id: &str) -> hauz_core::bill::Bill {
        let draft = BillDraft {
            id: BillId::new(id).unwrap(),
            vendor: Some(Vendor::new("Acme Power").unwrap()),
            amount: Some(Money::new(1_234, Currency::new("USD").unwrap())),
            period: Some(
                BillingPeriod::new(
                    time::Date::from_calendar_date(2026, time::Month::January, 1).unwrap(),
                    time::Date::from_calendar_date(2026, time::Month::January, 31).unwrap(),
                )
                .unwrap(),
            ),
            due: Some(time::Date::from_calendar_date(2026, time::Month::February, 15).unwrap()),
            status: Status::Extracted,
        };
        hauz_core::bill::Bill::try_from(draft).unwrap()
    }

    // Q1: file db, create-if-missing, WAL, embedded migrations.
    #[tokio::test]
    async fn q1_opens_and_migrates() -> Result<(), Error> {
        let path = tmp_db_path("q1");
        let pool = open(&path).await?;
        let mode: String = sqlx::query("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await?
            .try_get(0)?;
        assert_eq!(mode.to_lowercase(), "wal");
        let count: i64 = sqlx::query("SELECT count(*) FROM bills")
            .fetch_one(&pool)
            .await?
            .try_get(0)?;
        assert_eq!(count, 0);
        pool.close().await;
        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    // Q2: Date, Option<Date>, i64, 32-byte hash round-trip via bind/try_get.
    #[tokio::test]
    async fn q2_round_trips_types() -> Result<(), Error> {
        let path = tmp_db_path("q2");
        let pool = open(&path).await?;
        sqlx::query(
            "CREATE TABLE roundtrip (d DATE NOT NULL, d_opt DATE, n INTEGER NOT NULL, h BLOB NOT NULL)",
        )
        .execute(&pool)
        .await?;

        let d = time::Date::from_calendar_date(2026, time::Month::March, 3).unwrap();
        let d_opt: Option<time::Date> = None;
        let n: i64 = i64::MAX;
        let h: [u8; 32] = [7u8; 32];

        sqlx::query("INSERT INTO roundtrip (d, d_opt, n, h) VALUES (?, ?, ?, ?)")
            .bind(d)
            .bind(d_opt)
            .bind(n)
            .bind(h.as_slice())
            .execute(&pool)
            .await?;

        let row = sqlx::query("SELECT d, d_opt, n, h FROM roundtrip").fetch_one(&pool).await?;
        let got_d: time::Date = row.try_get("d")?;
        let got_d_opt: Option<time::Date> = row.try_get("d_opt")?;
        let got_n: i64 = row.try_get("n")?;
        let got_h_vec: Vec<u8> = row.try_get("h")?;
        let got_h: [u8; 32] = got_h_vec.try_into().map_err(|_| {
            Error::Sqlite(sqlx::Error::Decode("blob was not 32 bytes".into()))
        })?;

        assert_eq!(got_d, d);
        assert_eq!(got_d_opt, d_opt);
        assert_eq!(got_n, n);
        assert_eq!(got_h, h);

        pool.close().await;
        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    // Q3: dyn dispatch through `&dyn BillStore` for both backends, plus hash dedup.
    async fn exercise(store: &dyn BillStore) -> Result<(), Error> {
        let hash = [42u8; 32];
        let bill = sample_bill("bill-1");
        let id1 = store.insert(&hash, &bill).await?;

        let other = sample_bill("bill-2");
        let id2 = store.insert(&hash, &other).await?;
        assert_eq!(id1, id2, "second insert with same hash must return the existing id");

        let found = store.find_by_hash(&hash).await?;
        assert!(found.is_some());

        let fetched = store.get(&id1).await?;
        assert!(fetched.is_some());

        let all = store.list().await?;
        assert_eq!(all.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn q3_sqlite_backend_through_dyn() -> Result<(), Error> {
        let path = tmp_db_path("q3-sqlite");
        let pool = open(&path).await?;
        let store = SqliteBillStore::new(pool.clone());
        exercise(&store).await?;
        pool.close().await;
        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    #[tokio::test]
    async fn q3_memory_backend_through_dyn() -> Result<(), Error> {
        let store = MemoryBillStore::default();
        exercise(&store).await
    }

    // Q4: a handful of concurrent inserts against a default (multi-connection) pool.
    #[tokio::test]
    async fn q4_concurrent_inserts() -> Result<(), Error> {
        let path = tmp_db_path("q4");
        let pool = open(&path).await?;
        let store = std::sync::Arc::new(SqliteBillStore::new(pool.clone()));

        let mut handles = Vec::new();
        for i in 0..8u8 {
            let store = std::sync::Arc::clone(&store);
            handles.push(tokio::spawn(async move {
                let hash = [i; 32];
                let bill = sample_bill(&format!("concurrent-{i}"));
                store.insert(&hash, &bill).await
            }));
        }
        for handle in handles {
            handle.await.map_err(|_| Error::Sqlite(sqlx::Error::PoolClosed))??;
        }

        let all = store.list().await?;
        assert_eq!(all.len(), 8);

        pool.close().await;
        let _ = std::fs::remove_file(&path);
        Ok(())
    }
}
