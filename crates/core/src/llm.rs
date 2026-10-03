//! The LLM-side system edges: a chat client, a PDF page rasterizer, and env configuration.
//! Each is an injectable trait so other modules (and tests) never depend on a live model, a
//! real `pdftoppm` binary, or the process environment. The extractor that wires these
//! together is a later feature (#26); this module only owns the edges.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::BoxFuture;
use crate::bill::{BillingPeriod, Currency, Money, Vendor};
use crate::email::Envelope;
use crate::extract::{
    self, Confidence, Error as ExtractError, Extraction, Extractor, Field, Note, PdfTextExtractor,
    Source, Span,
};

/// One piece of a request sent to an [`LlmClient`]: plain instruction text, a rasterized
/// page, or a whole PDF document. Which kinds a given provider accepts is a property of the
/// client, not of this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Plain text.
    Text(String),
    /// A PNG image, e.g. one rasterized page.
    Png(Vec<u8>),
    /// A whole PDF document.
    Pdf(Vec<u8>),
}

/// A request to an [`LlmClient`]: a system instruction, the content parts, and the JSON
/// schema the reply must conform to.
#[derive(Debug, Clone)]
pub struct LlmRequest {
    /// The system instruction (preamble).
    pub instructions: String,
    /// The content parts, in order.
    pub parts: Vec<Part>,
    /// The JSON schema the reply must conform to.
    pub schema: schemars::Schema,
}

/// A chat completion client: the only async system edge an LLM pass needs. `dyn`-safe, like
/// [`crate::store::BillStore`] and [`crate::extract::Extractor`].
pub trait LlmClient: Send + Sync {
    /// Sends `req` and returns the reply's raw text (not yet parsed or validated).
    fn complete<'a>(&'a self, req: &'a LlmRequest) -> BoxFuture<'a, Result<String, Error>>;
}

/// Which chat provider a [`RigClient`] talks to, and the settings it needs. `Debug` is
/// manual: a credential never appears in a log.
pub enum Provider {
    /// A local or self-hosted Ollama daemon.
    Ollama {
        /// The daemon's address, e.g. `http://localhost:11434`.
        base_url: String,
    },
    /// Anthropic's API.
    Anthropic {
        /// The API key.
        api_key: String,
        /// An override of Anthropic's default base URL.
        base_url: Option<String>,
    },
    /// OpenAI's API.
    OpenAi {
        /// The API key.
        api_key: String,
        /// An override of OpenAI's default base URL.
        base_url: Option<String>,
    },
}

impl fmt::Debug for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ollama { base_url } => f
                .debug_struct("Ollama")
                .field("base_url", base_url)
                .finish(),
            Self::Anthropic { base_url, .. } => f
                .debug_struct("Anthropic")
                .field("api_key", &"<redacted>")
                .field("base_url", base_url)
                .finish(),
            Self::OpenAi { base_url, .. } => f
                .debug_struct("OpenAi")
                .field("api_key", &"<redacted>")
                .field("base_url", base_url)
                .finish(),
        }
    }
}

/// The concrete model a [`Provider`] and model id build, kept behind one variant per
/// provider: rig gives each provider its own wire type, so there is no single concrete
/// `Model<_>` to store generically.
enum Inner {
    Ollama(Box<rig_core::driver::Model<rig_core::providers::ollama::wire::Chat>>),
    Anthropic(Box<rig_core::driver::Model<rig_core::providers::anthropic::wire::Messages>>),
    OpenAi(Box<rig_core::driver::Model<rig_core::providers::openai::wire::OpenAiWire>>),
}

/// An [`LlmClient`] backed by rig-core. The only code in this crate importing rig.
pub struct RigClient {
    inner: Inner,
}

impl fmt::Debug for RigClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let provider = match &self.inner {
            Inner::Ollama(_) => "ollama",
            Inner::Anthropic(_) => "anthropic",
            Inner::OpenAi(_) => "openai",
        };
        f.debug_struct("RigClient")
            .field("provider", &provider)
            .finish()
    }
}

