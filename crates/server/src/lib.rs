//! The axum shell: raw bytes in, `hauz-core` calls out, status codes back. `POST
//! /v1/ingest/email` runs [`hauz_core::ingest::ingest`] over the raw RFC 5322 body;
//! `GET /v1/bills/{id}` reads a stored [`hauz_core::bill::Bill`] back as JSON. No
//! `Content-Type` enforcement, no logging: those are deliberately out of scope here.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::Request;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use hauz_core::bill::BillId;
use hauz_core::extract::Extractor;
use hauz_core::ingest::{Error as IngestError, Outcome, ingest};
use hauz_core::mail::{self, Fetched, MailSource};
use hauz_core::store::BillStore;
use serde::Serialize;
use time::OffsetDateTime;
use tokio::time::MissedTickBehavior;

/// The maximum accepted `POST /v1/ingest/email` body size: an oversized body is rejected
/// with 413 before `ingest` ever sees it.
pub const MAX_BODY_BYTES: usize = 25 * 1024 * 1024;

/// The shared secret every `/v1` request must present as `Authorization: Bearer <token>`.
/// Never blank, never printed: `Debug` is redacted and there is no accessor.
#[derive(Clone)]
pub struct ApiToken(Arc<str>);

impl ApiToken {
    /// `None` when `token` is empty or only whitespace (a blank token would leave the
    /// server silently open).
    #[must_use]
    pub fn new(token: impl Into<String>) -> Option<Self> {
        let token = token.into();
        if token.trim().is_empty() {
            None
        } else {
            Some(Self(Arc::from(token)))
        }
    }

    fn matches(&self, presented: &str) -> bool {
        constant_time_eq(self.0.as_bytes(), presented.as_bytes())
    }
}

impl std::fmt::Debug for ApiToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiToken(<redacted>)")
    }
}

/// Byte equality that XOR-folds every byte when the lengths match; only the length leaks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The server's shared state: the store, extractor and API token every request is handled
/// against. Cheap to clone (all fields are `Arc`), as axum requires for per-request state.
#[derive(Clone)]
pub struct AppState {
    store: Arc<dyn BillStore>,
    extractor: Arc<dyn Extractor>,
    token: ApiToken,
}

impl AppState {
    /// Builds the server's state from an injected store and extractor: both are system edges
    /// (storage, extraction), so tests supply fakes behind the same traits. The token is
    /// required: no unauthenticated state can be built.
    #[must_use]
    pub fn new(store: Arc<dyn BillStore>, extractor: Arc<dyn Extractor>, token: ApiToken) -> Self {
        Self {
            store,
            extractor,
            token,
        }
    }
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState").finish_non_exhaustive()
    }
}

/// The JSON body `{"id": "<hex>"}` returned by a successful `POST /v1/ingest/email`.
#[derive(Serialize)]
struct IdBody {
    id: String,
}

/// The JSON body `{"error": "<message>"}` returned on any non-2xx response.
#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

fn id_response(status: StatusCode, id: &BillId) -> Response {
    (
        status,
        Json(IdBody {
            id: id.as_str().to_owned(),
        }),
    )
        .into_response()
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(ErrorBody {
            error: message.into(),
        }),
    )
        .into_response()
}

/// `POST /v1/ingest/email`: the raw RFC 5322 bytes are the whole body. `Created`/`Duplicate`
/// answer 201/200 with the bill id; a message or extraction error answers 400 with its
/// `Display`; any other `ingest::Error` (store failure, an incomplete-bill bug, a future
/// variant) answers 500 without leaking detail.
async fn post_ingest_email(State(state): State<AppState>, body: Bytes) -> Response {
    match ingest(&body, &*state.extractor, &*state.store).await {
        Ok(Outcome::Created(id)) => id_response(StatusCode::CREATED, &id),
        Ok(Outcome::Duplicate(id)) => id_response(StatusCode::OK, &id),
        Err(err @ (IngestError::Email(_) | IngestError::Extract(_))) => {
            error_response(StatusCode::BAD_REQUEST, err.to_string())
        }
        // `ingest::Error` is #[non_exhaustive]; `Store`, `Bill`, and any future variant are
        // internal failures the client never sees the detail of.
        #[allow(clippy::wildcard_enum_match_arm)]
        Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
    }
}

