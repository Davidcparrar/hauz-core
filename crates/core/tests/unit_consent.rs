//! [unit] acceptance tests for `consent` (feature #57): a real loopback socket, an in-process
//! axum fake of the token endpoint, and a `reqwest` GET standing in for the browser.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::routing::post;
use hauz_core::consent::{Config, Error, authorize, entropy, pkce_challenge};

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;
type Forms = Arc<Mutex<Vec<HashMap<String, String>>>>;

fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |key| {
        pairs
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| (*v).to_owned())
    }
}

/// A token endpoint answering `(status, body)`; returns its recorded forms and base URL.
async fn fake_token(status: u16, body: &'static str) -> Result<(Forms, String)> {
    let forms: Forms = Arc::default();
    let app = Router::new()
        .route(
            "/token",
            post(
                move |State(forms): State<Forms>, Form(form): Form<HashMap<String, String>>| async move {
                    if let Ok(mut forms) = forms.lock() {
                        forms.push(form);
                    }
                    (
                        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                        [("content-type", "application/json")],
                        body,
                    )
                },
            ),
        )
        .with_state(Arc::clone(&forms));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok((forms, base))
}

fn config(token_base: &str) -> Result<Config> {
    let token_url = format!("{token_base}/token");
    Ok(Config::from_env(move |key| match key {
        "HAUZ_GMAIL_CLIENT_ID" => Some("cid".to_owned()),
        "HAUZ_GMAIL_CLIENT_SECRET" => Some("sec".to_owned()),
        "HAUZ_GMAIL_AUTH_URL" => Some("http://127.0.0.1:1/auth".to_owned()),
        "HAUZ_GMAIL_TOKEN_URL" => Some(token_url.clone()),
        _ => None,
    })?)
}

fn entropy_bytes() -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::try_from(i * 3 + 1).unwrap_or(0);
    }
    bytes
}

fn params(url: &str) -> Result<HashMap<String, String>> {
    Ok(reqwest::Url::parse(url)?
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect())
}

/// Runs `authorize` while a "browser" GETs each of `paths` (relative to the shown
/// `redirect_uri`; `{state}` is replaced by the real state) in order. Returns the result,
/// the shown URL's parameters and the browser's status codes.
async fn run(
    config: &Config,
    paths: Vec<&'static str>,
) -> Result<(
    core::result::Result<String, Error>,
    HashMap<String, String>,
    Vec<u16>,
)> {
    let shown: Arc<Mutex<Option<HashMap<String, String>>>> = Arc::default();
    let shown_in = Arc::clone(&shown);
    let browser = Arc::new(Mutex::new(None));
    let browser_in = Arc::clone(&browser);
    let result = authorize(config, entropy_bytes(), move |url| {
        let query = params(url).unwrap_or_default();
        let base = query.get("redirect_uri").cloned().unwrap_or_default();
        let state = query.get("state").cloned().unwrap_or_default();
        if let Ok(mut slot) = shown_in.lock() {
            *slot = Some(query);
        }
        let handle = tokio::spawn(async move {
            let client = reqwest::Client::new();
            let mut statuses = Vec::new();
            let mut idle = Vec::new();
            for path in paths {
                if path == "IDLE" {
                    let addr = base.trim_start_matches("http://");
                    if let Ok(stream) = tokio::net::TcpStream::connect(addr).await {
                        idle.push(stream);
                    }
                    continue;
                }
                let url = format!("{base}{}", path.replace("{state}", &state));
                if let Ok(response) = client.get(url).send().await {
                    statuses.push(response.status().as_u16());
                }
            }
            drop(idle);
            statuses
        });
        if let Ok(mut slot) = browser_in.lock() {
            *slot = Some(handle);
        }
    })
    .await;
    let handle = browser
        .lock()
        .map_err(|_| "poisoned")?
        .take()
        .ok_or("show was not called")?;
    let statuses = handle.await?;
    let shown = shown
        .lock()
        .map_err(|_| "poisoned")?
        .take()
        .ok_or("show was not called")?;
    Ok((result, shown, statuses))
}

const TOKEN_OK: &str = r#"{"access_token":"at","refresh_token":"RT-1","expires_in":3600}"#;

#[test]
fn ac1_config_requires_client_id_and_secret() -> Result<()> {
    for (pairs, missing) in [
        (
            &[("HAUZ_GMAIL_CLIENT_SECRET", "s")][..],
            "HAUZ_GMAIL_CLIENT_ID",
        ),
        (
            &[("HAUZ_GMAIL_CLIENT_ID", "c")][..],
            "HAUZ_GMAIL_CLIENT_SECRET",
        ),
        (
            &[
                ("HAUZ_GMAIL_CLIENT_ID", "c"),
                ("HAUZ_GMAIL_CLIENT_SECRET", ""),
            ][..],
            "HAUZ_GMAIL_CLIENT_SECRET",
        ),
        (
            &[
                ("HAUZ_GMAIL_CLIENT_ID", ""),
                ("HAUZ_GMAIL_CLIENT_SECRET", "s"),
            ][..],
            "HAUZ_GMAIL_CLIENT_ID",
        ),
    ] {
        let pairs: &'static [(&str, &str)] = Box::leak(pairs.to_vec().into_boxed_slice());
        match Config::from_env(env(pairs)) {
            Err(Error::Config { variable }) => assert_eq!(variable, missing),
            other => return Err(format!("expected Config error, got {other:?}").into()),
        }
    }
    let ok = Config::from_env(env(&[
        ("HAUZ_GMAIL_CLIENT_ID", "c"),
        ("HAUZ_GMAIL_CLIENT_SECRET", "SECRET-VALUE"),
    ]))?;
    assert!(!format!("{ok:?}").contains("SECRET-VALUE"));
    Ok(())
}