impl RigClient {
    /// A client for `provider`, addressing `model`.
    #[must_use]
    pub fn new(provider: Provider, model: &str) -> Self {
        use rig_core::providers::{anthropic, ollama, openai};

        let inner = match provider {
            Provider::Ollama { base_url } => {
                let client = ollama::wire::OllamaConfig::new()
                    .with_base_url(base_url)
                    .client();
                Inner::Ollama(Box::new(client.completion(model)))
            }
            Provider::Anthropic { api_key, base_url } => {
                let mut config = anthropic::wire::AnthropicConfig::new(api_key);
                if let Some(base_url) = base_url {
                    config = config.with_base_url(base_url);
                }
                Inner::Anthropic(Box::new(config.client().completion(model)))
            }
            Provider::OpenAi { api_key, base_url } => {
                let mut config = openai::wire::OpenAIConfig::new(api_key);
                if let Some(base_url) = base_url {
                    config = config.with_base_url(base_url);
                }
                Inner::OpenAi(Box::new(config.client().completion(model)))
            }
        };
        Self { inner }
    }

    /// `true` for the Ollama provider, which rejects whole-PDF parts before any network call.
    fn is_ollama(&self) -> bool {
        matches!(self.inner, Inner::Ollama(_))
    }
}

/// `req`'s parts as rig `UserContent`, one per part.
fn to_user_content(parts: &[Part]) -> Vec<rig_core::completion::message::UserContent> {
    use rig_core::completion::message::{
        Document, DocumentMediaType, DocumentSourceKind, Image, ImageMediaType, Text, UserContent,
    };

    parts
        .iter()
        .map(|part| match part {
            Part::Text(text) => UserContent::Text(Text::new(text)),
            Part::Png(bytes) => UserContent::Image(Image {
                data: DocumentSourceKind::base64(&base64_encode(bytes)),
                media_type: Some(ImageMediaType::PNG),
                detail: None,
                additional_params: None,
            }),
            Part::Pdf(bytes) => UserContent::Document(Document {
                data: DocumentSourceKind::base64(&base64_encode(bytes)),
                media_type: Some(DocumentMediaType::PDF),
                additional_params: None,
            }),
        })
        .collect()
}

impl LlmClient for RigClient {
    fn complete<'a>(&'a self, req: &'a LlmRequest) -> BoxFuture<'a, Result<String, Error>> {
        Box::pin(async move {
            if self.is_ollama() && req.parts.iter().any(|p| matches!(p, Part::Pdf(_))) {
                return Err(Error::Unsupported {
                    part: "pdf".to_owned(),
                    provider: "ollama".to_owned(),
                });
            }

            use rig_core::completion::CompletionRequest;
            use rig_core::completion::message::Message;

            let message = Message::User {
                content: to_user_content(&req.parts),
            };
            let request = CompletionRequest::new(message)
                .preamble(req.instructions.clone())
                .output_schema(Some(req.schema.clone()));

            let text = match &self.inner {
                Inner::Ollama(model) => model
                    .call(request)
                    .await
                    .map_err(|e| client_error(&e))?
                    .text(),
                Inner::Anthropic(model) => model
                    .call(request)
                    .await
                    .map_err(|e| client_error(&e))?
                    .text(),
                Inner::OpenAi(model) => model
                    .call(request)
                    .await
                    .map_err(|e| client_error(&e))?
                    .text(),
            };
            Ok(text)
        })
    }
}

fn client_error(e: &rig_core::error::ProviderError) -> Error {
    Error::Client {
        message: e.to_string(),
    }
}

/// Rasterizes a PDF's pages to PNG, one buffer per page, in page order.
pub trait Rasterizer: Send + Sync {
    /// Renders at most `max_pages` pages of `pdf`, in page order.
    fn rasterize(&self, pdf: &[u8], max_pages: u8) -> Result<Vec<Vec<u8>>, Error>;
}

/// A [`Rasterizer`] shelling out to poppler's `pdftoppm`.
#[derive(Debug, Clone)]
pub struct Pdftoppm {
    program: PathBuf,
    dpi: u16,
}

impl Pdftoppm {
    /// `pdftoppm` resolved from `PATH`, rendering at `dpi` dots per inch.
    #[must_use]
    pub fn new(dpi: u16) -> Self {
        Self {
            program: PathBuf::from("pdftoppm"),
            dpi,
        }
    }

