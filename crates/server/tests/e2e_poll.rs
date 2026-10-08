//! [e2e] acceptance tests for the background Gmail poll (feature #43): a `Poller` over a fake
//! `MailSource` (the network edge) into a real `SqliteStore`.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{Result, TOKEN, app_with_store, tmp_db_path};
use hauz_core::BoxFuture;
use hauz_core::extract::{Chain, Extractor, PdfTextExtractor, TextExtractor};
use hauz_core::ingest::Outcome;
use hauz_core::mail::{Config, Error, MailSource, MessageId, Page, PageToken};
use hauz_core::store::{BillStore, SqliteStore};
use hauz_server::Poller;
use time::macros::datetime;
use tower::ServiceExt;

const BILL_EML: &[u8] = include_bytes!("fixtures/bill.eml");
const MALFORMED_EML: &[u8] = include_bytes!("fixtures/malformed.eml");

/// Lists `messages` (id, raw) in one page; the first `fail_lists` list calls fail.
struct FakeSource {
    messages: Vec<(&'static str, &'static [u8])>,
    fail_lists: Mutex<usize>,
    queries: Mutex<Vec<String>>,
}

impl FakeSource {
    fn new(messages: Vec<(&'static str, &'static [u8])>, fail_lists: usize) -> Arc<Self> {
        Arc::new(Self {
            messages,
            fail_lists: Mutex::new(fail_lists),
            queries: Mutex::new(Vec::new()),
        })
    }

    fn queries(&self) -> Vec<String> {
        self.queries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl MailSource for FakeSource {
    fn list<'a>(
        &'a self,
        query: &'a str,
        _page: Option<&'a PageToken>,
    ) -> BoxFuture<'a, core::result::Result<Page, Error>> {
        Box::pin(async move {
            self.queries
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(query.to_owned());
            let mut fails = self
                .fail_lists
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if *fails > 0 {
                *fails -= 1;
                return Err(Error::Transport {
                    reason: "gmail answered 503".to_owned(),
                });
            }
            let ids = self
                .messages
                .iter()
                .map(|(id, _)| MessageId::new(*id))
                .collect::<core::result::Result<Vec<_>, _>>()?;
            Ok(Page { ids, next: None })
        })
    }

    fn fetch_raw<'a>(
        &'a self,
        id: &'a MessageId,
    ) -> BoxFuture<'a, core::result::Result<Vec<u8>, Error>> {
        Box::pin(async move {
            self.messages
                .iter()
                .find(|(m, _)| *m == id.as_str())
                .map(|(_, raw)| raw.to_vec())
                .ok_or_else(|| Error::Malformed {
                    reason: "unknown id".to_owned(),
                })
        })
    }
}

fn config() -> Result<Config> {
    let env = |key: &str| {
        match key {
            "HAUZ_GMAIL_CLIENT_ID" => Some("cid"),
            "HAUZ_GMAIL_CLIENT_SECRET" => Some("secret"),
            "HAUZ_GMAIL_REFRESH_TOKEN" => Some("refresh"),
            "HAUZ_GMAIL_LABEL" => Some("bills"),
            _ => None,
        }
        .map(str::to_owned)
    };
    Ok(Config::from_env(env)?.ok_or("configured")?)
}

fn extractor() -> Arc<dyn Extractor> {
    Arc::new(Chain::new(vec![
        Box::new(TextExtractor),
        Box::new(PdfTextExtractor),
    ]))
}

async fn store() -> Result<Arc<SqliteStore>> {
    Ok(Arc::new(SqliteStore::open(&tmp_db_path()).await?))
}

/// Waits (bounded) until `done` holds.
async fn wait_for<F: Fn() -> bool>(done: F) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .map_err(|_| "timed out".into())
}

fn lines_with(lines: &Arc<Mutex<Vec<String>>>, prefix: &str) -> Vec<String> {
    lines
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|l| l.starts_with(prefix))
        .cloned()
        .collect()
}

#[tokio::test]
async fn ac4_tick_ingests_listed_mail() -> Result<()> {
    let source = FakeSource::new(vec![("m1", BILL_EML)], 0);
    let store = store().await?;
    let config = config()?;
    let now = datetime!(2026-10-07 12:00:00 UTC);
    let expected_query = config.poll_query(now);
    let poller = Poller::new(source.clone(), config, extractor(), store.clone());

    let first = poller.tick(now).await?;
    assert_eq!(first.len(), 1);
    let id = match &first[0].outcome {
        Ok(Outcome::Created(id)) => id.clone(),
        other => return Err(format!("expected Created, got {other:?}").into()),
    };
    assert!(store.get(&id).await?.is_some());
    assert_eq!(source.queries(), vec![expected_query]);

    let second = poller.tick(now).await?;
    assert_eq!(second.len(), 1);
    assert!(matches!(&second[0].outcome, Ok(Outcome::Duplicate(d)) if *d == id));
    Ok(())
}

#[tokio::test]
async fn ac5_failed_tick_is_logged_and_retried() -> Result<()> {
    let source = FakeSource::new(vec![("m1", BILL_EML)], 1);
    let store = store().await?;
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = lines.clone();
    let handle = Poller::new(source, config()?, extractor(), store.clone()).spawn(
        Duration::from_millis(10),
        move |line| {
            sink.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(line);
        },
    );

    wait_for(|| !lines_with(&lines, "gmail poll: 1 messages").is_empty()).await?;
    assert_eq!(store.list().await?.len(), 1);

    let failed = lines_with(&lines, "gmail poll failed:");
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert!(failed[0].contains("transport error: gmail answered 503"));
    assert!(!handle.is_finished());
    handle.abort();
    Ok(())
}

#[tokio::test]
async fn ac6_per_message_failure_is_logged_and_server_serves() -> Result<()> {
    let source = FakeSource::new(vec![("good", BILL_EML), ("bad", MALFORMED_EML)], 0);
    let store = store().await?;
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = lines.clone();
    let handle = Poller::new(source, config()?, extractor(), store.clone()).spawn(
        Duration::from_millis(10),
        move |line| {
            sink.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(line);
        },
    );
    wait_for(|| !lines_with(&lines, "gmail poll: failed bad:").is_empty()).await?;
    handle.abort();

    let bills = store.list().await?;
    assert_eq!(bills.len(), 1);
    let id = bills[0].id().as_str().to_owned();
    let response = app_with_store(store.clone())
        .oneshot(
            Request::get(format!("/v1/bills/{id}"))
                .header("authorization", format!("Bearer {TOKEN}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}