/// `GET /v1/bills/{id}`: an id `BillId::new` rejects, or that a stored bill is not found for,
/// answers 404; a store failure answers 500; otherwise the stored `Bill` as JSON, 200.
async fn get_bill(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let Ok(id) = BillId::new(&id) else {
        return error_response(StatusCode::NOT_FOUND, "not found");
    };
    match state.store.get(&id).await {
        Ok(Some(bill)) => Json(bill).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "not found"),
        Err(_) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
    }
}

/// The credential in a single `Authorization: Bearer <token>` header, or `None` for any
/// other shape (absent, repeated, non-UTF-8, another scheme, no token).
fn bearer_credential(headers: &axum::http::HeaderMap) -> Option<&str> {
    let mut values = headers.get_all(AUTHORIZATION).iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    let value = value.to_str().ok()?;
    let (scheme, credential) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("Bearer").then_some(credential)
}

/// Rejects any request without the configured bearer token, before the handler (and so its
/// body extractor) runs: a fixed 401 with `WWW-Authenticate: Bearer`.
async fn require_bearer(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if bearer_credential(request.headers())
        .is_some_and(|credential| state.token.matches(credential))
    {
        return next.run(request).await;
    }
    let mut response = error_response(StatusCode::UNAUTHORIZED, "unauthorized");
    response
        .headers_mut()
        .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

/// Builds the server's router: `POST /v1/ingest/email`, `GET /v1/bills/{id}`, both against
/// `state`, each behind bearer-token auth (missing or wrong ⇒ 401), with the body of the
/// former capped at [`MAX_BODY_BYTES`] (oversized ⇒ 413).
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/ingest/email", post(post_ingest_email))
        .route("/v1/bills/{id}", get(get_bill))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_bearer,
        ))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}

/// The background Gmail poll: every tick runs [`mail::fetch`] over the last two UTC days of
/// the configured label, into the same extractor and store the webhook uses.
pub struct Poller {
    source: Arc<dyn MailSource>,
    config: mail::Config,
    extractor: Arc<dyn Extractor>,
    store: Arc<dyn BillStore>,
}

impl Poller {
    /// A poller over an injected mail source (a system edge), the Gmail `config` supplying the
    /// label, and the extractor and store to ingest into.
    #[must_use]
    pub fn new(
        source: Arc<dyn MailSource>,
        config: mail::Config,
        extractor: Arc<dyn Extractor>,
        store: Arc<dyn BillStore>,
    ) -> Self {
        Self {
            source,
            config,
            extractor,
            store,
        }
    }

    /// Runs one poll at `now` with `config.poll_query(now)`.
    ///
    /// # Errors
    /// Whatever [`mail::fetch`] returns: a failed list or download, or a store failure.
    pub async fn tick(&self, now: OffsetDateTime) -> Result<Vec<Fetched>, mail::Error> {
        mail::fetch(
            &*self.source,
            &self.config.poll_query(now),
            &*self.extractor,
            &*self.store,
        )
        .await
    }

    /// Polls forever on a background task: the first tick runs immediately, then one every
    /// `interval` (a slow tick delays the next rather than bursting). Each tick is reported
    /// through `log`; a failed tick is logged and retried at the next one.
    ///
    /// # Panics
    /// The task panics if `interval` is zero; [`mail::Config::poll_interval`] never is.
    pub fn spawn(
        self,
        interval: Duration,
        mut log: impl FnMut(String) + Send + 'static,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                match self.tick(OffsetDateTime::now_utc()).await {
                    Ok(fetched) => report(&fetched, &mut log),
                    Err(err) => log(format!("gmail poll failed: {err}")),
                }
            }
        })
    }
}

impl std::fmt::Debug for Poller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Poller")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// One summary line, then one line per message that failed to ingest.
fn report(fetched: &[Fetched], log: &mut impl FnMut(String)) {
    let created = fetched
        .iter()
        .filter(|f| matches!(f.outcome, Ok(Outcome::Created(_))))
        .count();
    let duplicate = fetched
        .iter()
        .filter(|f| matches!(f.outcome, Ok(Outcome::Duplicate(_))))
        .count();
    let failed = fetched.iter().filter(|f| f.outcome.is_err()).count();
    log(format!(
        "gmail poll: {} messages ({created} created, {duplicate} duplicate, {failed} failed)",
        fetched.len()
    ));
    for item in fetched {
        if let Err(err) = &item.outcome {
            log(format!("gmail poll: failed {}: {err}", item.id.as_str()));
        }
    }
}
