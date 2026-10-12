//! Shared fixtures for `web`'s [e2e] tests.
#![allow(dead_code)] // not every fixture is used by every test file

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use hauz_core::bill::{Bill, BillDraft, BillId, BillingPeriod, Currency, Money, Status, Vendor};
use hauz_core::store::{BillStore, RawHash, SqliteStore};
use http_body_util::BodyExt;
use time::macros::date;
use topcoat::router::{Body, Router, request::Request};

pub(crate) type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

/// A fresh, unique directory under the system temp dir.
pub(crate) fn tmp_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("hauz-web-{pid}-{nanos}-{n}"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// A complete `Extracted` bill: Acme Power, 1234.56 COP.
pub(crate) fn extracted() -> Result<Bill> {
    Ok(Bill::try_from(BillDraft {
        id: BillId::new("aaaaaaaa-old-bill")?,
        vendor: Some(Vendor::new("Acme Power")?),
        amount: Some(Money::new(123_456, Currency::new("COP")?)),
        period: Some(BillingPeriod::new(
            date!(2026 - 01 - 01),
            date!(2026 - 01 - 31),
        )?),
        issued: Some(date!(2026 - 01 - 05)),
        due: Some(date!(2026 - 02 - 15)),
        status: Status::Extracted,
    })?)
}

/// An amountless `NeedsReview` bill: Beta Water, no dates.
pub(crate) fn needs_review() -> Result<Bill> {
    Ok(Bill::try_from(BillDraft {
        id: BillId::new("bbbbbbbb-new-bill")?,
        vendor: Some(Vendor::new("Beta Water")?),
        amount: None,
        period: None,
        issued: None,
        due: None,
        status: Status::NeedsReview,
    })?)
}

/// A bill whose vendor name is an HTML injection attempt.
pub(crate) fn scripted() -> Result<Bill> {
    Ok(Bill::try_from(BillDraft {
        id: BillId::new("cccccccc-xss-bill")?,
        vendor: Some(Vendor::new("<script>alert(1)</script>")?),
        amount: None,
        period: None,
        issued: None,
        due: None,
        status: Status::NeedsReview,
    })?)
}

/// Creates a DB at `path` holding `bills` (oldest first).
pub(crate) async fn seed(path: &Path, bills: &[Bill]) -> Result<()> {
    let store = SqliteStore::open(path).await?;
    for (n, bill) in bills.iter().enumerate() {
        store.insert(&RawHash::new([n as u8; 32]), bill).await?;
    }
    Ok(())
}

/// A router over a fresh tmp DB seeded with `bills`, opened read-only.
pub(crate) async fn router_with(bills: &[Bill]) -> Result<Router> {
    let db = tmp_dir().join("t.db");
    seed(&db, bills).await?;
    Ok(hauz_web::router(SqliteStore::open_read_only(&db).await?))
}

/// One in-process GET: status, content type, body.
pub(crate) async fn get(router: &Router, uri: &str) -> Result<(u16, String, String)> {
    let response = router
        .handle(Request::get(uri).body(Body::default())?)
        .await;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let bytes = response
        .into_body()
        .collect()
        .await
        .map_err(|e| e.to_string())?
        .to_bytes();
    Ok((status, content_type, String::from_utf8(bytes.to_vec())?))
}
