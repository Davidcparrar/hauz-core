//! [e2e] tests through the outer entry point: `tower::ServiceExt::oneshot` against
//! `hauz_server::router`. Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode};
use hauz_core::bill::{Currency, Money, Status};
use hauz_core::store::BillStore;
use http_body_util::BodyExt;
use tower::ServiceExt;

/// Boxed so any error type propagates with `?`; tests never unwrap or expect.
type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

const BILL_EML: &[u8] = include_bytes!("fixtures/bill.eml");
const DIAN_NO_PERIOD_EML: &[u8] =
    include_bytes!("../../core/tests/fixtures/ubl/dian_no_period.eml");
const BILL_WITH_BZIP2_EML: &[u8] =
    include_bytes!("../../core/tests/fixtures/zip/bill_with_bzip2.eml");
const DIAN_CORRUPT_EML: &[u8] = include_bytes!("../../core/tests/fixtures/ubl/dian_corrupt.eml");
const MALFORMED_EML: &[u8] = include_bytes!("fixtures/malformed.eml");

/// Sends `body` as a `POST /v1/ingest/email` request and returns the response's status plus
/// its JSON body (`serde_json::Value::Null` when the body is empty).
async fn post_email(router: Router, body: Vec<u8>) -> Result<(StatusCode, serde_json::Value)> {
    let request = Request::builder()
        .method("POST")
        .uri("/v1/ingest/email")
        .body(Body::from(body))?;
    let response = router.oneshot(request).await?;
    let status = response.status();
    let bytes: Bytes = response.into_body().collect().await?.to_bytes();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes)?
    };
    Ok((status, json))
}

/// Sends `body` as a `POST /v1/ingest/email` request and returns only the response's status:
/// for cases (like a body-limit rejection) where the response body is not JSON.
async fn post_email_status(router: Router, body: Vec<u8>) -> Result<StatusCode> {
    let request = Request::builder()
        .method("POST")
        .uri("/v1/ingest/email")
        .body(Body::from(body))?;
    Ok(router.oneshot(request).await?.status())
}

/// Sends a `GET /v1/bills/{id}` request and returns the response's status plus its JSON body.
async fn get_bill(router: Router, id: &str) -> Result<(StatusCode, serde_json::Value)> {
    let request = Request::builder()
        .method("GET")
        .uri(format!("/v1/bills/{id}"))
        .body(Body::empty())?;
    let response = router.oneshot(request).await?;
    let status = response.status();
    let bytes: Bytes = response.into_body().collect().await?.to_bytes();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes)?
    };
    Ok((status, json))
}

/// Lowercase hex of `bytes`, matching `hauz_core::ingest::raw_hash`'s id encoding.
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// AC1: POSTing `bill.eml` to an empty store answers 201 with `{"id": h}` (`h` = lowercase hex
/// of `raw_hash`), and `GET /v1/bills/{h}` answers 200 with a `Bill` of id `h`, `NeedsReview`,
/// amount `Money(123456, EUR)`, due 2026-10-15, vendor `acme-power.example`.
#[tokio::test]
async fn ac1_post_bill_then_get_returns_extracted_fields() -> Result<()> {
    let (router, _store) = common::app().await?;
    let expected_id = to_hex(hauz_core::ingest::raw_hash(BILL_EML).as_bytes());

    let (status, body) = post_email(router.clone(), BILL_EML.to_vec()).await?;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["id"], expected_id);

    let (status, body) = get_bill(router, &expected_id).await?;
    assert_eq!(status, StatusCode::OK);
    let bill: hauz_core::bill::Bill = serde_json::from_value(body)?;
    assert_eq!(bill.id().as_str(), expected_id);
    assert_eq!(bill.status(), Status::NeedsReview);
    assert_eq!(
        bill.amount().cloned(),
        Some(Money::new(123_456, Currency::new("EUR")?))
    );
    assert_eq!(bill.due(), Some(time::macros::date!(2026 - 10 - 15)));
    assert_eq!(bill.vendor().map(|v| v.name()), Some("acme-power.example"));
    Ok(())
}

/// AC2: POSTing the same bytes again answers 200 with AC1's id, and `store.list()` still has
/// length 1.
#[tokio::test]
async fn ac2_duplicate_post_returns_same_id_and_store_unchanged() -> Result<()> {
    let (router, store) = common::app().await?;
    let expected_id = to_hex(hauz_core::ingest::raw_hash(BILL_EML).as_bytes());

    let (status, _body) = post_email(router.clone(), BILL_EML.to_vec()).await?;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = post_email(router, BILL_EML.to_vec()).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], expected_id);
    assert_eq!(store.list().await?.len(), 1);
    Ok(())
}

