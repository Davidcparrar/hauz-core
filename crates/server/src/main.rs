//! Thin binary shell: reads config from the environment, opens the store, builds the
//! extractor chain (escalating to an `LlmExtractor` when `HAUZ_LLM_PROVIDER` is set), and
//! serves. Untested by design (constitution › Shape); all behavior lives in
//! `hauz_server::router` and `hauz-core`.

use std::env;
use std::path::Path;
use std::sync::Arc;

use hauz_core::extract::{
    Chain, Escalate, Extractor, PdfTextExtractor, TextExtractor, XmlInvoiceExtractor,
};
use hauz_core::ingest::EXTRACTED_MIN_CONFIDENCE;
use hauz_core::llm::{Config, LlmExtractor, LlmOptions, Pdftoppm, RigClient};
use hauz_core::store::SqliteStore;
use hauz_server::{AppState, router};

/// Defaulted when `BIND_ADDR` is unset.
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";

/// `None` (no `HAUZ_LLM_PROVIDER`) is today's `Chain([XmlInvoiceExtractor, TextExtractor,
/// PdfTextExtractor])`; `Some(config)` wraps it in `Escalate` with an `LlmExtractor` as the
/// secondary.
fn build_extractor(config: Option<Config>) -> Box<dyn Extractor> {
    let chain = Chain::new(vec![
        Box::new(XmlInvoiceExtractor),
        Box::new(TextExtractor),
        Box::new(PdfTextExtractor),
    ]);
    let Some(config) = config else {
        return Box::new(chain);
    };
    let llm_extractor = LlmExtractor::new(
        Box::new(RigClient::new(config.provider, &config.model)),
        Box::new(Pdftoppm::new(150)),
        LlmOptions::default(),
    );
    Box::new(Escalate::new(
        Box::new(chain),
        Box::new(llm_extractor),
        EXTRACTED_MIN_CONFIDENCE,
    ))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let database_url = env::var("DATABASE_URL")?;
    let db_path = database_url
        .strip_prefix("sqlite://")
        .unwrap_or(database_url.as_str());
    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string());
    let config = Config::from_env(|key| env::var(key).ok())?;

    let store = SqliteStore::open(Path::new(db_path)).await?;
    let extractor: Arc<dyn Extractor> = Arc::from(build_extractor(config));
    let state = AppState::new(Arc::new(store), extractor);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, router(state)).await?;
    Ok(())
}
