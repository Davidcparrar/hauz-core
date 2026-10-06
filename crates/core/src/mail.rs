//! The mail-source edge: [`MailSource`] lists and downloads raw messages, [`GmailSource`] is
//! its Gmail REST implementation (OAuth refresh token, `gmail.readonly`), [`Config`] reads it
//! from the environment, and [`fetch()`] downloads everything under a query and runs each
//! message through [`ingest()`](crate::ingest::ingest). Secrets are redacted from every `Debug`.

use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::BoxFuture;
use crate::extract::Extractor;
use crate::ingest::{self, Outcome, ingest};
use crate::store::BillStore;

const DEFAULT_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const DEFAULT_API_BASE: &str = "https://gmail.googleapis.com";
const MAX_RESULTS: &str = "500";
/// Refresh this long before the token's stated expiry.
const EXPIRY_MARGIN: Duration = Duration::from_secs(60);

/// A Gmail message id: never empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MessageId(String);

impl MessageId {
    /// # Errors
    /// `Error::Malformed` when `id` is empty.
    pub fn new(id: impl Into<String>) -> Result<Self, Error> {
        let id = id.into();
        if id.is_empty() {
            return Err(Error::Malformed {
                reason: "empty message id".to_owned(),
            });
        }
        Ok(Self(id))
    }

    /// The id as sent by the source.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An opaque continuation token for the next page of a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageToken(String);

impl PageToken {
    /// Wraps a token as received from the source.
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    /// The token text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One page of a listing: the message ids, and the token for the next page if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// Message ids on this page.
    pub ids: Vec<MessageId>,
    /// `Some` while more pages remain.
    pub next: Option<PageToken>,
}

/// `mail`'s errors. Reasons never carry secrets or tokens.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A required environment variable is missing.
    #[error("missing configuration: {variable}")]
    Config {
        /// The variable name.
        variable: String,
    },
    /// The token endpoint or Gmail rejected the credentials.
    #[error("auth rejected: {reason}")]
    Auth {
        /// What the server answered.
        reason: String,
    },
    /// The request failed, or the server answered a non-auth error status.
    #[error("transport error: {reason}")]
    Transport {
        /// What went wrong.
        reason: String,
    },
    /// The response body was not what the API documents.
    #[error("malformed response: {reason}")]
    Malformed {
        /// What was wrong with it.
        reason: String,
    },
    /// Ingesting message `id` failed for a reason that aborts the run.
    #[error("ingest of message {} failed: {source}", id.as_str())]
    Ingest {
        /// The Gmail id of the message.
        id: MessageId,
        /// The underlying failure.
        source: ingest::Error,
    },
}

/// Where raw messages come from: an injectable system edge.
pub trait MailSource: Send + Sync {
    /// One page of ids matching `query`, continuing from `page`.
    fn list<'a>(
        &'a self,
        query: &'a str,
        page: Option<&'a PageToken>,
    ) -> BoxFuture<'a, Result<Page, Error>>;

    /// The raw RFC 5322 bytes of message `id`.
    fn fetch_raw<'a>(&'a self, id: &'a MessageId) -> BoxFuture<'a, Result<Vec<u8>, Error>>;
}

/// OAuth client and user credentials. `Debug` redacts the secret and token.
#[derive(Clone)]
pub struct Credentials {
    /// OAuth client id.
    pub client_id: String,
    /// OAuth client secret.
    pub client_secret: String,
    /// The user's long-lived refresh token.
    pub refresh_token: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .field("refresh_token", &"<redacted>")
            .finish()
    }
}

/// A [`MailSource`] over the Gmail REST API. The access token is cached until it expires.
pub struct GmailSource {
    http: reqwest::Client,
    creds: Credentials,
    token_url: String,
    api_base: String,
    token: Mutex<Option<(String, Instant)>>,
}

impl fmt::Debug for GmailSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GmailSource")
            .field("creds", &self.creds)
            .field("token_url", &self.token_url)
            .field("api_base", &self.api_base)
            .field("token", &"<redacted>")
            .finish()
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<u64>,
}

#[derive(Deserialize)]
struct ListResponse {
    #[serde(default)]
    messages: Vec<ListedMessage>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct ListedMessage {
    id: String,
}

#[derive(Deserialize)]
struct RawResponse {
    raw: String,
}

impl GmailSource {
    /// A source talking to Google's production endpoints.
    #[must_use]
    pub fn new(creds: Credentials) -> Self {
        Self::with_endpoints(creds, DEFAULT_TOKEN_URL.to_owned(), DEFAULT_API_BASE.to_owned())
    }

