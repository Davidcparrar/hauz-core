//! [unit] tests for the `llm` module's public API: `Config::from_env`, `RigClient`'s
//! PDF-on-Ollama rejection, `Provider`'s redacted `Debug` (#23), and `LlmExtractor`'s request
//! building and reply mapping (#26). One file per level per module. Test fn names carry the
//! spec criterion they satisfy: `acN_<behavior>`.

mod common;

use common::Result;
use hauz_core::bill::{BillingPeriod, Currency, Money, Vendor};
use hauz_core::email::{Document, Envelope, MimeType};
use hauz_core::extract::{
    Confidence, Error as ExtractError, Extraction, Extractor, Field, Note, Source, Span,
};
use hauz_core::llm::{
    Error, LlmClient, LlmExtractor, LlmOptions, LlmRequest, Part, PdfDelivery, Pdftoppm, Provider,
    Rasterizer, RigClient,
};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use time::macros::date;

/// An env lookup closure over a fixed map, so tests never touch the real process environment.
fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + use<'a> {
    let map: HashMap<&str, &str> = vars.iter().copied().collect();
    move |key: &str| map.get(key).map(|v| (*v).to_owned())
}

#[test]
fn ac1_no_provider_is_none() -> Result<()> {
    let config = hauz_core::llm::Config::from_env(env(&[]))?;
    assert!(config.is_none());
    Ok(())
}

#[test]
fn ac1_ollama_resolves_default_base_url_and_model() -> Result<()> {
    let config = hauz_core::llm::Config::from_env(env(&[
        ("HAUZ_LLM_PROVIDER", "ollama"),
        ("HAUZ_LLM_MODEL", "gemma4:latest"),
    ]))?
    .ok_or("expected Some(Config)")?;
    assert!(matches!(
        config.provider,
        Provider::Ollama { ref base_url } if base_url == "http://localhost:11434"
    ));
    assert_eq!(config.model, "gemma4:latest");
    Ok(())
}

#[test]
fn ac1_anthropic_without_key_errors() {
    let result = hauz_core::llm::Config::from_env(env(&[
        ("HAUZ_LLM_PROVIDER", "anthropic"),
        ("HAUZ_LLM_MODEL", "claude"),
    ]));
    assert!(matches!(
        result,
        Err(Error::Config { ref variable }) if variable == "ANTHROPIC_API_KEY"
    ));
}

#[test]
fn ac1_bogus_provider_errors() {
    let result = hauz_core::llm::Config::from_env(env(&[
        ("HAUZ_LLM_PROVIDER", "bogus"),
        ("HAUZ_LLM_MODEL", "whatever"),
    ]));
    assert!(matches!(
        result,
        Err(Error::Config { ref variable }) if variable == "HAUZ_LLM_PROVIDER"
    ));
}

#[tokio::test]
async fn ac2_ollama_rejects_pdf_part_without_network() -> Result<()> {
    let client = RigClient::new(
        Provider::Ollama {
            base_url: "http://localhost:11434".to_owned(),
        },
        "gemma4:latest",
    );
    let request = LlmRequest {
        instructions: "ignored".to_owned(),
        parts: vec![Part::Pdf(vec![1, 2, 3])],
        schema: schemars::Schema::default(),
    };
    let result = client.complete(&request).await;
    assert_eq!(
        result,
        Err(Error::Unsupported {
            part: "pdf".to_owned(),
            provider: "ollama".to_owned(),
        })
    );
    Ok(())
}

#[test]
fn ac4_missing_program_is_rasterizer_error() -> Result<()> {
    let fixture = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/synthetic_bill.pdf"
    ))?;
    let rasterizer = Pdftoppm::with_program(PathBuf::from("/nonexistent/pdftoppm"), 150);
    let result = rasterizer.rasterize(&fixture, 4);
    assert!(matches!(result, Err(Error::Rasterizer { .. })));
    Ok(())
}

#[test]
fn ac3_anthropic_debug_redacts_api_key() {
    let provider = Provider::Anthropic {
        api_key: "sk-secret".to_owned(),
        base_url: None,
    };
    let rendered = format!("{provider:?}");
    assert!(rendered.contains("<redacted>"));
    assert!(!rendered.contains("sk-secret"));
}

// ---------------------------------------------------------------------------------------
// LlmExtractor (#26): request building (AC1-AC2) and reply mapping (AC3-AC5).
// ---------------------------------------------------------------------------------------

/// Records every request it is sent and always replies with a fixed result.
struct FakeClient {
    reply: core::result::Result<String, Error>,
    seen: Mutex<Vec<LlmRequest>>,
}

