//! Thin binary shell: reads config from the environment, opens the store, builds the
//! extractor chain, and serves. Untested by design (constitution › Shape); all behavior lives
//! in `hauz_server::router` and `hauz-core`.

use std::env;
use std::path::Path;
use std::sync::Arc;

use hauz_core::extract::{Chain, Extractor, PdfTextExtractor, TextExtractor};
use hauz_core::store::SqliteStore;
use hauz_server::{AppState, router};

/// Defaulted when `BIND_ADDR` is unset.
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let database_url = env::var("DATABASE_URL")?;
    let db_path = database_url
        .strip_prefix("sqlite://")
        .unwrap_or(database_url.as_str());
    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string());

    let store = SqliteStore::open(Path::new(db_path)).await?;
    let extractor: Arc<dyn Extractor> = Arc::new(Chain::new(vec![
        Box::new(TextExtractor),
        Box::new(PdfTextExtractor),
    ]));
    let state = AppState::new(Arc::new(store), extractor);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, router(state)).await?;
    Ok(())
}
