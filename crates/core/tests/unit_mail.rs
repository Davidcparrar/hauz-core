//! [unit] acceptance tests for `mail` (feature #42): `GmailSource` against an in-process
//! axum fake of Google's token and Gmail endpoints, and `Config`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Form, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use hauz_core::mail::{
    Config, Credentials, DateRange, Error, GmailSource, MailSource, MessageId, PageToken,
};

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

/// What the fake answers, and what it saw.
#[derive(Default)]
struct Fake {
    token: (u16, String),
    list: (u16, String),
    get: (u16, String),
    token_forms: Vec<HashMap<String, String>>,
    list_calls: Vec<(Option<String>, HashMap<String, String>)>,
    get_calls: Vec<(String, Option<String>, HashMap<String, String>)>,
}

type Shared = Arc<Mutex<Fake>>;

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Fake> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn ok_token() -> (u16, String) {
    (
        200,
        r#"{"access_token":"at-1","expires_in":3600,"token_type":"Bearer"}"#.to_owned(),
    )
}

fn reply((status, body): (u16, String)) -> (StatusCode, [(&'static str, &'static str); 1], String) {
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        [("content-type", "application/json")],
        body,
    )
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

async fn serve(fake: Fake) -> Result<(Shared, String)> {
    let shared: Shared = Arc::new(Mutex::new(fake));
    let app = Router::new()
        .route(
            "/token",
            post(
                |State(s): State<Shared>, Form(form): Form<HashMap<String, String>>| async move {
                    let mut s = lock(&s);
                    s.token_forms.push(form);
                    reply(s.token.clone())
                },
            ),
        )
        .route(
            "/gmail/v1/users/me/messages",
            get(
                |State(s): State<Shared>,
                 headers: HeaderMap,
                 Query(q): Query<HashMap<String, String>>| async move {
                    let mut s = lock(&s);
                    s.list_calls.push((bearer(&headers), q));
                    reply(s.list.clone())
                },
            ),
        )
        .route(
            "/gmail/v1/users/me/messages/{id}",
            get(
                |State(s): State<Shared>,
                 Path(id): Path<String>,
                 headers: HeaderMap,
                 Query(q): Query<HashMap<String, String>>| async move {
                    let mut s = lock(&s);
                    s.get_calls.push((id, bearer(&headers), q));
                    reply(s.get.clone())
                },
            ),
        )
        .with_state(Arc::clone(&shared));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok((shared, base))
}

fn creds() -> Credentials {
    Credentials {
        client_id: "cid".to_owned(),
        client_secret: "SECRET-VALUE".to_owned(),
        refresh_token: "REFRESH-VALUE".to_owned(),
    }
}

fn source(base: &str) -> GmailSource {
    GmailSource::with_endpoints(creds(), format!("{base}/token"), base.to_owned())
}

#[tokio::test]
async fn ac1_lists_ids_with_token_form_and_bearer() -> Result<()> {
    let (shared, base) = serve(Fake {
        token: ok_token(),
        list: (
            200,
            r#"{"messages":[{"id":"a1","threadId":"t"},{"id":"b2"}],"nextPageToken":"NEXT"}"#
                .to_owned(),
        ),
        ..Fake::default()
    })
    .await?;
    let src = source(&base);

    let page = src
        .list("label:bills", Some(&PageToken::new("PREV")))
        .await?;

    assert_eq!(
        page.ids.iter().map(MessageId::as_str).collect::<Vec<_>>(),
        ["a1", "b2"]
    );
    assert_eq!(page.next.as_ref().map(PageToken::as_str), Some("NEXT"));
    {
        let s = shared.lock().unwrap();
        let form = &s.token_forms[0];
        assert_eq!(form["client_id"], "cid");
        assert_eq!(form["client_secret"], "SECRET-VALUE");
        assert_eq!(form["refresh_token"], "REFRESH-VALUE");
        assert_eq!(form["grant_type"], "refresh_token");
        let (auth, q) = &s.list_calls[0];
        assert_eq!(auth.as_deref(), Some("Bearer at-1"));
        assert_eq!(q["q"], "label:bills");
        assert_eq!(q["maxResults"], "500");
        assert_eq!(q["pageToken"], "PREV");
    }
    empty_listing_has_no_ids_and_no_next().await
}