impl FakeClient {
    fn new(reply: core::result::Result<String, Error>) -> Self {
        Self {
            reply,
            seen: Mutex::new(Vec::new()),
        }
    }
}

/// A sharable handle on a [`FakeClient`]: implements [`LlmClient`] itself (the orphan rule
/// blocks implementing it directly on `Arc<FakeClient>`) so a test keeps its own clone to
/// inspect `seen` after the `Box<dyn LlmClient>` it hands to `LlmExtractor` has moved away.
#[derive(Clone)]
struct SharedClient(Arc<FakeClient>);

impl LlmClient for SharedClient {
    fn complete<'a>(
        &'a self,
        req: &'a LlmRequest,
    ) -> hauz_core::BoxFuture<'a, core::result::Result<String, Error>> {
        Box::pin(async move {
            self.0
                .seen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(req.clone());
            self.0.reply.clone()
        })
    }
}

/// Always returns a fixed rasterization result, ignoring its input.
struct FakeRasterizer(core::result::Result<Vec<Vec<u8>>, Error>);

impl Rasterizer for FakeRasterizer {
    fn rasterize(&self, _pdf: &[u8], _max_pages: u8) -> core::result::Result<Vec<Vec<u8>>, Error> {
        self.0.clone()
    }
}

const PDF_MIME: &str = "application/pdf";
const PNG_MIME: &str = "image/png";

fn document(mime: &str, bytes: Vec<u8>) -> Result<Document> {
    Ok(Document {
        mime: MimeType::new(mime)?,
        filename: None,
        bytes,
    })
}

/// An otherwise-empty envelope with a sender, optional text/html, and a list of documents.
fn envelope(text: Option<&str>, html: Option<&str>, documents: Vec<Document>) -> Envelope {
    Envelope {
        subject: None,
        sender: "billing@example.com".to_owned(),
        sender_name: None,
        date: None,
        text: text.map(str::to_owned),
        html: html.map(str::to_owned),
        documents,
    }
}

/// A full, schema-conformant reply (every field present) wrapped in a ```` ```json ```` fence,
/// at confidence 100 — the spec's "full reply" fixture.
const FULL_REPLY: &str = "```json\n{\"vendor\":\"Acme Power\",\"amount_minor_units\":999,\
\"currency\":\"USD\",\"period_start\":\"2026-09-01\",\"period_end\":\"2026-09-30\",\
\"issued\":\"2026-10-01\",\"due\":\"2026-10-15\",\"confidence\":100}\n```";

#[tokio::test]
async fn ac1_sends_text_then_pdf_pages_then_text_layer_with_full_schema() -> Result<()> {
    let pdf_bytes = common::minimal_pdf(&["Acme Power Invoice"]);
    let docs = vec![
        document(PDF_MIME, pdf_bytes)?,
        document(PNG_MIME, vec![0, 1, 2, 3])?,
    ];
    let env = envelope(Some("Body text"), Some("<p>ignored</p>"), docs);

    let client = Arc::new(FakeClient::new(Ok(FULL_REPLY.to_owned())));
    let rasterizer = FakeRasterizer(Ok(vec![vec![1, 1], vec![2, 2]]));
    let extractor = LlmExtractor::new(
        Box::new(SharedClient(client.clone())),
        Box::new(rasterizer),
        LlmOptions::default(),
    );

    let _ = extractor.extract(&env).await?;

    let seen = client.seen.lock().unwrap_or_else(PoisonError::into_inner);
    let request = seen.first().ok_or("expected one request")?;
    assert_eq!(request.parts.len(), 4);
    assert_eq!(request.parts[0], Part::Text("Body text".to_owned()));
    assert_eq!(request.parts[1], Part::Png(vec![1, 1]));
    assert_eq!(request.parts[2], Part::Png(vec![2, 2]));
    let Part::Text(layer) = &request.parts[3] else {
        return Err("expected a trailing Text part".into());
    };
    assert!(layer.contains("Acme Power Invoice"));
    assert!(!request.instructions.is_empty());

    let schema_json = serde_json::to_value(&request.schema)?;
    let required: BTreeSet<&str> = schema_json
        .get("required")
        .and_then(serde_json::Value::as_array)
        .ok_or("expected a required array")?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    let expected: BTreeSet<&str> = [
        "vendor",
        "amount_minor_units",
        "currency",
        "period_start",
        "period_end",
        "issued",
        "due",
        "confidence",
    ]
    .into_iter()
    .collect();
    assert_eq!(required, expected);
    Ok(())
}

