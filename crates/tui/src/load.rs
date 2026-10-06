//! Opens the DB read-only and builds the `App`.

use std::path::Path;

use anyhow::Context;
use hauz_core::store::{BillStore, SqliteStore};

use crate::App;

/// `open_read_only` + `list` + `App::new`. Fails on a missing, unmigrated or corrupt DB.
///
/// # Errors
/// The store error, with the DB path as context.
pub async fn load(db: &Path) -> anyhow::Result<App> {
    let store = SqliteStore::open_read_only(db)
        .await
        .with_context(|| format!("opening {}", db.display()))?;
    let bills = store
        .list()
        .await
        .with_context(|| format!("reading {}", db.display()))?;
    Ok(App::new(bills))
}
