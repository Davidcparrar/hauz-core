//! Shared fixtures for `cli`'s [e2e] tests. Each `tests/*.rs` file compiles this module
//! separately, so a helper unused by one file is still valid in the other.
#![allow(dead_code)] // not every fixture is used by every test file

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hauz_core::store::{BillStore, SqliteStore};

pub(crate) type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

/// A fresh, unique directory under `std::env::temp_dir()` for one test's process `cwd` and
/// `--db` path, so parallel tests (across processes and threads) never collide. The name
/// mixes the process id, the current time in nanoseconds, and a per-process counter, since
/// time alone can repeat across threads scheduled close together.
pub(crate) fn tmp_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("hauz-cli-{pid}-{nanos}-{n}"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// The path to a fixture under `tests/fixtures/`.
pub(crate) fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Every bill id stored in the `SqliteStore` at `db`, insertion order.
pub(crate) async fn ids(db: &Path) -> Result<Vec<String>> {
    let store = SqliteStore::open(db).await?;
    let bills = store.list().await?;
    Ok(bills
        .iter()
        .map(|bill| bill.id().as_str().to_owned())
        .collect())
}

/// The lowercase hex of `raw_hash(raw)` — the bill id `hauz ingest` prints and stores.
pub(crate) fn hash_hex(raw: &[u8]) -> String {
    hauz_core::ingest::raw_hash(raw)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Stores a bare `NeedsReview` bill (id = hash hex, every optional field `None`) in the db at
/// `db` under `raw`'s hash: the row an older, weaker extractor would have left.
pub(crate) async fn seed_bare_bill(db: &Path, raw: &[u8]) -> Result<()> {
    use hauz_core::bill::{Bill, BillDraft, BillId, Status};
    let draft = BillDraft {
        id: BillId::new(&hash_hex(raw))?,
        vendor: None,
        amount: None,
        period: None,
        issued: None,
        due: None,
        status: Status::NeedsReview,
    };
    let store = SqliteStore::open(db).await?;
    store
        .insert(&hauz_core::ingest::raw_hash(raw), &Bill::try_from(draft)?)
        .await?;
    Ok(())
}
