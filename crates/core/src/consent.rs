//! The one-time Gmail OAuth consent for a "Desktop app" client (loopback redirect + PKCE):
//! [`authorize`] binds `127.0.0.1`, shows the consent URL, waits for the browser's redirect,
//! checks `state`, and exchanges the code for a refresh token. Reasons never carry secrets,
//! codes or tokens.

use std::fmt;
use std::io::Read;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::mail::{DEFAULT_TOKEN_URL, encode_base64url};

const DEFAULT_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";
/// The longest request head read from the browser.
const MAX_REQUEST: usize = 8192;

/// `consent`'s errors. Reasons never carry secrets, codes or tokens.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A required environment variable is missing, empty or invalid.
    #[error("missing or invalid configuration: {variable}")]
    Config {
        /// The variable name.
        variable: String,
    },
    /// The entropy source or the loopback socket failed.
    #[error("i/o error: {reason}")]
    Io {
        /// What went wrong.
        reason: String,
    },
    /// The redirect's `state` is not the one that was sent.
    #[error("state mismatch: the redirect did not come from this consent request")]
    StateMismatch,
    /// The user (or Google) denied the consent.
    #[error("consent denied: {reason}")]
    Denied {
        /// The `error` parameter of the redirect.
        reason: String,
    },
    /// The token endpoint rejected the exchange.
    #[error("auth rejected: {reason}")]
    Auth {
        /// What the server answered.
        reason: String,
    },
    /// The token request failed, or the server answered a non-auth error status.
    #[error("transport error: {reason}")]
    Transport {
        /// What went wrong.
        reason: String,
    },
    /// The response was not what the API documents (for instance no `refresh_token`).
    #[error("malformed response: {reason}")]
    Malformed {
        /// What was wrong with it.
        reason: String,
    },
}

/// OAuth client configuration for the consent. `Debug` redacts the secret.
#[derive(Clone)]
pub struct Config {
    client_id: String,
    client_secret: String,
    auth_url: Option<String>,
    token_url: Option<String>,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .field("auth_url", &self.auth_url)
            .field("token_url", &self.token_url)
            .finish()
    }
}

impl Config {
    /// Reads `HAUZ_GMAIL_CLIENT_ID` and `_CLIENT_SECRET` (both required, non-empty) and the
    /// optional `_AUTH_URL` / `_TOKEN_URL` overrides through `get`.
    ///
    /// # Errors
    /// `Error::Config { variable }` naming the first missing (or empty) required variable.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Result<Self, Error> {
        let require = |variable: &str| {
            get(variable)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| Error::Config {
                    variable: variable.to_owned(),
                })
        };
        Ok(Self {
            client_id: require("HAUZ_GMAIL_CLIENT_ID")?,
            client_secret: require("HAUZ_GMAIL_CLIENT_SECRET")?,
            auth_url: get("HAUZ_GMAIL_AUTH_URL"),
            token_url: get("HAUZ_GMAIL_TOKEN_URL"),
        })
    }
}

/// 64 random bytes from the operating system (`/dev/urandom`).
///
/// # Errors
/// `Error::Io` when the device cannot be read.
pub fn entropy() -> Result<[u8; 64], Error> {
    let mut bytes = [0u8; 64];
    std::fs::File::open("/dev/urandom")
        .map_err(Error::from)?
        .read_exact(&mut bytes)
        .map_err(Error::from)?;
    Ok(bytes)
}

/// The PKCE S256 challenge of `verifier`: base64url (no padding) of its SHA-256.
#[must_use]
pub fn pkce_challenge(verifier: &str) -> String {
    encode_base64url(Sha256::digest(verifier.as_bytes()).as_slice())
}

#[derive(Deserialize)]
struct TokenResponse {
    refresh_token: Option<String>,
}

/// What the browser's redirect carried.
enum Redirect {
    /// No `code` / `error`: not the redirect (a favicon request); keep waiting.
    Other,
    Code {
        state: Option<String>,
        code: String,
    },
    Error {
        state: Option<String>,
        reason: String,
    },
}

