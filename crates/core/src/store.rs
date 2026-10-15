//! Persistence for `Bill`s: the `BillStore` trait (the system edge other modules depend on),
//! a `SqliteStore` backed by sqlx with embedded migrations, and an `InMemoryStore` fake with
//! identical semantics. Insert is idempotent on the raw-message hash the caller computes: the
//! same email twice yields one row and the same id.

use std::path::Path;
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
    /// `replace` found no row stored under this id.
    #[error("no stored bill with id {0:?}")]
    NotFound(BillId),
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

/// How many bills one [`BillStore::list_page`] call returns at most: 1..=200, default 50.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSize(u32);

/// A page size outside `1..=200`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("page size {0} is outside 1..=200")]
pub struct InvalidPageSize(pub u32);

impl PageSize {
    /// The page size used when the caller names none.
    pub const DEFAULT: Self = Self(50);

    /// Parse, don't validate: accepts `1..=200`.
    ///
    /// # Errors
    /// Returns [`InvalidPageSize`] for 0 or anything above 200.
    pub fn new(raw: u32) -> Result<Self, InvalidPageSize> {
        if (1..=200).contains(&raw) {
            Ok(Self(raw))
        } else {
            Err(InvalidPageSize(raw))
        }
    }

    /// The page size as a number.
    #[must_use]
    pub fn get(self) -> u32 {
        self.0
    }
}

/// What [`BillStore::list_page`] returns: newest bills first, optionally one status, one page
/// at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListQuery {
    /// Only bills with this status; `None` lists every status.
    pub status: Option<Status>,
    /// The most bills to return.
    pub limit: PageSize,
    /// Only bills inserted before this one (a cursor from [`BillPage::next`]); `None` starts
    /// at the newest.
    pub before: Option<BillId>,
}

/// One page of bills, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillPage {
    /// The bills of this page.
    pub bills: Vec<Bill>,
    /// The last returned bill's id when more matching bills exist; pass it as
    /// [`ListQuery::before`] for the next page.
    pub next: Option<BillId>,
}

impl BillPage {
    /// Builds a page from up to `limit + 1` matching bills, newest first: the extra one only
    /// proves more exist and is dropped.
    fn from_overfetched(mut bills: Vec<Bill>, limit: PageSize) -> Self {
        let limit = limit.get() as usize;
        let next = if bills.len() > limit {
            bills.truncate(limit);
            bills.last().map(|bill| bill.id().clone())
        } else {
            None
        };
        Self { bills, next }
    }
}

/// Moved to the crate root as [`crate::BoxFuture`]; re-exported here so existing paths
/// (`store::BoxFuture`) keep compiling.
pub use crate::BoxFuture;

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

    /// Lists one page of bills, newest first, filtered by `query.status` and starting after
    /// `query.before`. A `before` that is not stored yields `Error::NotFound(before)`.
    fn list_page<'a>(&'a self, query: &'a ListQuery) -> BoxFuture<'a, Result<BillPage, Error>>;

    /// Overwrites vendor, amount, period, issued, due and status of the row stored under
    /// `bill.id()`; never touches the row's id, hash, insertion time or list position.
    /// No such row yields `Error::NotFound(id)` and writes nothing.
    fn replace<'a>(&'a self, bill: &'a Bill) -> BoxFuture<'a, Result<(), Error>>;
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

/// Rebuilds a `BillDraft` from already-fetched column values and an already-decoded id;
/// each failure carries a human-readable reason.
#[allow(clippy::too_many_arguments)] // one arg per stored column, all needed to rebuild a Bill
fn build_draft(
    id: BillId,
    vendor: Option<String>,
    amount_minor: Option<i64>,
    currency: Option<String>,
    period_start: Option<time::Date>,
    period_end: Option<time::Date>,
    issued: Option<time::Date>,
    due: Option<time::Date>,
    status_raw: &str,
) -> Result<BillDraft, String> {
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
        issued,
        due,
        status,
    })
}

/// Rebuilds a `Bill` from a `SELECT id, vendor, amount_minor, currency, period_start,
/// period_end, issued, due, status` row.
fn row_to_bill(row: &SqliteRow) -> Result<Bill, Error> {
    let id_raw: String = row.try_get("id")?;
    let vendor: Option<String> = row.try_get("vendor")?;
    let amount_minor: Option<i64> = row.try_get("amount_minor")?;
    let currency: Option<String> = row.try_get("currency")?;
    let period_start: Option<time::Date> = row.try_get("period_start")?;
    let period_end: Option<time::Date> = row.try_get("period_end")?;
    let issued: Option<time::Date> = row.try_get("issued")?;
    let due: Option<time::Date> = row.try_get("due")?;
    let status_raw: String = row.try_get("status")?;

    let to_corrupt = |reason: String| Error::Corrupt {
        id: id_raw.clone(),
        reason,
    };
    let draft = build_draft(
        decode_id(&id_raw)?,
        vendor,
        amount_minor,
        currency,
        period_start,
        period_end,
        issued,
        due,
        &status_raw,
    )
    .map_err(to_corrupt)?;
    Bill::try_from(draft).map_err(|source| to_corrupt(source.to_string()))
}

/// The columns of `bill` that `insert` and `replace` write, in the order
/// `(currency, amount_minor, period_start, period_end, status)`.
fn encode_columns(
    bill: &Bill,
) -> (
    Option<String>,
    Option<i64>,
    Option<time::Date>,
    Option<time::Date>,
    &'static str,
) {
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
    (
        currency,
        amount_minor,
        period_start,
        period_end,
        encode_status(bill.status()),
    )
}