    /// Like [`Self::new`], but runs `program` instead of resolving `pdftoppm` from `PATH`.
    #[must_use]
    pub fn with_program(program: PathBuf, dpi: u16) -> Self {
        Self { program, dpi }
    }
}

/// A fresh, unique directory under the system temp dir.
fn unique_tmp_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("hauz-core-rasterize-{nanos}-{n}"))
}

impl Rasterizer for Pdftoppm {
    fn rasterize(&self, pdf: &[u8], max_pages: u8) -> Result<Vec<Vec<u8>>, Error> {
        let dir = unique_tmp_dir();
        let result = rasterize_in(&self.program, self.dpi, pdf, max_pages, &dir);
        let _ = fs::remove_dir_all(&dir);
        result
    }
}

/// [`Pdftoppm::rasterize`]'s body, run inside `dir` (a fresh directory the caller owns and
/// removes).
fn rasterize_in(
    program: &PathBuf,
    dpi: u16,
    pdf: &[u8],
    max_pages: u8,
    dir: &PathBuf,
) -> Result<Vec<Vec<u8>>, Error> {
    fs::create_dir_all(dir).map_err(|e| rasterizer_err(format!("creating temp dir: {e}")))?;
    let in_path = dir.join("in.pdf");
    fs::write(&in_path, pdf).map_err(|e| rasterizer_err(format!("writing input: {e}")))?;
    let prefix = dir.join("page");

    let output = Command::new(program)
        .arg("-r")
        .arg(dpi.to_string())
        .arg("-f")
        .arg("1")
        .arg("-l")
        .arg(max_pages.to_string())
        .arg("-png")
        .arg(&in_path)
        .arg(&prefix)
        .output()
        .map_err(|e| rasterizer_err(format!("running pdftoppm: {e}")))?;

    if !output.status.success() {
        return Err(rasterizer_err(format!(
            "pdftoppm exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let mut pages = Vec::new();
    for n in 1..=max_pages {
        let page_path = dir.join(format!("page-{n}.png"));
        if !page_path.exists() {
            break;
        }
        pages.push(fs::read(&page_path).map_err(|e| rasterizer_err(format!("reading {n}: {e}")))?);
    }
    if pages.is_empty() {
        return Err(rasterizer_err("pdftoppm produced no output".to_owned()));
    }
    Ok(pages)
}

fn rasterizer_err(reason: String) -> Error {
    Error::Rasterizer { reason }
}

/// How a PDF attachment is delivered to the model: rendered to page images, or sent whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfDelivery {
    /// One [`Part::Png`] per rasterized page, via [`LlmExtractor`]'s [`Rasterizer`].
    RasterizedPages,
    /// One [`Part::Pdf`] carrying the whole document; the rasterizer is never called.
    Native,
}

/// Tunables for [`LlmExtractor`]. `Default` is [`PdfDelivery::RasterizedPages`], 4 pages, a
/// confidence ceiling of 70, and a 20 000-char body truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlmOptions {
    /// How PDF attachments are delivered to the model.
    pub delivery: PdfDelivery,
    /// The most pages rasterized per PDF (only when `delivery` is `RasterizedPages`).
    pub max_pages: u8,
    /// A ceiling applied to the model's self-reported confidence: every mapped field's
    /// [`Confidence`] is `min(reported, max_confidence)`.
    pub max_confidence: u8,
    /// The body text (plain or tag-stripped HTML) is truncated to this many `char`s before
    /// being sent.
    pub max_body_chars: usize,
}

impl Default for LlmOptions {
    fn default() -> Self {
        Self {
            delivery: PdfDelivery::RasterizedPages,
            max_pages: 4,
            max_confidence: 70,
            max_body_chars: 20_000,
        }
    }
}

/// The system instruction sent with every request: names the fields, the expected formats,
/// and asks for an honest confidence rather than a confident-sounding guess.
const INSTRUCTIONS: &str = "Extract billing data from the attached email body and/or \
document into JSON matching the schema exactly. Fields: vendor (the billing company's name); \
amount_minor_units (the total due, as an integer in the currency's smallest unit, e.g. \
cents); currency (a 3-letter ISO-4217 code — resolve a bare symbol like \"$\" from the \
surrounding country or language cues rather than assuming USD); period_start and period_end \
(the billing period's first and last day, ISO-8601 dates); issued (the invoice/issue date, \
ISO-8601); due (the payment due date, ISO-8601). Use JSON null for any field you cannot find \
with confidence — never guess a value you are not reasonably sure of. Report your own \
confidence in 0-100 honestly: near 100 for a clearly stated field, low for a guess.";

/// The private JSON schema a model reply is validated against: every field `required` (but
/// still nullable), so a model that omits a key fails validation rather than silently
/// dropping a field we would otherwise treat as "found".
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
struct LlmFields {
    #[schemars(required)]
    vendor: Option<String>,
    #[schemars(required)]
    amount_minor_units: Option<i64>,
    #[schemars(required)]
    currency: Option<String>,
    #[schemars(required)]
    period_start: Option<String>,
    #[schemars(required)]
    period_end: Option<String>,
    #[schemars(required)]
    issued: Option<String>,
    #[schemars(required)]
    due: Option<String>,
    #[schemars(required)]
    confidence: u8,
}

/// An [`Extractor`] backed by an [`LlmClient`]: sends the envelope's body and PDF attachments
/// (rendered per [`LlmOptions::delivery`]) to the model with a JSON schema, and maps the
/// reply into an [`Extraction`]. Behind [`extract::Escalate`], this is the pass that runs only
/// when a cheaper extractor leaves a bill short of complete.
pub struct LlmExtractor {
    client: Box<dyn LlmClient>,
    rasterizer: Box<dyn Rasterizer>,
    options: LlmOptions,
}

impl fmt::Debug for LlmExtractor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LlmExtractor")
            .field("options", &self.options)
            .finish()
    }
}

