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
    /// A stored row did not reconstruct into a valid `Bill`: an id that fails
    /// [`BillId::new`], an unknown `status` string, or any column whose text fails its
    /// validated type's constructor.
    #[error("stored row {id:?} is corrupt: {reason}")]
    Corrupt {
        /// The raw stored `id` column text (may itself be invalid, so it is not a
        /// [`BillId`]).
        id: String,
        /// Human-readable reason the row's columns did not reconstruct a valid `Bill`.
        reason: String,
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
    BillId::new(raw).map_err(|source| Error::Corrupt {
        id: raw.to_owned(),
        reason: source.to_string(),
    })
}

/// Parses the `status` column. Unlike the other columns this has no validated type of its
/// own to delegate to: an unrecognized value is corrupt data, never silently coerced to
/// [`Status::NeedsReview`].
fn decode_status(raw: &str) -> Result<Status, String> {
    match raw {
        "extracted" => Ok(Status::Extracted),
        "needs_review" => Ok(Status::NeedsReview),
        other => Err(format!("unknown status {other:?}")),
    }
}

/// Rebuilds a `BillDraft` from already-fetched column values, without allocating a
/// [`BillId`] until the id itself is known to be valid; each failure carries a
/// human-readable reason.
#[allow(clippy::too_many_arguments)] // one arg per stored column, all needed to rebuild a Bill
fn build_draft(
    id_raw: &str,
    vendor: Option<String>,
    amount_minor: Option<i64>,
    currency: Option<String>,
    period_start: Option<time::Date>,
    period_end: Option<time::Date>,
    due: Option<time::Date>,
    status_raw: &str,
) -> Result<BillDraft, String> {
    let id = BillId::new(id_raw).map_err(|e| e.to_string())?;
    let status = decode_status(status_raw)?;
    let vendor = vendor
        .map(|v| Vendor::new(&v))
        .transpose()
        .map_err(|e| e.to_string())?;
    let amount = match (amount_minor, currency) {
        (Some(minor_units), Some(code)) => Some(Money::new(
            minor_units,
            Currency::new(&code).map_err(|e| e.to_string())?,
        )),
        _ => None,
    };
    let period = match (period_start, period_end) {
        (Some(start), Some(end)) => {
            Some(BillingPeriod::new(start, end).map_err(|e| e.to_string())?)
        }
        _ => None,
    };
    Ok(BillDraft {
        id,
        vendor,
        amount,
        period,
        due,
        status,
    })
}

/// Rebuilds a `Bill` from a `SELECT id, vendor, amount_minor, currency, period_start,
/// period_end, due, status` row.
fn row_to_bill(row: &SqliteRow) -> Result<Bill, Error> {
    let id_raw: String = row.try_get("id")?;
    let vendor: Option<String> = row.try_get("vendor")?;
    let amount_minor: Option<i64> = row.try_get("amount_minor")?;
    let currency: Option<String> = row.try_get("currency")?;
    let period_start: Option<time::Date> = row.try_get("period_start")?;
    let period_end: Option<time::Date> = row.try_get("period_end")?;
    let due: Option<time::Date> = row.try_get("due")?;
    let status_raw: String = row.try_get("status")?;

    let to_corrupt = |reason: String| Error::Corrupt {
        id: id_raw.clone(),
        reason,
    };
    let draft = build_draft(
        &id_raw,
        vendor,
        amount_minor,
        currency,
        period_start,
        period_end,
        due,
        &status_raw,
    )
    .map_err(to_corrupt)?;
    Bill::try_from(draft).map_err(|source| to_corrupt(source.to_string()))
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
            // Hash check, id check, and the write all hold the write lock together: without
            // `IMMEDIATE`, sqlite's default deferred transaction only takes the write lock on
            // the first write, letting two concurrent inserts both pass their checks.
            let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;

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
