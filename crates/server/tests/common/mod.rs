//! Shared fixtures for `server`'s [e2e] tests. Each `tests/*.rs` file compiles this module
//! separately, so a helper unused by one file is still valid in the other.
#![allow(dead_code)] // not every fixture is used by every test file

use std::sync::Arc;

use axum::Router;
use hauz_core::bill::{Bill, BillId};
use hauz_core::extract::{Chain, Extractor, PdfTextExtractor, TextExtractor, XmlInvoiceExtractor};
use hauz_core::store::{
    BillStore, BoxFuture, Error as StoreError, InsertOutcome, RawHash, SqliteStore,
};
use hauz_server::{ApiToken, AppState};

pub(crate) type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

/// The bearer token every fixture `AppState` is configured with.
pub(crate) const TOKEN: &str = "fixture-token-0123456789abcdef";

/// [`TOKEN`] as the `ApiToken` the server state needs.
#[allow(clippy::expect_used)] // the fixture constant is non-blank; a test-setup failure should abort
fn api_token() -> ApiToken {
    ApiToken::new(TOKEN).expect("fixture token is non-blank")
}

/// A unique tmp-file path for a `SqliteStore` under test, so parallel tests never collide.
pub(crate) fn tmp_db_path() -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("hauz-server-{nanos}.sqlite3"))
}

/// A `Router` over a fresh `SqliteStore` and `Chain([TextExtractor, PdfTextExtractor])`, plus
/// a handle to that same store for tests to inspect (`store.list()`, etc.).
pub(crate) async fn app() -> Result<(Router, Arc<SqliteStore>)> {
    let store = Arc::new(SqliteStore::open(&tmp_db_path()).await?);
    let extractor: Arc<dyn Extractor> = Arc::new(Chain::new(vec![
        Box::new(TextExtractor),
        Box::new(PdfTextExtractor),
    ]));
    let state = AppState::new(store.clone(), extractor, api_token());
    Ok((hauz_server::router(state), store))
}

/// Like [`app`], but over the #27 chain `Chain([XmlInvoiceExtractor, TextExtractor,
/// PdfTextExtractor])` that reads DIAN zip attachments.
pub(crate) async fn app_with_xml() -> Result<(Router, Arc<SqliteStore>)> {
    let store = Arc::new(SqliteStore::open(&tmp_db_path()).await?);
    let extractor: Arc<dyn Extractor> = Arc::new(Chain::new(vec![
        Box::new(XmlInvoiceExtractor),
        Box::new(TextExtractor),
        Box::new(PdfTextExtractor),
    ]));
    let state = AppState::new(store.clone(), extractor, api_token());
    Ok((hauz_server::router(state), store))
}

/// A `Router` over `store` (e.g. a [`FailingStore`]) and `Chain([TextExtractor,
/// PdfTextExtractor])`.
pub(crate) fn app_with_store(store: Arc<dyn BillStore>) -> Router {
    let extractor: Arc<dyn Extractor> = Arc::new(Chain::new(vec![
        Box::new(TextExtractor),
        Box::new(PdfTextExtractor),
    ]));
    hauz_server::router(AppState::new(store, extractor, api_token()))
}

/// A `BillStore` every method fails on: the legitimate fake for exercising the 500 path, since
/// storage is a true system edge behind the injected `BillStore` trait.
#[derive(Debug, Default)]
pub(crate) struct FailingStore;

fn boom() -> StoreError {
    StoreError::Corrupt {
        id: "boom".to_string(),
        reason: "boom".to_string(),
    }
}

impl BillStore for FailingStore {
    fn insert<'a>(
        &'a self,
        _hash: &'a RawHash,
        _bill: &'a Bill,
    ) -> BoxFuture<'a, core::result::Result<InsertOutcome, StoreError>> {
        Box::pin(async { Err(boom()) })
    }

    fn get<'a>(
        &'a self,
        _id: &'a BillId,
    ) -> BoxFuture<'a, core::result::Result<Option<Bill>, StoreError>> {
        Box::pin(async { Err(boom()) })
    }

    fn find_by_hash<'a>(
        &'a self,
        _hash: &'a RawHash,
    ) -> BoxFuture<'a, core::result::Result<Option<Bill>, StoreError>> {
        Box::pin(async { Err(boom()) })
    }

    fn list<'a>(&'a self) -> BoxFuture<'a, core::result::Result<Vec<Bill>, StoreError>> {
        Box::pin(async { Err(boom()) })
    }
}
