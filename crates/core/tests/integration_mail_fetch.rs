//! [integration] acceptance tests for `mail::fetch` (feature #42) over a fake `MailSource`,
//! the real `TextExtractor` and `InMemoryStore`.

use std::collections::HashMap;

use hauz_core::BoxFuture;
use hauz_core::extract::TextExtractor;
use hauz_core::ingest::{Error as IngestError, Outcome};
use hauz_core::mail::{Error, MailSource, MessageId, Page, PageToken, fetch};
use hauz_core::store::{BillStore, InMemoryStore};

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

const BILL_A: &[u8] = b"From: billing@acme-power.example\r\nSubject: A\r\nDate: Mon, 1 Jan 2024 12:00:00 +0000\r\nContent-Type: text/plain\r\n\r\nTotal: 1,234.56 EUR\r\nDue date: 15/10/2026\r\n";
const BILL_B: &[u8] = b"From: billing@water.example\r\nSubject: B\r\nDate: Tue, 2 Jan 2024 12:00:00 +0000\r\nContent-Type: text/plain\r\n\r\nTotal: 20.00 EUR\r\nDue date: 16/10/2026\r\n";
const NO_SENDER: &[u8] = b"Subject: No sender\r\nDate: Mon, 1 Jan 2024 12:00:00 +0000\r\nContent-Type: text/plain\r\n\r\nBody text\r\n";

/// Two pages: `m1`, `m2` then `m3`. A message missing from `raw` fails to download.
struct FakeSource {
    raw: HashMap<&'static str, &'static [u8]>,
}

impl MailSource for FakeSource {
    fn list<'a>(
        &'a self,
        _query: &'a str,
        page: Option<&'a PageToken>,
    ) -> BoxFuture<'a, core::result::Result<Page, Error>> {
        Box::pin(async move {
            let (ids, next): (&[&str], _) = match page.map(PageToken::as_str) {
                None => (&["m1", "m2"], Some(PageToken::new("p2"))),
                Some(_) => (&["m3"], None),
            };
            let ids = ids
                .iter()
                .map(|id| MessageId::new(*id))
                .collect::<core::result::Result<_, _>>()?;
            Ok(Page { ids, next })
        })
    }

    fn fetch_raw<'a>(
        &'a self,
        id: &'a MessageId,
    ) -> BoxFuture<'a, core::result::Result<Vec<u8>, Error>> {
        Box::pin(async move {
            self.raw
                .get(id.as_str())
                .map(|bytes| bytes.to_vec())
                .ok_or_else(|| Error::Transport {
                    reason: format!("no such message {}", id.as_str()),
                })
        })
    }
}

#[tokio::test]
async fn ac5_ingests_listing_in_order_and_reruns_as_duplicates() -> Result<()> {
    let source = FakeSource {
        raw: HashMap::from([("m1", BILL_A), ("m2", BILL_B), ("m3", NO_SENDER)]),
    };
    let store = InMemoryStore::new();

    let first = fetch(&source, "label:bills", &TextExtractor, &store).await?;

    let ids: Vec<_> = first.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["m1", "m2", "m3"]);
    assert!(matches!(first[0].outcome, Ok(Outcome::Created(_))));
    assert!(matches!(first[1].outcome, Ok(Outcome::Created(_))));
    assert!(matches!(first[2].outcome, Err(IngestError::Email(_))));
    assert_eq!(store.list().await?.len(), 2);

    let second = fetch(&source, "label:bills", &TextExtractor, &store).await?;
    assert!(matches!(second[0].outcome, Ok(Outcome::Duplicate(_))));
    assert!(matches!(second[1].outcome, Ok(Outcome::Duplicate(_))));
    assert_eq!(store.list().await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn ac6_download_failure_aborts_keeping_earlier_bills() -> Result<()> {
    let source = FakeSource {
        raw: HashMap::from([("m1", BILL_A)]),
    };
    let store = InMemoryStore::new();

    let result = fetch(&source, "q", &TextExtractor, &store).await;

    assert!(matches!(result, Err(Error::Transport { .. })), "{result:?}");
    assert_eq!(store.list().await?.len(), 1);
    Ok(())
}