#[tokio::test]
async fn ac2_short_body_truncated_and_html_only_when_text_absent() -> Result<()> {
    let env = envelope(None, Some("<p>Total</p><p>99</p>"), Vec::new());
    let client = Arc::new(FakeClient::new(Ok(FULL_REPLY.to_owned())));
    let options = LlmOptions {
        max_body_chars: 5,
        ..LlmOptions::default()
    };
    let extractor = LlmExtractor::new(
        Box::new(SharedClient(client.clone())),
        Box::new(FakeRasterizer(Ok(Vec::new()))),
        options,
    );

    let _ = extractor.extract(&env).await?;

    let seen = client.seen.lock().unwrap_or_else(PoisonError::into_inner);
    let request = seen.first().ok_or("expected one request")?;
    assert_eq!(request.parts.first(), Some(&Part::Text("Total".to_owned())));
    Ok(())
}

#[tokio::test]
async fn ac2_native_delivery_sends_pdf_part_and_skips_rasterizer() -> Result<()> {
    let pdf_bytes = common::minimal_pdf(&["Acme"]);
    let env = envelope(
        Some("body"),
        None,
        vec![document(PDF_MIME, pdf_bytes.clone())?],
    );
    let client = Arc::new(FakeClient::new(Ok(FULL_REPLY.to_owned())));
    // A rasterizer that errors if called at all: `Native` delivery must never call it.
    let rasterizer = FakeRasterizer(Err(Error::Rasterizer {
        reason: "must not be called under Native delivery".to_owned(),
    }));
    let options = LlmOptions {
        delivery: PdfDelivery::Native,
        ..LlmOptions::default()
    };
    let extractor = LlmExtractor::new(
        Box::new(SharedClient(client.clone())),
        Box::new(rasterizer),
        options,
    );

    let _ = extractor.extract(&env).await?;

    let seen = client.seen.lock().unwrap_or_else(PoisonError::into_inner);
    let request = seen.first().ok_or("expected one request")?;
    assert!(request.parts.contains(&Part::Pdf(pdf_bytes)));
    assert!(!request.parts.iter().any(|p| matches!(p, Part::Png(_))));
    Ok(())
}

#[tokio::test]
async fn ac3_full_reply_maps_every_field_at_capped_confidence_no_notes() -> Result<()> {
    let env = envelope(Some("body"), None, Vec::new());
    let client = Arc::new(FakeClient::new(Ok(FULL_REPLY.to_owned())));
    let extractor = LlmExtractor::new(
        Box::new(SharedClient(client)),
        Box::new(FakeRasterizer(Ok(Vec::new()))),
        LlmOptions::default(),
    );

    let extraction = extractor.extract(&env).await?;

    let span = Span {
        source: Source::Model,
        start: 0,
        end: 0,
    };
    let confidence = Confidence::new(70)?;
    assert_eq!(
        extraction.amount,
        Some(Field {
            value: Money::new(999, Currency::new("USD")?),
            confidence,
            span,
        })
    );
    assert_eq!(
        extraction.vendor,
        Some(Field {
            value: Vendor::new("Acme Power")?,
            confidence,
            span,
        })
    );
    assert_eq!(
        extraction.period,
        Some(Field {
            value: BillingPeriod::new(date!(2026 - 09 - 01), date!(2026 - 09 - 30))?,
            confidence,
            span,
        })
    );
    assert_eq!(
        extraction.issued,
        Some(Field {
            value: date!(2026 - 10 - 01),
            confidence,
            span,
        })
    );
    assert_eq!(
        extraction.due,
        Some(Field {
            value: date!(2026 - 10 - 15),
            confidence,
            span,
        })
    );
    assert!(extraction.notes.is_empty());
    Ok(())
}

#[tokio::test]
async fn ac4_invalid_currency_blank_vendor_and_null_fields_become_none() -> Result<()> {
    let reply = "{\"vendor\":\"  \",\"amount_minor_units\":999,\"currency\":\"$\",\
\"period_start\":\"2026-09-01\",\"period_end\":null,\"issued\":\"2026-10-01\",\"due\":null,\
\"confidence\":40}";
    let env = envelope(Some("body"), None, Vec::new());
    let client = Arc::new(FakeClient::new(Ok(reply.to_owned())));
    let extractor = LlmExtractor::new(
        Box::new(SharedClient(client)),
        Box::new(FakeRasterizer(Ok(Vec::new()))),
        LlmOptions::default(),
    );

    let extraction = extractor.extract(&env).await?;

    assert_eq!(extraction.amount, None);
    assert_eq!(extraction.vendor, None);
    assert_eq!(extraction.period, None);
    assert_eq!(extraction.due, None);
    let issued = extraction.issued.ok_or("expected issued")?;
    assert_eq!(issued.value, date!(2026 - 10 - 01));
    assert_eq!(issued.confidence, Confidence::new(40)?);
    Ok(())
}