#[test]
fn ac2_pkce_challenge_matches_rfc7636_appendix_b() {
    assert_eq!(
        pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[test]
fn ac3_entropy_differs_between_calls() -> Result<()> {
    assert_ne!(entropy()?, entropy()?);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn ac4_shows_url_exchanges_code_and_returns_refresh_token() -> Result<()> {
    let (forms, base) = fake_token(200, TOKEN_OK).await?;
    let (result, shown, statuses) =
        run(&config(&base)?, vec!["/?state={state}&code=CODE-1"]).await?;
    assert_eq!(result?, "RT-1");
    assert_eq!(statuses, vec![200]);

    let get = |k: &str| shown.get(k).map(String::as_str);
    assert_eq!(get("client_id"), Some("cid"));
    assert_eq!(get("response_type"), Some("code"));
    assert_eq!(
        get("scope"),
        Some("https://www.googleapis.com/auth/gmail.readonly")
    );
    assert_eq!(get("access_type"), Some("offline"));
    assert_eq!(get("prompt"), Some("consent"));
    assert_eq!(get("code_challenge_method"), Some("S256"));
    let redirect = reqwest::Url::parse(get("redirect_uri").ok_or("no redirect_uri")?)?;
    assert_eq!(redirect.host_str(), Some("127.0.0.1"));
    assert!(redirect.port().is_some_and(|p| p != 0));

    let forms = forms.lock().map_err(|_| "poisoned")?;
    assert_eq!(forms.len(), 1);
    let form = forms.first().ok_or("no form")?;
    let field = |k: &str| form.get(k).map(String::as_str);
    assert_eq!(field("grant_type"), Some("authorization_code"));
    assert_eq!(field("code"), Some("CODE-1"));
    assert_eq!(field("redirect_uri"), get("redirect_uri"));
    assert_eq!(field("client_id"), Some("cid"));
    assert_eq!(field("client_secret"), Some("sec"));
    let verifier = field("code_verifier").ok_or("no verifier")?;
    assert_eq!(
        Some(pkce_challenge(verifier).as_str()),
        get("code_challenge")
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn ac5_ignores_requests_without_code_then_completes() -> Result<()> {
    let (forms, base) = fake_token(200, TOKEN_OK).await?;
    // "IDLE" is a browser preconnect socket that sends nothing and stays open.
    let (result, _, statuses) = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        run(
            &config(&base)?,
            vec!["IDLE", "/favicon.ico", "/?state={state}&code=CODE-1"],
        ),
    )
    .await
    .map_err(|_| "authorize hung behind an idle connection")??;
    assert_eq!(result?, "RT-1");
    assert_eq!(statuses, vec![404, 200]);
    assert_eq!(forms.lock().map_err(|_| "poisoned")?.len(), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn ac6_state_mismatch_is_rejected_without_token_request() -> Result<()> {
    let (forms, base) = fake_token(200, TOKEN_OK).await?;
    let (result, _, statuses) = run(&config(&base)?, vec!["/?state=WRONG&code=CODE-1"]).await?;
    assert!(matches!(result, Err(Error::StateMismatch)), "{result:?}");
    assert_eq!(statuses, vec![400]);
    assert!(forms.lock().map_err(|_| "poisoned")?.is_empty());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn ac7_denied_consent_is_reported_without_token_request() -> Result<()> {
    let (forms, base) = fake_token(200, TOKEN_OK).await?;
    let (result, _, _) = run(&config(&base)?, vec!["/?state={state}&error=access_denied"]).await?;
    match result {
        Err(Error::Denied { reason }) => assert_eq!(reason, "access_denied"),
        other => return Err(format!("expected Denied, got {other:?}").into()),
    }
    assert!(forms.lock().map_err(|_| "poisoned")?.is_empty());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn ac8_token_endpoint_failures_map_to_auth_and_malformed() -> Result<()> {
    let (_, base) = fake_token(400, r#"{"error":"invalid_grant"}"#).await?;
    let (result, _, _) = run(&config(&base)?, vec!["/?state={state}&code=C"]).await?;
    assert!(matches!(result, Err(Error::Auth { .. })), "{result:?}");

    let (_, base) = fake_token(200, r#"{"access_token":"at"}"#).await?;
    let (result, _, _) = run(&config(&base)?, vec!["/?state={state}&code=C"]).await?;
    assert!(matches!(result, Err(Error::Malformed { .. })), "{result:?}");
    Ok(())
}
