// Sketch only. core::mail
use futures::future::BoxFuture; // whatever BillStore already uses
pub struct MessageId(String);                 // newtype, non-empty
pub struct Page { pub ids: Vec<MessageId>, pub next: Option<PageToken> }
pub struct PageToken(String);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("auth rejected: {0}")] Auth(String),      // invalid_grant / 401
    #[error("transport: {0}")] Transport(String),
    #[error("malformed response: {0}")] Malformed(String),
}

pub trait MailSource: Send + Sync {
    fn list<'a>(&'a self, query: &'a str, page: Option<&'a PageToken>)
        -> BoxFuture<'a, Result<Page, Error>>;
    fn fetch_raw<'a>(&'a self, id: &'a MessageId) -> BoxFuture<'a, Result<Vec<u8>, Error>>;
}
// default method / free fn `list_all(&dyn MailSource, q)` loops until next == None.

pub struct Credentials { pub client_id: String, pub client_secret: String, pub refresh_token: String } // Debug redacts
pub struct GmailSource { http: reqwest::Client, creds: Credentials, token_url: String, api_base: String }
impl GmailSource {
    pub fn new(creds: Credentials) -> Self { /* oauth2.googleapis.com/token, gmail.googleapis.com */ }
    pub fn with_endpoints(creds: Credentials, token_url: String, api_base: String) -> Self { /*tests*/ }
    // access token fetched per run (one-shot CLI), cached with expires_in if a server loop comes later
}
// list  : GET {api_base}/gmail/v1/users/me/messages?q=..&maxResults=500[&pageToken=..]
//         missing `messages` => empty Vec (serde default)
// fetch : GET {api_base}/gmail/v1/users/me/messages/{id}?format=raw -> {"raw": b64url}
// tests : axum Router on 127.0.0.1:0 serving /token, list, get; assert form fields + Bearer header.