impl LlmExtractor {
    /// Wraps `client` and `rasterizer` behind the [`Extractor`] trait, tuned by `options`.
    #[must_use]
    pub fn new(
        client: Box<dyn LlmClient>,
        rasterizer: Box<dyn Rasterizer>,
        options: LlmOptions,
    ) -> Self {
        Self {
            client,
            rasterizer,
            options,
        }
    }

    /// Builds the request's parts, in order: the body text (if any), then — per
    /// `application/pdf` attachment, other mime types skipped — its rendering (rasterized
    /// pages or the whole document, per [`LlmOptions::delivery`]) followed by its text layer
    /// when one is present. A rasterizer failure degrades to
    /// [`extract::Error::Llm`]; a corrupt PDF's text layer propagates
    /// [`extract::Error::Pdf`] exactly as [`PdfTextExtractor`] would.
    fn build_parts(&self, envelope: &Envelope) -> Result<Vec<Part>, ExtractError> {
        let mut parts = Vec::new();
        let body = body_text(envelope, self.options.max_body_chars);
        if !body.is_empty() {
            parts.push(Part::Text(body));
        }

        for (index, document) in envelope.documents.iter().enumerate() {
            if document.mime.as_str() != extract::PDF_MIME {
                continue;
            }
            match self.options.delivery {
                PdfDelivery::RasterizedPages => {
                    let pages = self
                        .rasterizer
                        .rasterize(&document.bytes, self.options.max_pages)
                        .map_err(ExtractError::Llm)?;
                    parts.extend(pages.into_iter().map(Part::Png));
                }
                PdfDelivery::Native => parts.push(Part::Pdf(document.bytes.clone())),
            }
            match PdfTextExtractor::text_layer(&document.bytes) {
                Ok(Some(layer)) => parts.push(Part::Text(layer)),
                Ok(None) => {}
                Err(ExtractError::Pdf { reason, .. }) => {
                    return Err(ExtractError::Pdf {
                        document: index,
                        reason,
                    });
                }
                Err(err @ ExtractError::InvalidConfidence(_)) => return Err(err),
                // `text_layer` only ever constructs `Error::Pdf`; `InvalidConfidence` and
                // `Llm` cannot occur here, but `extract::Error` is `#[non_exhaustive]` so
                // these arms keep the match exhaustive.
                Err(err @ ExtractError::Llm(_)) => return Err(err),
            }
        }
        Ok(parts)
    }
}

