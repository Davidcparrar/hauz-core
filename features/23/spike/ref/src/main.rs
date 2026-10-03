//! Spike for #23, Q1+Q2: rig-core 0.43 against a local Ollama daemon with a
//! rasterized synthetic bill PNG, asking for schema-conformant structured
//! output. Throwaway code: see features/23/spike/findings.md.

use std::time::Instant;

use rig_core::completion::message::{
    DocumentSourceKind, Image, ImageMediaType, Message, Text, UserContent,
};
use rig_core::completion::CompletionRequest;
use rig_core::driver::Model;
use rig_core::providers::ollama::wire::OllamaConfig;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct BillFields {
    vendor: Option<String>,
    amount_minor_units: Option<i64>,
    currency: Option<String>,
    period_start: Option<String>,
    period_end: Option<String>,
    due: Option<String>,
    confidence: u8,
}

const BASE_URL: &str = "http://localhost:11434";
const MODEL: &str = "gemma4:latest";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let png_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "bill_150dpi.png".to_string());
    let png_bytes = std::fs::read(&png_path)?;
    let b64 = base64_encode(&png_bytes);

    // Explicit base URL, not env magic.
    let ollama = OllamaConfig::new().with_base_url(BASE_URL).client();
    let model: Model<_> = ollama.completion(MODEL);

    let schema = schemars::schema_for!(BillFields);

    let instruction = Text::new(
        "This image is a one-page bill. Extract vendor, amount in minor units, \
         currency (ISO 4217), billing period start/end, and due date (ISO 8601 \
         where possible). Return only the requested JSON fields. Rate your own \
         confidence 0-100.",
    );
    let image = Image {
        data: DocumentSourceKind::base64(&b64),
        media_type: Some(ImageMediaType::PNG),
        detail: None,
        additional_params: None,
    };
    let message = Message::User {
        content: vec![UserContent::Text(instruction), UserContent::Image(image)],
    };

    let request = CompletionRequest::new(message).output_schema(Some(schema));

    for attempt in 1..=3 {
        let start = Instant::now();
        let result = model.call(request.clone()).await;
        let elapsed = start.elapsed();
        match result {
            Ok(response) => {
                let text = response.text();
                println!("attempt {attempt}: {elapsed:?} wall-clock");
                println!("raw text: {text}");
                match serde_json::from_str::<BillFields>(&text) {
                    Ok(fields) => {
                        println!("parsed OK: {fields:?}");
                        return Ok(());
                    }
                    Err(e) => println!("parse FAILED: {e}"),
                }
            }
            Err(e) => println!("attempt {attempt}: call FAILED after {elapsed:?}: {e}"),
        }
    }
    Ok(())
}

/// Minimal base64 encoder (no external dependency beyond the spike's
/// declared set): RFC 4648 standard alphabet, with padding.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 { ALPHABET[((n >> 6) & 0x3f) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[(n & 0x3f) as usize] as char } else { '=' });
    }
    out
}
