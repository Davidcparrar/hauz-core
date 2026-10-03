//! Shared fixtures for `cli`'s [e2e] tests. Each `tests/*.rs` file compiles this module
//! separately, so a helper unused by one file is still valid in the other.
#![allow(dead_code)] // not every fixture is used by every test file

use std::path::{Path, PathBuf};

use hauz_core::store::{BillStore, SqliteStore};

pub(crate) type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

/// A fresh, unique directory under `std::env::temp_dir()` for one test's process `cwd` and
/// `--db` path, so parallel tests never collide.
pub(crate) fn tmp_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!("hauz-cli-{nanos}"));
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