    /// A source talking to the given token endpoint and API base (a fake in tests).
    #[must_use]
    pub fn with_endpoints(creds: Credentials, token_url: String, api_base: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            creds,
            token_url,
            api_base: api_base.trim_end_matches('/').to_owned(),
            token: Mutex::new(None),
        }
    }

    fn cached_token(&self) -> Option<String> {
        let guard = self.token.lock().unwrap_or_else(|e| e.into_inner());
        match &*guard {
            Some((token, expires)) if Instant::now() < *expires => Some(token.clone()),
            Some(_) | None => None,
        }
    }

    async fn access_token(&self) -> Result<String, Error> {
        if let Some(token) = self.cached_token() {
            return Ok(token);
        }
        let response = self
            .http
            .post(&self.token_url)
            .form(&[
                ("client_id", self.creds.client_id.as_str()),
                ("client_secret", self.creds.client_secret.as_str()),
                ("refresh_token", self.creds.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .map_err(transport)?;
        let status = response.status();
        if matches!(status.as_u16(), 400 | 401) {
            return Err(Error::Auth {
                reason: format!("token endpoint answered {status}"),
            });
        }
        if !status.is_success() {
            return Err(Error::Transport {
                reason: format!("token endpoint answered {status}"),
            });
        }
        let body: TokenResponse = response.json().await.map_err(malformed)?;
        let lifetime = Duration::from_secs(body.expires_in.unwrap_or(0));
        let expires = Instant::now() + lifetime.saturating_sub(EXPIRY_MARGIN);
        *self.token.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((body.access_token.clone(), expires));
        Ok(body.access_token)
    }

    /// GETs `url` with the bearer token and decodes the JSON body.
    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        query: &[(&str, &str)],
    ) -> Result<T, Error> {
        let token = self.access_token().await?;
        let response = self
            .http
            .get(url)
            .bearer_auth(token)
            .query(query)
            .send()
            .await
            .map_err(transport)?;
        let status = response.status();
        if matches!(status.as_u16(), 401 | 403) {
            return Err(Error::Auth {
                reason: format!("gmail answered {status}"),
            });
        }
        if !status.is_success() {
            return Err(Error::Transport {
                reason: format!("gmail answered {status}"),
            });
        }
        response.json().await.map_err(malformed)
    }
}

fn transport(e: reqwest::Error) -> Error {
    Error::Transport {
        reason: e.without_url().to_string(),
    }
}

fn malformed(e: reqwest::Error) -> Error {
    Error::Malformed {
        reason: e.without_url().to_string(),
    }
}

impl MailSource for GmailSource {
    fn list<'a>(
        &'a self,
        query: &'a str,
        page: Option<&'a PageToken>,
    ) -> BoxFuture<'a, Result<Page, Error>> {
        Box::pin(async move {
            let url = format!("{}/gmail/v1/users/me/messages", self.api_base);
            let mut params = vec![("q", query), ("maxResults", MAX_RESULTS)];
            if let Some(page) = page {
                params.push(("pageToken", page.as_str()));
            }
            let body: ListResponse = self.get_json(&url, &params).await?;
            let ids = body
                .messages
                .into_iter()
                .map(|m| MessageId::new(m.id))
                .collect::<Result<Vec<_>, _>>()?;
            let next = body
                .next_page_token
                .filter(|t| !t.is_empty())
                .map(PageToken::new);
            Ok(Page { ids, next })
        })
    }

    fn fetch_raw<'a>(&'a self, id: &'a MessageId) -> BoxFuture<'a, Result<Vec<u8>, Error>> {
        Box::pin(async move {
            let url = format!(
                "{}/gmail/v1/users/me/messages/{}",
                self.api_base,
                percent_encode(id.as_str())
            );
            let body: RawResponse = self.get_json(&url, &[("format", "raw")]).await?;
            decode_base64url(&body.raw)
        })
    }
}

