//! The axum shell: raw bytes in, `hauz-core` calls out, status codes back. `POST
//! /v1/ingest/email` runs [`hauz_core::ingest::ingest`] over the raw RFC 5322 body;
//! `GET /v1/bills/{id}` reads a stored [`hauz_core::bill::Bill`] back as JSON. No
//! `Content-Type` enforcement, no auth, no logging: those are deliberately out of scope here.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use hauz_core::bill::BillId;
use hauz_core::extract::Extractor;
use hauz_core::ingest::{Error as IngestError, Outcome, ingest};
use hauz_core::store::BillStore;
use serde::Serialize;

/// The maximum accepted `POST /v1/ingest/email` body size: an oversized body is rejected
/// with 413 before `ingest` ever sees it.
pub const MAX_BODY_BYTES: usize = 25 * 1024 * 1024;

/// The server's shared state: the store and extractor every request is handled against.
/// Cheap to clone (both fields are `Arc`), as axum requires for per-request state.
#[derive(Clone)]
pub struct AppState {
    store: Arc<dyn BillStore>,
    extractor: Arc<dyn Extractor>,
}

impl AppState {
    /// Builds the server's state from an injected store and extractor: both are system edges
    /// (storage, extraction), so tests supply fakes behind the same traits.
    #[must_use]
    pub fn new(store: Arc<dyn BillStore>, extractor: Arc<dyn Extractor>) -> Self {
        Self { store, extractor }
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

/// Builds the server's router: `POST /v1/ingest/email`, `GET /v1/bills/{id}`, both against
/// `state`, with the body of the former capped at [`MAX_BODY_BYTES`] (oversized ⇒ 413).
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/ingest/email", post(post_ingest_email))
        .route("/v1/bills/{id}", get(get_bill))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}