impl Extractor for LlmExtractor {
    /// Builds a request from `envelope` and sends it; an empty request (no body, no PDF
    /// attachments) short-circuits to the default extraction without calling the client. A
    /// client failure degrades to the default plus [`Note::LlmUnavailable`]; an unparseable
    /// reply degrades to the default plus [`Note::LlmMalformed`]. An unsupported
    /// part/provider combination, a rasterizer failure, or a configuration fault is a
    /// genuine error.
    fn extract<'a>(
        &'a self,
        envelope: &'a Envelope,
    ) -> BoxFuture<'a, Result<Extraction, ExtractError>> {
        Box::pin(async move {
            let parts = self.build_parts(envelope)?;
            if parts.is_empty() {
                return Ok(Extraction::default());
            }

            let request = LlmRequest {
                instructions: INSTRUCTIONS.to_owned(),
                parts,
                schema: schemars::schema_for!(LlmFields),
            };

            match self.client.complete(&request).await {
                Ok(reply) => Ok(map_reply(&reply, self.options.max_confidence)),
                Err(Error::Client { .. }) => {
                    let mut extraction = Extraction::default();
                    extraction.notes.insert(Note::LlmUnavailable);
                    Ok(extraction)
                }
                Err(
                    err @ (Error::Unsupported { .. }
                    | Error::Rasterizer { .. }
                    | Error::Config { .. }),
                ) => Err(ExtractError::Llm(err)),
            }
        })
    }
}

/// The body sent as the leading `Part::Text`: `text` verbatim, else tag-stripped `html`,
/// truncated to `max_chars` (by `char`, not byte). Empty (both absent, or empty after
/// truncation) becomes an empty string, so the caller can skip the part entirely.
fn body_text(envelope: &Envelope, max_chars: usize) -> String {
    let full = match (&envelope.text, &envelope.html) {
        (Some(text), _) => text.clone(),
        (None, Some(html)) => extract::strip_html(html),
        (None, None) => String::new(),
    };
    full.chars().take(max_chars).collect()
}

/// Strips an optional leading ```` ```json ```` or ```` ``` ```` fence line and a matching
/// trailing ```` ``` ```` line, as some models wrap an otherwise schema-conformant reply in a
/// markdown code fence.
fn strip_fence(reply: &str) -> &str {
    let trimmed = reply.trim();
    let Some(after_open) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
    else {
        return trimmed;
    };
    let after_open = after_open.trim_start_matches(['\r', '\n']);
    after_open
        .strip_suffix("```")
        .map_or(after_open, str::trim_end)
}

