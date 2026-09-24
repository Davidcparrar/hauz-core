//! Persistence for `Bill`s: the `BillStore` trait (the system edge other modules depend on),
//! a `SqliteStore` backed by sqlx with embedded migrations, and an `InMemoryStore` fake with
//! identical semantics. Insert is idempotent on the raw-message hash the caller computes: the
//! same email twice yields one row and the same id.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Mutex, PoisonError};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;

use crate::bill;
use crate::bill::{Bill, BillDraft, BillId, BillingPeriod, Currency, Money, Status, Vendor};

/// A 32-byte hash of the raw message a `Bill` was extracted from. `store` only keys on it;
/// computing it is the caller's job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RawHash([u8; 32]);

impl RawHash {
    /// Wraps a 32-byte hash.
    #[must_use]
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The raw bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Errors a `BillStore` can return.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The bill's id already exists under a different hash.
    #[error("bill id already exists: {0:?}")]
    DuplicateId(BillId),
    /// A stored row did not reconstruct into a valid `Bill`.
    #[error("stored row for {id:?} is corrupt: {source}")]
    Corrupt {
        /// The id of the corrupt row.
        id: BillId,
        /// Why the row's fields did not reconstruct a valid `Bill`.
        source: bill::Error,
    },
    /// The underlying storage backend failed.
    #[error("storage backend error: {0}")]
    Backend(#[from] sqlx::Error),
    /// An embedded migration failed to apply.
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

/// The result of an `insert`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    /// The bill was newly stored under this id.
    Inserted(BillId),
    /// A bill was already stored under this hash; carries that row's id.
    Duplicate(BillId),
}

/// A future boxed for a dyn-compatible async trait: no `async-trait`, no `unsafe`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Persistence for `Bill`s, keyed by id and by the raw-message hash that produced them.
/// Dyn-compatible: callers hold `&dyn BillStore` at the system edge.
pub trait BillStore: Send + Sync {
    /// Inserts `bill` keyed by `hash`.
    ///
    /// Check order: a row already stored under `hash` yields `Duplicate(existing id)` and
    /// stores nothing; else a row already stored under `bill.id()` (a different hash) yields
    /// `Error::DuplicateId` and stores nothing; else the row is inserted and `Inserted(id)` is
    /// returned.
    fn insert<'a>(
        &'a self,
        hash: &'a RawHash,
        bill: &'a Bill,
    ) -> BoxFuture<'a, Result<InsertOutcome, Error>>;

    /// Looks a bill up by its id. `Ok(None)` when no row matches.
    fn get<'a>(&'a self, id: &'a BillId) -> BoxFuture<'a, Result<Option<Bill>, Error>>;

    /// Looks a bill up by its raw-message hash. `Ok(None)` when no row matches.
    fn find_by_hash<'a>(&'a self, hash: &'a RawHash) -> BoxFuture<'a, Result<Option<Bill>, Error>>;

    /// Lists every stored bill in insertion order, oldest first.
    fn list<'a>(&'a self) -> BoxFuture<'a, Result<Vec<Bill>, Error>>;
}

/// Parses a row's `id` column, which the store always writes as an already-validated
/// [`BillId`]; a failure here means the column no longer holds that invariant.
fn decode_id(raw: &str) -> Result<BillId, Error> {
    BillId::new(raw).map_err(|_| {
        Error::Backend(sqlx::Error::Decode(
            format!("invalid id column: {raw:?}").into(),
        ))
    })
}

/// Rebuilds a `Bill` from a `SELECT id, vendor, amount_minor, currency, period_start,
/// period_end, due, status` row.
fn row_to_bill(row: &SqliteRow) -> Result<Bill, Error> {
    let id_raw: String = row.try_get("id")?;
    let id = decode_id(&id_raw)?;
    let vendor: Option<String> = row.try_get("vendor")?;
    let amount_minor: Option<i64> = row.try_get("amount_minor")?;
    let currency: Option<String> = row.try_get("currency")?;
    let period_start: Option<time::Date> = row.try_get("period_start")?;
    let period_end: Option<time::Date> = row.try_get("period_end")?;
    let due: Option<time::Date> = row.try_get("due")?;
    let status_raw: String = row.try_get("status")?;

    let draft_result: Result<BillDraft, bill::Error> = (|| {
        Ok(BillDraft {
            id: id.clone(),
            vendor: vendor.map(|v| Vendor::new(&v)).transpose()?,
            amount: match (amount_minor, currency) {
                (Some(minor_units), Some(code)) => {
                    Some(Money::new(minor_units, Currency::new(&code)?))
                }
                _ => None,
            },
            period: match (period_start, period_end) {
                (Some(start), Some(end)) => Some(BillingPeriod::new(start, end)?),
                _ => None,
            },
            due,
            status: if status_raw == "extracted" {
                Status::Extracted
            } else {
                Status::NeedsReview
            },
        })
    })();

    let draft = draft_result.map_err(|source| Error::Corrupt {
        id: id.clone(),
        source,
    })?;
    Bill::try_from(draft).map_err(|source| Error::Corrupt { id, source })
}

/// A `SqlitePool`-backed `BillStore`: one file, WAL mode, embedded migrations.
#[derive(Debug, Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    /// Opens (creating if missing) a WAL-mode SQLite file at `path` and runs embedded
    /// migrations. Safe to call more than once on the same path: later handles see rows
    /// inserted through earlier ones.
    ///
    /// # Errors
    /// Returns [`Error::Backend`] when the file cannot be opened or created (for example, its
    /// parent directory does not exist), or [`Error::Migrate`] when a migration fails to apply.
    pub async fn open(path: &Path) -> Result<Self, Error> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new().connect_with(options).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }
}