/// Escapes everything but unreserved characters, so an id is always one path segment.
fn percent_encode(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Decodes base64url (RFC 4648 §5), padded or not.
fn decode_base64url(text: &str) -> Result<Vec<u8>, Error> {
    let bad = |reason: &str| Error::Malformed {
        reason: format!("raw is not base64url: {reason}"),
    };
    let trimmed = text.trim_end_matches('=');
    if text.len() - trimmed.len() > 2 || trimmed.len() % 4 == 1 {
        return Err(bad("bad length"));
    }
    let mut out = Vec::with_capacity(trimmed.len() / 4 * 3 + 2);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for byte in trimmed.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return Err(bad("invalid character")),
        };
        acc = (acc << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xff).map_err(|_| bad("overflow"))?);
        }
    }
    Ok(out)
}

/// The Gmail configuration read from the environment. `Debug` redacts secrets.
#[derive(Clone)]
pub struct Config {
    credentials: Credentials,
    label: String,
    token_url: Option<String>,
    api_base: Option<String>,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("credentials", &self.credentials)
            .field("label", &self.label)
            .field("token_url", &self.token_url)
            .field("api_base", &self.api_base)
            .finish()
    }
}

impl Config {
    /// Reads `HAUZ_GMAIL_*` through `get`. No `HAUZ_GMAIL_CLIENT_ID` means Gmail is not
    /// configured (`Ok(None)`); otherwise `_CLIENT_SECRET`, `_REFRESH_TOKEN` and `_LABEL` are
    /// required, and `_TOKEN_URL` / `_API_BASE` optionally override Google's endpoints.
    ///
    /// # Errors
    /// `Error::Config { variable }` naming the first missing (or empty) required variable.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Result<Option<Self>, Error> {
        let Some(client_id) = get("HAUZ_GMAIL_CLIENT_ID").filter(|v| !v.is_empty()) else {
            return Ok(None);
        };
        let require = |variable: &str| {
            get(variable)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| Error::Config {
                    variable: variable.to_owned(),
                })
        };
        Ok(Some(Self {
            credentials: Credentials {
                client_id,
                client_secret: require("HAUZ_GMAIL_CLIENT_SECRET")?,
                refresh_token: require("HAUZ_GMAIL_REFRESH_TOKEN")?,
            },
            label: require("HAUZ_GMAIL_LABEL")?,
            token_url: get("HAUZ_GMAIL_TOKEN_URL"),
            api_base: get("HAUZ_GMAIL_API_BASE"),
        }))
    }

    /// The Gmail search query selecting the configured label, `label:<label>` verbatim.
    #[must_use]
    pub fn query(&self) -> String {
        format!("label:{}", self.label)
    }

    /// A [`GmailSource`] for these credentials and endpoints.
    #[must_use]
    pub fn source(&self) -> GmailSource {
        GmailSource::with_endpoints(
            self.credentials.clone(),
            self.token_url
                .clone()
                .unwrap_or_else(|| DEFAULT_TOKEN_URL.to_owned()),
            self.api_base
                .clone()
                .unwrap_or_else(|| DEFAULT_API_BASE.to_owned()),
        )
    }
}

/// The result of ingesting one listed message.
#[derive(Debug)]
pub struct Fetched {
    /// The Gmail id of the message.
    pub id: MessageId,
    /// Its ingest outcome; `Err` only for malformed input (`Email`, `Extract`).
    pub outcome: Result<Outcome, ingest::Error>,
}

/// Lists every page of `query`, then downloads and ingests each message in listing order.
/// A malformed message is recorded in its [`Fetched`] and the run continues.
///
/// # Errors
/// The source's error on any list or download failure, or `Error::Ingest` when the store or
/// bill construction fails. Bills ingested before the failure stay stored.
pub async fn fetch(
    source: &dyn MailSource,
    query: &str,
    extractor: &dyn Extractor,
    store: &dyn BillStore,
) -> Result<Vec<Fetched>, Error> {
    let mut ids = Vec::new();
    let mut page: Option<PageToken> = None;
    loop {
        let listed = source.list(query, page.as_ref()).await?;
        ids.extend(listed.ids);
        match listed.next {
            Some(next) => page = Some(next),
            None => break,
        }
    }

    let mut fetched = Vec::with_capacity(ids.len());
    for id in ids {
        let raw = source.fetch_raw(&id).await?;
        let outcome = match ingest(&raw, extractor, store).await {
            Ok(outcome) => Ok(outcome),
            Err(e @ (ingest::Error::Email(_) | ingest::Error::Extract(_))) => Err(e),
            Err(source) => return Err(Error::Ingest { id, source }),
        };
        fetched.push(Fetched { id, outcome });
    }
    Ok(fetched)
}