#[tokio::test]
async fn ac5_client_error_degrades_to_default_with_llm_unavailable_note() -> Result<()> {
    let env = envelope(Some("body"), None, Vec::new());
    let client = Arc::new(FakeClient::new(Err(Error::Client {
        message: "connection refused".to_owned(),
    })));
    let extractor = LlmExtractor::new(
        Box::new(SharedClient(client)),
        Box::new(FakeRasterizer(Ok(Vec::new()))),
        LlmOptions::default(),
    );

    let extraction = extractor.extract(&env).await?;
    assert_eq!(extraction, {
        let mut expected = Extraction::default();
        expected.notes.insert(Note::LlmUnavailable);
        expected
    });
    Ok(())
}

#[tokio::test]
async fn ac5_unparseable_reply_degrades_to_default_with_llm_malformed_note() -> Result<()> {
    let env = envelope(Some("body"), None, Vec::new());
    let client = Arc::new(FakeClient::new(Ok("not json".to_owned())));
    let extractor = LlmExtractor::new(
        Box::new(SharedClient(client)),
        Box::new(FakeRasterizer(Ok(Vec::new()))),
        LlmOptions::default(),
    );

    let extraction = extractor.extract(&env).await?;
    assert_eq!(extraction, {
        let mut expected = Extraction::default();
        expected.notes.insert(Note::LlmMalformed);
        expected
    });
    Ok(())
}

#[tokio::test]
async fn ac5_unsupported_client_error_is_extract_error_llm() -> Result<()> {
    let env = envelope(Some("body"), None, Vec::new());
    let client = Arc::new(FakeClient::new(Err(Error::Unsupported {
        part: "pdf".to_owned(),
        provider: "ollama".to_owned(),
    })));
    let extractor = LlmExtractor::new(
        Box::new(SharedClient(client)),
        Box::new(FakeRasterizer(Ok(Vec::new()))),
        LlmOptions::default(),
    );

    let result = extractor.extract(&env).await;
    assert!(matches!(
        result,
        Err(ExtractError::Llm(Error::Unsupported { .. }))
    ));
    Ok(())
}

#[tokio::test]
async fn ac5_rasterizer_failure_is_extract_error_llm() -> Result<()> {
    let pdf_bytes = common::minimal_pdf(&["Acme"]);
    let env = envelope(Some("body"), None, vec![document(PDF_MIME, pdf_bytes)?]);
    let client = Arc::new(FakeClient::new(Ok(FULL_REPLY.to_owned())));
    let rasterizer = FakeRasterizer(Err(Error::Rasterizer {
        reason: "pdftoppm failed".to_owned(),
    }));
    let extractor = LlmExtractor::new(
        Box::new(SharedClient(client)),
        Box::new(rasterizer),
        LlmOptions::default(),
    );

    let result = extractor.extract(&env).await;
    assert!(matches!(
        result,
        Err(ExtractError::Llm(Error::Rasterizer { .. }))
    ));
    Ok(())
}

#[tokio::test]
async fn ac5_empty_envelope_skips_client_call() -> Result<()> {
    let env = envelope(None, None, Vec::new());
    let client = Arc::new(FakeClient::new(Ok(FULL_REPLY.to_owned())));
    let extractor = LlmExtractor::new(
        Box::new(SharedClient(client.clone())),
        Box::new(FakeRasterizer(Ok(Vec::new()))),
        LlmOptions::default(),
    );

    let extraction = extractor.extract(&env).await?;
    assert_eq!(extraction, Extraction::default());
    assert!(
        client
            .seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn ac5_non_iso_currency_omits_amount() -> Result<()> {
    let reply = "{\"vendor\":\"Acme Power\",\"amount_minor_units\":999,\"currency\":\"NIT\",\
\"period_start\":\"2026-09-01\",\"period_end\":null,\"issued\":\"2026-10-01\",\"due\":null,\
\"confidence\":40}";
    let env = envelope(Some("body"), None, Vec::new());
    let client = Arc::new(FakeClient::new(Ok(reply.to_owned())));
    let extractor = LlmExtractor::new(
        Box::new(SharedClient(client)),
        Box::new(FakeRasterizer(Ok(Vec::new()))),
        LlmOptions::default(),
    );

    let extraction = extractor.extract(&env).await?;

    assert_eq!(extraction.amount, None);
    let vendor = extraction.vendor.ok_or("expected vendor")?;
    assert_eq!(vendor.value, Vendor::new("Acme Power")?);
    assert!(extraction.issued.is_some());
    Ok(())
}