/// `[year]-[month]-[day]`, the only date shape this module accepts from a reply.
const DATE_FORMAT: &[time::format_description::FormatItem<'_>] =
    time::macros::format_description!("[year]-[month]-[day]");

/// Parses `raw` as an ISO-8601 `[year]-[month]-[day]` date, or `None` when it does not match.
fn parse_date(raw: &str) -> Option<time::Date> {
    time::Date::parse(raw, DATE_FORMAT).ok()
}

/// Maps a client's raw reply into an [`Extraction`]: strips a markdown fence and parses the
/// schema JSON; an unparseable reply degrades to the default extraction plus
/// [`Note::LlmMalformed`]. Each present field's [`Confidence`] is the reply's self-reported
/// confidence, capped at `max_confidence`; every field shares one zero-length
/// [`Source::Model`] [`Span`]. A field whose domain constructor rejects the reply's value
/// (an invalid currency, an empty vendor name, an unparseable date, an inverted period)
/// becomes `None` rather than failing the whole extraction.
fn map_reply(reply: &str, max_confidence: u8) -> Extraction {
    let Ok(fields) = rig_core::serde_json::from_str::<LlmFields>(strip_fence(reply)) else {
        let mut extraction = Extraction::default();
        extraction.notes.insert(Note::LlmMalformed);
        return extraction;
    };

    let confidence = Confidence::clamped(fields.confidence.min(max_confidence));
    let span = Span {
        source: Source::Model,
        start: 0,
        end: 0,
    };

    let amount = fields
        .amount_minor_units
        .zip(
            fields
                .currency
                .as_deref()
                .and_then(|currency| Currency::new(currency).ok()),
        )
        .map(|(minor_units, currency)| Field {
            value: Money::new(minor_units, currency),
            confidence,
            span,
        });

    let vendor = fields
        .vendor
        .as_deref()
        .and_then(|name| Vendor::new(name).ok())
        .map(|value| Field {
            value,
            confidence,
            span,
        });

    let period = fields
        .period_start
        .as_deref()
        .and_then(parse_date)
        .zip(fields.period_end.as_deref().and_then(parse_date))
        .and_then(|(start, end)| BillingPeriod::new(start, end).ok())
        .map(|value| Field {
            value,
            confidence,
            span,
        });

    let issued = fields
        .issued
        .as_deref()
        .and_then(parse_date)
        .map(|value| Field {
            value,
            confidence,
            span,
        });

    let due = fields
        .due
        .as_deref()
        .and_then(parse_date)
        .map(|value| Field {
            value,
            confidence,
            span,
        });

    Extraction {
        amount,
        issued,
        due,
        period,
        vendor,
        notes: BTreeSet::new(),
    }
}

/// The resolved LLM configuration: which provider to use, and which model.
#[derive(Debug)]
pub struct Config {
    /// The provider to use.
    pub provider: Provider,
    /// The model id to address.
    pub model: String,
}

const DEFAULT_OLLAMA_BASE_URL: &str = "http://localhost:11434";

impl Config {
    /// Reads the LLM configuration from the environment, through `get` (so tests never touch
    /// the real process environment). `HAUZ_LLM_PROVIDER` unset means no LLM pass is
    /// configured (`Ok(None)`); set to anything else, every other variable it implies becomes
    /// required.
    ///
    /// # Errors
    /// `Error::Config { variable }` for an unrecognized `HAUZ_LLM_PROVIDER` or a missing
    /// required variable.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Result<Option<Self>, Error> {
        let Some(provider_name) = get("HAUZ_LLM_PROVIDER") else {
            return Ok(None);
        };
        let model = require(&get, "HAUZ_LLM_MODEL")?;

        let provider = match provider_name.as_str() {
            "ollama" => Provider::Ollama {
                base_url: get("OLLAMA_API_BASE_URL")
                    .unwrap_or_else(|| DEFAULT_OLLAMA_BASE_URL.to_owned()),
            },
            "anthropic" => Provider::Anthropic {
                api_key: require(&get, "ANTHROPIC_API_KEY")?,
                base_url: get("ANTHROPIC_BASE_URL"),
            },
            "openai" => Provider::OpenAi {
                api_key: require(&get, "OPENAI_API_KEY")?,
                base_url: get("OPENAI_BASE_URL"),
            },
            _ => {
                return Err(Error::Config {
                    variable: "HAUZ_LLM_PROVIDER".to_owned(),
                });
            }
        };

        Ok(Some(Self { provider, model }))
    }
}

/// `get(variable)`, or `Error::Config { variable }` when unset.
fn require(get: &impl Fn(&str) -> Option<String>, variable: &str) -> Result<String, Error> {
    get(variable).ok_or_else(|| Error::Config {
        variable: variable.to_owned(),
    })
}

/// `llm`'s errors: a client failure, an unsupported part/provider combination, a rasterizer
/// failure, or a missing/invalid env variable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The chat client's call failed.
    #[error("llm client error: {message}")]
    Client {
        /// The provider's error message.
        message: String,
    },
    /// `part` is not supported by `provider`.
    #[error("{provider} does not support {part} parts")]
    Unsupported {
        /// The unsupported part kind, e.g. `"pdf"`.
        part: String,
        /// The provider that rejected it, e.g. `"ollama"`.
        provider: String,
    },
    /// The rasterizer failed.
    #[error("rasterizer error: {reason}")]
    Rasterizer {
        /// What went wrong.
        reason: String,
    },
    /// A required env variable was missing, or held an unrecognized value.
    #[error("invalid or missing configuration: {variable}")]
    Config {
        /// The variable name.
        variable: String,
    },
}

/// Minimal base64 encoder (RFC 4648 standard alphabet, with padding) — no external
/// dependency beyond the crate's declared set.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = *chunk.first().unwrap_or(&0);
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        let idx = |shift: u32| {
            ALPHABET
                .get(((n >> shift) & 0x3f) as usize)
                .copied()
                .unwrap_or(b'A') as char
        };
        out.push(idx(18));
        out.push(idx(12));
        out.push(if chunk.len() > 1 { idx(6) } else { '=' });
        out.push(if chunk.len() > 2 { idx(0) } else { '=' });
    }
    out
}