/// AC3: POSTing `malformed.eml` answers 400 with a non-empty `error` field, and `store.list()`
/// stays empty.
#[tokio::test]
async fn ac3_malformed_email_is_bad_request_and_store_stays_empty() -> Result<()> {
    let (router, store) = common::app().await?;

    let (status, body) = post_email(router, MALFORMED_EML.to_vec()).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().is_some_and(|s| !s.is_empty()));
    assert_eq!(store.list().await?.len(), 0);
    Ok(())
}

/// AC4: `GET /v1/bills/{id}` answers 404 both for a well-formed id that is not stored and for
/// a string `BillId::new` rejects.
#[tokio::test]
async fn ac4_get_unknown_or_invalid_id_is_not_found() -> Result<()> {
    let (router, _store) = common::app().await?;

    let (status, _body) = get_bill(router.clone(), "does-not-exist").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _body) = get_bill(router, "has%20spaces").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

/// AC5: with the state holding a `FailingStore`, both a POST of `bill.eml` and a GET answer
/// 500 with `{"error":"internal error"}`.
#[tokio::test]
async fn ac5_failing_store_is_internal_error_for_post_and_get() -> Result<()> {
    let router = common::app_with_store(Arc::new(common::FailingStore));

    let (status, body) = post_email(router.clone(), BILL_EML.to_vec()).await?;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "internal error");

    let (status, body) = get_bill(router, "any-id").await?;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "internal error");
    Ok(())
}

/// AC6: a `POST` body of `MAX_BODY_BYTES + 1` bytes answers 413.
#[tokio::test]
async fn ac6_oversized_body_is_payload_too_large() -> Result<()> {
    let (router, _store) = common::app().await?;
    let oversized = vec![0u8; hauz_server::MAX_BODY_BYTES + 1];

    let status = post_email_status(router, oversized).await?;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    Ok(())
}

/// AC9 (#37): POSTing `dian_no_period.eml` through the #27 chain and fetching the bill answers
/// 200 with `status` `"extracted"`, `period` null and `issued` 2026-09-10.
#[tokio::test]
async fn ac9_dian_without_period_is_extracted_with_issued() -> Result<()> {
    let (router, _store) = common::app_with_xml().await?;
    let id = to_hex(hauz_core::ingest::raw_hash(DIAN_NO_PERIOD_EML).as_bytes());

    let (status, _) = post_email(router.clone(), DIAN_NO_PERIOD_EML.to_vec()).await?;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = get_bill(router, &id).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "extracted");
    assert!(body["period"].is_null());
    let bill: hauz_core::bill::Bill = serde_json::from_value(body)?;
    assert_eq!(bill.issued(), Some(time::macros::date!(2026 - 09 - 10)));
    Ok(())
}

/// AC10 (#37): POSTing `bill.eml` (no issued, no period) and fetching it answers `status`
/// `"needs_review"` with `issued` null.
#[tokio::test]
async fn ac10_bill_without_issued_or_period_needs_review() -> Result<()> {
    let (router, _store) = common::app_with_xml().await?;
    let id = to_hex(hauz_core::ingest::raw_hash(BILL_EML).as_bytes());

    let (status, _) = post_email(router.clone(), BILL_EML.to_vec()).await?;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = get_bill(router, &id).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "needs_review");
    assert!(body.get("issued").is_some_and(serde_json::Value::is_null));
    Ok(())
}

/// AC5 (#36): POSTing `bill_with_bzip2.eml` through the #27 chain answers 201, and fetching
/// the bill returns amount 1,234.56 EUR.
#[tokio::test]
async fn ac5_bill_with_unsupported_zip_is_created_with_amount() -> Result<()> {
    let (router, _store) = common::app_with_xml().await?;
    let id = to_hex(hauz_core::ingest::raw_hash(BILL_WITH_BZIP2_EML).as_bytes());

    let (status, _) = post_email(router.clone(), BILL_WITH_BZIP2_EML.to_vec()).await?;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = get_bill(router, &id).await?;
    assert_eq!(status, StatusCode::OK);
    let bill: hauz_core::bill::Bill = serde_json::from_value(body)?;
    assert_eq!(
        bill.amount(),
        Some(&Money::new(123_456, Currency::new("EUR")?))
    );
    Ok(())
}

/// AC6 (#36): POSTing `dian_corrupt.eml` (a malformed zip) through the #27 chain still
/// answers 400 and stores nothing.
#[tokio::test]
async fn ac6_malformed_zip_is_bad_request_and_store_stays_empty() -> Result<()> {
    let (router, store) = common::app_with_xml().await?;

    let (status, _) = post_email(router, DIAN_CORRUPT_EML.to_vec()).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(store.list().await?.len(), 0);
    Ok(())
}