/// Runs the consent: binds `127.0.0.1:0`, calls `show` once with the consent URL, awaits the
/// redirect, and exchanges the code (PKCE S256) for a refresh token.
///
/// # Errors
/// `Error::Config` for an unparsable endpoint override, `Error::Io` for a socket failure,
/// `Error::StateMismatch` / `Error::Denied` from the redirect, `Error::Auth` when the token
/// endpoint answers 400/401, `Error::Transport` for other failures, `Error::Malformed` when
/// the answer carries no `refresh_token`.
pub async fn authorize(
    config: &Config,
    entropy: [u8; 64],
    show: impl FnOnce(&str),
) -> Result<String, Error> {
    let (state_bytes, verifier_bytes) = entropy.split_at(16);
    let state = encode_base64url(state_bytes);
    let verifier = encode_base64url(verifier_bytes);

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(Error::from)?;
    let redirect_uri = format!(
        "http://127.0.0.1:{}",
        listener.local_addr().map_err(Error::from)?.port()
    );
    let challenge = pkce_challenge(&verifier);
    let url = reqwest::Url::parse_with_params(
        config.auth_url.as_deref().unwrap_or(DEFAULT_AUTH_URL),
        [
            ("client_id", config.client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("state", state.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
        ],
    )
    .map_err(|_| Error::Config {
        variable: "HAUZ_GMAIL_AUTH_URL".to_owned(),
    })?;
    show(url.as_str());

    let code = await_code(&listener, &state).await?;
    exchange(config, &code, &redirect_uri, &verifier).await
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io {
            reason: e.to_string(),
        }
    }
}

/// Serves the loopback until the redirect arrives; returns its code if `state` matches.
async fn await_code(listener: &TcpListener, state: &str) -> Result<String, Error> {
    loop {
        let (mut stream, _) = listener.accept().await.map_err(Error::from)?;
        let redirect = read_redirect(&mut stream).await.unwrap_or(Redirect::Other);
        let (sent, result) = match redirect {
            Redirect::Other => {
                respond(&mut stream, "404 Not Found", "Not found.").await;
                continue;
            }
            Redirect::Code { state: got, .. } | Redirect::Error { state: got, .. }
                if got.as_deref() != Some(state) =>
            {
                ("400 Bad Request", Err(Error::StateMismatch))
            }
            Redirect::Error { reason, .. } => ("400 Bad Request", Err(Error::Denied { reason })),
            Redirect::Code { code, .. } => ("200 OK", Ok(code)),
        };
        let body = if result.is_ok() {
            "Consent received. You can close this tab."
        } else {
            "Consent failed. See the terminal."
        };
        respond(&mut stream, sent, body).await;
        return result;
    }
}

/// Reads the request line and extracts the redirect's parameters.
async fn read_redirect(stream: &mut TcpStream) -> Result<Redirect, std::io::Error> {
    let mut head = Vec::new();
    let mut chunk = [0u8; 1024];
    while !head.windows(2).any(|w| w == b"\r\n") && head.len() < MAX_REQUEST {
        let read = stream.read(&mut chunk).await?;
        let Some(bytes) = chunk.get(..read).filter(|b| !b.is_empty()) else {
            break;
        };
        head.extend_from_slice(bytes);
    }
    let text = String::from_utf8_lossy(&head);
    let target = text
        .lines()
        .next()
        .filter(|line| line.starts_with("GET "))
        .and_then(|line| line.split_whitespace().nth(1));
    let Some(url) = target.and_then(|t| reqwest::Url::parse(&format!("http://127.0.0.1{t}")).ok())
    else {
        return Ok(Redirect::Other);
    };
    let param = |name: &str| {
        url.query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
    };
    let state = param("state");
    Ok(match (param("error"), param("code")) {
        (Some(reason), _) => Redirect::Error { state, reason },
        (None, Some(code)) => Redirect::Code { state, code },
        (None, None) => Redirect::Other,
    })
}

async fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    // The browser may already have gone away; the consent outcome does not depend on it.
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

async fn exchange(
    config: &Config,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<String, Error> {
    let response = reqwest::Client::new()
        .post(config.token_url.as_deref().unwrap_or(DEFAULT_TOKEN_URL))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|e| Error::Transport {
            reason: e.without_url().to_string(),
        })?;
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
    let body: TokenResponse = response.json().await.map_err(|e| Error::Malformed {
        reason: e.without_url().to_string(),
    })?;
    body.refresh_token
        .filter(|t| !t.is_empty())
        .ok_or_else(|| Error::Malformed {
            reason: "token response has no refresh_token".to_owned(),
        })
}