async fn empty_listing_has_no_ids_and_no_next() -> Result<()> {
    let (shared, base) = serve(Fake {
        token: ok_token(),
        list: (200, "{}".to_owned()),
        ..Fake::default()
    })
    .await?;

    let page = source(&base).list("label:x", None).await?;

    assert!(page.ids.is_empty());
    assert!(page.next.is_none());
    let s = lock(&shared);
    let (_, q) = s.list_calls.first().ok_or("one list call")?;
    assert!(!q.contains_key("pageToken"));
    Ok(())
}

#[tokio::test]
async fn ac2_decodes_padded_and_unpadded_base64url_with_one_token_request() -> Result<()> {
    // Bytes 0xfb 0xff 0xfe encode to "-__-" (uses `-` and `_`); "ab" encodes to "YWI=".
    let (shared, base) = serve(Fake {
        token: ok_token(),
        list: (200, r#"{"messages":[{"id":"a1"}]}"#.to_owned()),
        get: (200, r#"{"raw":"-__-"}"#.to_owned()),
        ..Fake::default()
    })
    .await?;
    let src = source(&base);
    let id = MessageId::new("a1")?;

    src.list("q", None).await?;
    assert_eq!(src.fetch_raw(&id).await?, vec![0xfb, 0xff, 0xfe]);
    shared.lock().unwrap().get = (200, r#"{"raw":"YWI="}"#.to_owned());
    assert_eq!(src.fetch_raw(&id).await?, b"ab");
    shared.lock().unwrap().get = (200, r#"{"raw":"YWI"}"#.to_owned());
    assert_eq!(src.fetch_raw(&id).await?, b"ab");

    let s = shared.lock().unwrap();
    assert_eq!(s.token_forms.len(), 1);
    let (path_id, auth, q) = &s.get_calls[0];
    assert_eq!(path_id, "a1");
    assert_eq!(auth.as_deref(), Some("Bearer at-1"));
    assert_eq!(q["format"], "raw");
    Ok(())
}

async fn list_error(fake: Fake) -> Result<Error> {
    let (_shared, base) = serve(fake).await?;
    match source(&base).list("q", None).await {
        Ok(page) => Err(format!("expected an error, got {page:?}").into()),
        Err(e) => Ok(e),
    }
}

#[tokio::test]
async fn ac3_maps_auth_transport_and_malformed() -> Result<()> {
    let token_400 = list_error(Fake {
        token: (400, r#"{"error":"invalid_grant"}"#.to_owned()),
        ..Fake::default()
    })
    .await?;
    assert!(matches!(token_400, Error::Auth { .. }), "{token_400:?}");

    let token_401 = list_error(Fake {
        token: (401, "{}".to_owned()),
        ..Fake::default()
    })
    .await?;
    assert!(matches!(token_401, Error::Auth { .. }), "{token_401:?}");

    for status in [401, 403] {
        let e = list_error(Fake {
            token: ok_token(),
            list: (status, "{}".to_owned()),
            ..Fake::default()
        })
        .await?;
        assert!(matches!(e, Error::Auth { .. }), "{e:?}");
    }

    let server_error = list_error(Fake {
        token: ok_token(),
        list: (500, "oops".to_owned()),
        ..Fake::default()
    })
    .await?;
    assert!(
        matches!(server_error, Error::Transport { .. }),
        "{server_error:?}"
    );

    let token_500 = list_error(Fake {
        token: (500, "oops".to_owned()),
        ..Fake::default()
    })
    .await?;
    assert!(
        matches!(token_500, Error::Transport { .. }),
        "{token_500:?}"
    );

    let bad_json = list_error(Fake {
        token: ok_token(),
        list: (200, "not json".to_owned()),
        ..Fake::default()
    })
    .await?;
    assert!(matches!(bad_json, Error::Malformed { .. }), "{bad_json:?}");

    let (_shared, base) = serve(Fake {
        token: ok_token(),
        get: (200, r#"{"raw":"@@@@"}"#.to_owned()),
        ..Fake::default()
    })
    .await?;
    let bad_b64 = source(&base).fetch_raw(&MessageId::new("a1")?).await;
    assert!(
        matches!(bad_b64, Err(Error::Malformed { .. })),
        "{bad_b64:?}"
    );
    Ok(())
}

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    move |key| map.get(key).cloned()
}

const FULL: [(&str, &str); 4] = [
    ("HAUZ_GMAIL_CLIENT_ID", "cid"),
    ("HAUZ_GMAIL_CLIENT_SECRET", "SECRET-VALUE"),
    ("HAUZ_GMAIL_REFRESH_TOKEN", "REFRESH-VALUE"),
    ("HAUZ_GMAIL_LABEL", "bills"),
];

#[test]
fn ac4_config_absent_missing_and_redacted() -> Result<()> {
    assert!(Config::from_env(env(&[]))?.is_none());

    for missing in [
        "HAUZ_GMAIL_CLIENT_SECRET",
        "HAUZ_GMAIL_REFRESH_TOKEN",
        "HAUZ_GMAIL_LABEL",
    ] {
        let pairs: Vec<_> = FULL
            .iter()
            .filter(|(k, _)| *k != missing)
            .copied()
            .collect();
        match Config::from_env(env(&pairs)) {
            Err(Error::Config { variable }) => assert_eq!(variable, missing),
            other => return Err(format!("expected Config error, got {other:?}").into()),
        }
    }

    let config = Config::from_env(env(&FULL))?.ok_or("configured")?;
    assert_eq!(config.query(&DateRange::parse(None, None)?), "label:bills");
    let rendered = [
        format!("{config:?}"),
        format!("{:?}", creds()),
        format!("{:?}", config.source()),
        format!("{:?}", source("http://x")),
        format!(
            "{:?}",
            Error::Auth {
                reason: "token endpoint answered 400".to_owned()
            }
        ),
        format!(
            "{:?}",
            Config::from_env(env(&[("HAUZ_GMAIL_CLIENT_ID", "c")]))
        ),
    ];
    for text in rendered {
        assert!(!text.contains("SECRET-VALUE"), "{text}");
        assert!(!text.contains("REFRESH-VALUE"), "{text}");
    }
    Ok(())
}

#[test]
fn ac9_date_range_bounds_the_query_and_rejects_bad_input() -> Result<()> {
    let config = Config::from_env(env(&FULL))?.ok_or("configured")?;
    let query =
        |after, before| -> Result<String> { Ok(config.query(&DateRange::parse(after, before)?)) };
    assert_eq!(query(None, None)?, "label:bills");
    assert_eq!(
        query(Some("2026-09-01"), None)?,
        "label:bills after:1788220800"
    );
    assert_eq!(
        query(None, Some("2026-10-01"))?,
        "label:bills before:1790812800"
    );
    assert_eq!(
        query(Some("2026-09-01"), Some("2026-10-01"))?,
        "label:bills after:1788220800 before:1790812800"
    );
    for (after, before) in [
        (Some("2026/09/01"), None),
        (Some("2026-9-1"), None),
        (None, Some("not-a-day")),
        (Some("2026-02-30"), None),
        (Some("2026-10-01"), Some("2026-10-01")),
        (Some("2026-10-02"), Some("2026-10-01")),
    ] {
        assert!(
            matches!(
                DateRange::parse(after, before),
                Err(Error::InvalidRange { .. })
            ),
            "{after:?} {before:?}"
        );
    }
    Ok(())
}