impl BillStore for SqliteStore {
    fn insert<'a>(
        &'a self,
        hash: &'a RawHash,
        bill: &'a Bill,
    ) -> BoxFuture<'a, Result<InsertOutcome, Error>> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await?;

            let by_hash = sqlx::query("SELECT id FROM bills WHERE hash = ?")
                .bind(hash.as_bytes().as_slice())
                .fetch_optional(&mut *tx)
                .await?;
            if let Some(row) = by_hash {
                let id_raw: String = row.try_get("id")?;
                return Ok(InsertOutcome::Duplicate(decode_id(&id_raw)?));
            }

            let by_id = sqlx::query("SELECT id FROM bills WHERE id = ?")
                .bind(bill.id().as_str())
                .fetch_optional(&mut *tx)
                .await?;
            if by_id.is_some() {
                return Err(Error::DuplicateId(bill.id().clone()));
            }

            let (currency, amount_minor) = match bill.amount() {
                Some(money) => (
                    Some(money.currency().as_str().to_owned()),
                    Some(money.minor_units()),
                ),
                None => (None, None),
            };
            let (period_start, period_end) = match bill.period() {
                Some(period) => (Some(period.start()), Some(period.end())),
                None => (None, None),
            };
            let status = match bill.status() {
                Status::Extracted => "extracted",
                Status::NeedsReview => "needs_review",
            };

            sqlx::query(
                "INSERT INTO bills \
                 (id, hash, vendor, amount_minor, currency, period_start, period_end, due, \
                 status, inserted_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(bill.id().as_str())
            .bind(hash.as_bytes().as_slice())
            .bind(bill.vendor().map(Vendor::name))
            .bind(amount_minor)
            .bind(currency)
            .bind(period_start)
            .bind(period_end)
            .bind(bill.due())
            .bind(status)
            .bind(OffsetDateTime::now_utc())
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok(InsertOutcome::Inserted(bill.id().clone()))
        })
    }

    fn get<'a>(&'a self, id: &'a BillId) -> BoxFuture<'a, Result<Option<Bill>, Error>> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT id, vendor, amount_minor, currency, period_start, period_end, due, \
                 status FROM bills WHERE id = ?",
            )
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await?;
            row.as_ref().map(row_to_bill).transpose()
        })
    }

    fn find_by_hash<'a>(&'a self, hash: &'a RawHash) -> BoxFuture<'a, Result<Option<Bill>, Error>> {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT id, vendor, amount_minor, currency, period_start, period_end, due, \
                 status FROM bills WHERE hash = ?",
            )
            .bind(hash.as_bytes().as_slice())
            .fetch_optional(&self.pool)
            .await?;
            row.as_ref().map(row_to_bill).transpose()
        })
    }

    fn list<'a>(&'a self) -> BoxFuture<'a, Result<Vec<Bill>, Error>> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT id, vendor, amount_minor, currency, period_start, period_end, due, \
                 status FROM bills ORDER BY rowid ASC",
            )
            .fetch_all(&self.pool)
            .await?;
            rows.iter().map(row_to_bill).collect()
        })
    }
}

/// An in-memory [`BillStore`], the fake other modules use at this system edge. Same
/// semantics as [`SqliteStore`] over a `std::sync::Mutex`; a poisoned lock is recovered, never
/// panics.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    rows: Mutex<Vec<(RawHash, Bill)>>,
}

impl InMemoryStore {
    /// Creates an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl BillStore for InMemoryStore {
    fn insert<'a>(
        &'a self,
        hash: &'a RawHash,
        bill: &'a Bill,
    ) -> BoxFuture<'a, Result<InsertOutcome, Error>> {
        Box::pin(async move {
            let mut rows = self.rows.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some((_, existing)) = rows.iter().find(|(h, _)| h == hash) {
                return Ok(InsertOutcome::Duplicate(existing.id().clone()));
            }
            if rows.iter().any(|(_, b)| b.id() == bill.id()) {
                return Err(Error::DuplicateId(bill.id().clone()));
            }
            rows.push((*hash, bill.clone()));
            Ok(InsertOutcome::Inserted(bill.id().clone()))
        })
    }

    fn get<'a>(&'a self, id: &'a BillId) -> BoxFuture<'a, Result<Option<Bill>, Error>> {
        Box::pin(async move {
            let rows = self.rows.lock().unwrap_or_else(PoisonError::into_inner);
            Ok(rows
                .iter()
                .find(|(_, b)| b.id() == id)
                .map(|(_, b)| b.clone()))
        })
    }

    fn find_by_hash<'a>(&'a self, hash: &'a RawHash) -> BoxFuture<'a, Result<Option<Bill>, Error>> {
        Box::pin(async move {
            let rows = self.rows.lock().unwrap_or_else(PoisonError::into_inner);
            Ok(rows.iter().find(|(h, _)| h == hash).map(|(_, b)| b.clone()))
        })
    }

    fn list<'a>(&'a self) -> BoxFuture<'a, Result<Vec<Bill>, Error>> {
        Box::pin(async move {
            let rows = self.rows.lock().unwrap_or_else(PoisonError::into_inner);
            Ok(rows.iter().map(|(_, b)| b.clone()).collect())
        })
    }
}
