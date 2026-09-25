//! Owns MIME decoding: [`Envelope::parse`] turns a raw RFC 5322 message into subject,
//! sender, date, text/HTML body, and one [`Document`] per attachment, however deeply the
//! multipart tree nests. Malformed input is an `Err`, never a panic. No recursion into
//! `message/rfc822` parts: a forwarded message is one `Document` the caller may parse again.

use mail_parser::{Message, MessageParser, MessagePart, MimeHeaders};
use time::OffsetDateTime;

/// Errors from parsing a raw email into an [`Envelope`], or a raw string into a
/// [`MimeType`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The raw bytes have no header section `mail-parser` can find.
    #[error("malformed message: no header section")]
    Malformed,
    /// Neither `From` nor `Sender` carries an addr-spec.
    #[error("no sender address in From or Sender")]
    MissingSender,
    /// A value passed to [`MimeType::new`] is not a bare `type/subtype` string.
    #[error("invalid mime type: {0}")]
    InvalidMimeType(String),
}

/// A `type/subtype` MIME type: exactly one `/`, both halves non-empty, no parameters,
/// stored lowercased.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MimeType(String);

impl MimeType {
    /// Validates `value` as a bare `type/subtype` string and lowercases it.
    ///
    /// # Errors
    /// Returns [`Error::InvalidMimeType`] unless `value` has exactly one `/`, both halves
    /// non-empty, and no parameters (no whitespace, no `;`).
    pub fn new(value: &str) -> Result<Self, Error> {
        let invalid = || Error::InvalidMimeType(value.to_string());

        let mut parts = value.split('/');
        let ty = parts.next().ok_or_else(invalid)?;
        let subtype = parts.next().ok_or_else(invalid)?;
        if parts.next().is_some() {
            return Err(invalid());
        }
        if ty.is_empty() || subtype.is_empty() {
            return Err(invalid());
        }
        if !value.is_ascii() || value.contains(char::is_whitespace) || value.contains(';') {
            return Err(invalid());
        }

        Ok(Self(value.to_ascii_lowercase()))
    }

    /// The `type/subtype` string, lowercased.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The fallback mime type for a `Content-Type` value [`MimeType::new`] rejects.
    fn octet_stream() -> Self {
        Self("application/octet-stream".to_string())
    }
}

impl std::fmt::Display for MimeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for MimeType {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

/// One attachment: its mime type, filename (if any), and decoded bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// The part's `Content-Type`, or `application/octet-stream` when that value is not a
    /// bare `type/subtype` string.
    pub mime: MimeType,
    /// The attachment's filename, from `Content-Disposition` or `Content-Type`.
    pub filename: Option<String>,
    /// The decoded bytes (base64 and quoted-printable already applied).
    pub bytes: Vec<u8>,
}

/// A parsed email: header fields, both bodies, and every attachment in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// The `Subject` header, if present.
    pub subject: Option<String>,
    /// The bare addr-spec of `From`, else `Sender`.
    pub sender: String,
    /// The `Date` header as a UTC instant, `None` unless it parses to a valid date.
    pub date: Option<OffsetDateTime>,
    /// The plain-text body: the first `text/plain` part, or the first HTML part
    /// converted to text when there is no `text/plain` part.
    pub text: Option<String>,
    /// The first HTML part, if any.
    pub html: Option<String>,
    /// Every non-body part across the multipart tree, in document order.
    pub documents: Vec<Document>,
}

impl Envelope {
    /// Parses a raw RFC 5322 message.
    ///
    /// # Errors
    /// [`Error::Malformed`] when the bytes have no header section; [`Error::MissingSender`]
    /// when neither `From` nor `Sender` carries an addr-spec.
    pub fn parse(raw: &[u8]) -> Result<Self, Error> {
        let message = MessageParser::default()
            .parse(raw)
            .ok_or(Error::Malformed)?;

        let sender = sender_of(&message).ok_or(Error::MissingSender)?;
        let subject = message.subject().map(str::to_string);
        let date = message
            .date()
            .filter(|d| d.is_valid())
            .and_then(|d| OffsetDateTime::from_unix_timestamp(d.to_timestamp()).ok());
        let text = message.body_text(0).map(|body| body.into_owned());
        let html = html_of(&message);
        let documents = message.attachments().map(document_of).collect();

        Ok(Self {
            subject,
            sender,
            date,
            text,
            html,
            documents,
        })
    }
}

/// The first genuine HTML part's contents, or `None` when the message has no `text/html`
/// part (unlike `body_text`, this never converts a plain-text body into HTML: a single
/// inline part satisfies mail-parser's internal "need html body" bookkeeping too, which
/// would otherwise surface a synthesized HTML rendering of a plain-text-only message).
fn html_of(message: &Message<'_>) -> Option<String> {
    let part = message.html_part(0)?;
    if part.is_content_type("text", "html") {
        message.body_html(0).map(|body| body.into_owned())
    } else {
        None
    }
}

/// The bare addr-spec of `From`, else `Sender`.
fn sender_of(message: &Message<'_>) -> Option<String> {
    addr_spec(message.from())
        .or_else(|| addr_spec(message.sender()))
        .map(str::to_string)
}

/// The first addr-spec in an address field, if any.
fn addr_spec<'a>(address: Option<&'a mail_parser::Address<'a>>) -> Option<&'a str> {
    address
        .and_then(mail_parser::Address::first)
        .and_then(mail_parser::Addr::address)
}

/// Maps one `attachments()` part to a [`Document`].
fn document_of(part: &MessagePart<'_>) -> Document {
    Document {
        mime: mime_of(part),
        filename: part.attachment_name().map(str::to_string),
        bytes: part.contents().to_vec(),
    }
}

/// The part's `Content-Type` as a [`MimeType`]: default `text/plain` when absent, else
/// `application/octet-stream` when the header value is not a bare `type/subtype` string.
fn mime_of(part: &MessagePart<'_>) -> MimeType {
    let raw = part.content_type().map_or_else(
        || "text/plain".to_string(),
        |ct| match ct.subtype() {
            Some(subtype) => format!("{}/{subtype}", ct.ctype()),
            None => ct.ctype().to_string(),
        },
    );
    MimeType::new(&raw).unwrap_or_else(|_| MimeType::octet_stream())
}
