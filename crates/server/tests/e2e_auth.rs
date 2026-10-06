//! [e2e] tests for bearer-token auth on every `/v1` route (#40), in-process via `oneshot`.

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use hauz_core::store::BillStore;
use hauz_server::ApiToken;
use http_body_util::BodyExt;
use tower::ServiceExt;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

const BILL_EML: &[u8] = include_bytes!("fixtures/bill.eml");

/// Sends `method uri` with an optional raw `Authorization` value; returns status, the
/// `WWW-Authenticate` header and the body bytes.
async fn send(
    router: Router,
    method: &str,
    uri: &str,
    auth: Option<&str>,
    body: Vec<u8>,
) -> Result<(StatusCode, Option<String>, Vec<u8>)> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(value) = auth {
        builder = builder.header("authorization", value);
    }
    let response = router.oneshot(builder.body(Body::from(body))?).await?;
    let status = response.status();
    let challenge = response
        .headers()
        .get("www-authenticate")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let bytes = response.into_body().collect().await?.to_bytes().to_vec();
    Ok((status, challenge, bytes))
}

fn bearer() -> String {
    format!("Bearer {}", common::TOKEN)
}

fn id_of(bytes: &[u8]) -> Result<String> {
    let json: serde_json::Value = serde_json::from_slice(bytes)?;
    Ok(json
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned())
}

async fn assert_unauthorized(
    router: &Router,
    store: &impl BillStore,
    auth: Option<&str>,
) -> Result<()> {
    let (status, challenge, body) = send(
        router.clone(),
        "POST",
        "/v1/ingest/email",
        auth,
        BILL_EML.to_vec(),
    )
    .await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "auth: {auth:?}");
    assert_eq!(challenge.as_deref(), Some("Bearer"));
    assert_eq!(body, br#"{"error":"unauthorized"}"#);
    assert_eq!(store.list().await?.len(), 0);

    let (status, challenge, body) =
        send(router.clone(), "GET", "/v1/bills/abc", auth, Vec::new()).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "auth: {auth:?}");
    assert_eq!(challenge.as_deref(), Some("Bearer"));
    assert_eq!(body, br#"{"error":"unauthorized"}"#);
    Ok(())
}

/// AC1: the right token reaches both routes.
#[tokio::test]
async fn ac1_correct_token_is_accepted() -> Result<()> {
    let (router, store) = common::app().await?;
    let auth = bearer();
    let (status, _, body) = send(
        router.clone(),
        "POST",
        "/v1/ingest/email",
        Some(&auth),
        BILL_EML.to_vec(),
    )
    .await?;
    assert_eq!(status, StatusCode::CREATED);
    let id = id_of(&body)?;
    assert!(!id.is_empty());
    assert_eq!(store.list().await?.len(), 1);

    let (status, _, body) = send(
        router,
        "GET",
        &format!("/v1/bills/{id}"),
        Some(&auth),
        Vec::new(),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(json.get("id").and_then(|v| v.as_str()), Some(id.as_str()));
    Ok(())
}

/// AC2: no header is a fixed 401.
#[tokio::test]
async fn ac2_missing_header_is_unauthorized() -> Result<()> {
    let (router, store) = common::app().await?;
    assert_unauthorized(&router, &*store, None).await
}

/// AC3: every malformed or wrong credential is the same 401.
#[tokio::test]
async fn ac3_wrong_credentials_are_unauthorized() -> Result<()> {
    let (router, store) = common::app().await?;
    let token = common::TOKEN;
    let mut flipped = token.to_owned();
    flipped.replace_range(0..1, "X");
    let wrong = [
        format!("Bearer {flipped}"),
        format!(
            "Bearer {}",
            token
                .trim_end_matches(|_| true)
                .chars()
                .take(token.len() - 1)
                .collect::<String>()
        ),
        format!("Bearer {token}x"),
        format!("Basic {token}"),
        "Bearer".to_owned(),
        format!("Bearer  {token}"),
    ];
    for auth in &wrong {
        assert_unauthorized(&router, &*store, Some(auth)).await?;
    }
    Ok(())
}

/// AC4: the scheme is case-insensitive.
#[tokio::test]
async fn ac4_lowercase_scheme_is_accepted() -> Result<()> {
    let (router, _store) = common::app().await?;
    let auth = format!("bearer {}", common::TOKEN);
    let (status, _, _) = send(
        router,
        "POST",
        "/v1/ingest/email",
        Some(&auth),
        BILL_EML.to_vec(),
    )
    .await?;
    assert_eq!(status, StatusCode::CREATED);
    Ok(())
}

/// AC5: auth runs before the body is read.
#[tokio::test]
async fn ac5_auth_is_checked_before_body_limit() -> Result<()> {
    let (router, _store) = common::app().await?;
    let oversized = vec![0u8; hauz_server::MAX_BODY_BYTES + 1];
    let (status, _, _) = send(
        router.clone(),
        "POST",
        "/v1/ingest/email",
        None,
        oversized.clone(),
    )
    .await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let auth = bearer();
    let (status, _, _) = send(router, "POST", "/v1/ingest/email", Some(&auth), oversized).await?;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    Ok(())
}

/// AC6: blank tokens are refused; the Debug output is redacted.
#[test]
fn ac6_blank_token_rejected_and_debug_redacted() {
    assert!(ApiToken::new("").is_none());
    assert!(ApiToken::new("  \t").is_none());
    let token = ApiToken::new("s3cret-value");
    assert!(token.is_some());
    assert!(!format!("{token:?}").contains("s3cret-value"));
}