/// The `status` column text for `status`; the inverse of [`decode_status`].
fn encode_status(status: Status) -> &'static str {
    match status {
        Status::Extracted => "extracted",
        Status::NeedsReview => "needs_review",
    }
}

/// How long a pooled connection waits for the SQLite write lock before giving up.
const WRITE_LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

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
            .journal_mode(SqliteJournalMode::Wal)
            // `BEGIN IMMEDIATE` takes the write lock up front, which is the one place
            // SQLite honors the busy handler; pinned here so contention between pooled
            // writers waits instead of surfacing as `Error::Backend` (SQLITE_BUSY).
            .busy_timeout(WRITE_LOCK_WAIT);
        let pool = SqlitePoolOptions::new().connect_with(options).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    /// Opens an existing SQLite file read-only: never creates it, never changes the journal
    /// mode, never migrates. Safe beside a live writer on a WAL database. `insert` through
    /// this handle fails with [`Error::Backend`].
    ///
    /// # Errors
    /// Returns [`Error::Backend`] when the file does not exist or cannot be opened.
    pub async fn open_read_only(path: &Path) -> Result<Self, Error> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .read_only(true)
            .busy_timeout(WRITE_LOCK_WAIT);
        let pool = SqlitePoolOptions::new().connect_with(options).await?;
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

            let (currency, amount_minor, period_start, period_end, status) = encode_columns(bill);

            sqlx::query(
                "INSERT INTO bills \
                 (id, hash, vendor, amount_minor, currency, period_start, period_end, issued, due, \
                 status, inserted_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(bill.id().as_str())
            .bind(hash.as_bytes().as_slice())
            .bind(bill.vendor().map(Vendor::name))
            .bind(amount_minor)
            .bind(currency)
            .bind(period_start)
            .bind(period_end)
            .bind(bill.issued())
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
                "SELECT id, vendor, amount_minor, currency, period_start, period_end, issued, due, \
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
                "SELECT id, vendor, amount_minor, currency, period_start, period_end, issued, due, \
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
                "SELECT id, vendor, amount_minor, currency, period_start, period_end, issued, due, \
                 status FROM bills ORDER BY rowid ASC",
            )
            .fetch_all(&self.pool)
            .await?;
            rows.iter().map(row_to_bill).collect()
        })
    }

    fn list_page<'a>(&'a self, query: &'a ListQuery) -> BoxFuture<'a, Result<BillPage, Error>> {
        Box::pin(async move {
            let cursor = match &query.before {
                Some(before) => {
                    let row = sqlx::query("SELECT rowid FROM bills WHERE id = ?")
                        .bind(before.as_str())
                        .fetch_optional(&self.pool)
                        .await?;
                    let row = row.ok_or_else(|| Error::NotFound(before.clone()))?;
                    Some(row.try_get::<i64, _>("rowid")?)
                }
                None => None,
            };
            let rows = sqlx::query(
                "SELECT id, vendor, amount_minor, currency, period_start, period_end, issued, due, \
                 status FROM bills WHERE (?1 IS NULL OR rowid < ?1) \
                 AND (?2 IS NULL OR status = ?2) ORDER BY rowid DESC LIMIT ?3",
            )
            .bind(cursor)
            .bind(query.status.map(encode_status))
            .bind(i64::from(query.limit.get()) + 1)
            .fetch_all(&self.pool)
            .await?;
            let bills = rows.iter().map(row_to_bill).collect::<Result<_, _>>()?;
            Ok(BillPage::from_overfetched(bills, query.limit))
        })
    }

    fn replace<'a>(&'a self, bill: &'a Bill) -> BoxFuture<'a, Result<(), Error>> {
        Box::pin(async move {
            let (currency, amount_minor, period_start, period_end, status) = encode_columns(bill);
            let result = sqlx::query(
                "UPDATE bills SET vendor = ?, amount_minor = ?, currency = ?, period_start = ?, \
                 period_end = ?, issued = ?, due = ?, status = ? WHERE id = ?",
            )
            .bind(bill.vendor().map(Vendor::name))
            .bind(amount_minor)
            .bind(currency)
            .bind(period_start)
            .bind(period_end)
            .bind(bill.issued())
            .bind(bill.due())
            .bind(status)
            .bind(bill.id().as_str())
            .execute(&self.pool)
            .await?;
            if result.rows_affected() == 0 {
                return Err(Error::NotFound(bill.id().clone()));
            }
            Ok(())
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

    fn list_page<'a>(&'a self, query: &'a ListQuery) -> BoxFuture<'a, Result<BillPage, Error>> {
        Box::pin(async move {
            let rows = self.rows.lock().unwrap_or_else(PoisonError::into_inner);
            let end = match &query.before {
                Some(before) => rows
                    .iter()
                    .position(|(_, b)| b.id() == before)
                    .ok_or_else(|| Error::NotFound(before.clone()))?,
                None => rows.len(),
            };
            let bills = rows
                .iter()
                .take(end)
                .rev()
                .map(|(_, b)| b)
                .filter(|b| query.status.is_none_or(|status| b.status() == status))
                .take(query.limit.get() as usize + 1)
                .cloned()
                .collect();
            Ok(BillPage::from_overfetched(bills, query.limit))
        })
    }

    fn replace<'a>(&'a self, bill: &'a Bill) -> BoxFuture<'a, Result<(), Error>> {
        Box::pin(async move {
            let mut rows = self.rows.lock().unwrap_or_else(PoisonError::into_inner);
            match rows.iter_mut().find(|(_, b)| b.id() == bill.id()) {
                Some((_, slot)) => {
                    *slot = bill.clone();
                    Ok(())
                }
                None => Err(Error::NotFound(bill.id().clone())),
            }
        })
    }
}
