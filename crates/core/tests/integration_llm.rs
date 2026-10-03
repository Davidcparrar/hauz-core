//! [integration] tests for the `llm` module crossing real system edges: a real `pdftoppm`
//! binary (AC5) and, opt-in, a real local Ollama daemon (AC6). Both skip (`Ok(())`) when
//! their precondition is absent, per the spec's test plan.

mod common;

use common::Result;
use hauz_core::llm::{LlmClient, LlmRequest, Part, Pdftoppm, Provider, Rasterizer, RigClient};

const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

fn fixture_bytes() -> Result<Vec<u8>> {
    Ok(std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/synthetic_bill.pdf"
    ))?)
}

/// Strips an optional leading ```` ```json ```` or ```` ``` ```` line and a trailing ```` ``` ````
/// line, as some models wrap an otherwise schema-conformant reply in a markdown code fence.
/// Test-local only: [`hauz_core::llm::RigClient::complete`] keeps returning the raw reply.
fn strip_markdown_fence(reply: &str) -> &str {
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

/// `true` when a `pdftoppm` binary resolves on `PATH` (checked the same way the OS would:
/// trying to run it).
fn pdftoppm_on_path() -> bool {
    std::process::Command::new("pdftoppm")
        .arg("-v")
        .output()
        .is_ok()
}

#[test]
fn ac5_rasterizes_fixture_at_150_dpi_into_one_png_page() -> Result<()> {
    if !pdftoppm_on_path() {
        return Ok(());
    }
    let fixture = fixture_bytes()?;
    let rasterizer = Pdftoppm::new(150);
    let pages = rasterizer.rasterize(&fixture, 4)?;
    assert_eq!(pages.len(), 1);
    assert!(pages[0].starts_with(&PNG_SIGNATURE));
    Ok(())
}

#[tokio::test]
async fn ac6_live_ollama_answers_schema_conformant_json() -> Result<()> {
    if std::env::var("HAUZ_LIVE_OLLAMA").as_deref() != Ok("1") {
        return Ok(());
    }
    if !pdftoppm_on_path() {
        return Ok(());
    }

    let fixture = fixture_bytes()?;
    let page = Pdftoppm::new(150)
        .rasterize(&fixture, 1)?
        .into_iter()
        .next()
        .ok_or("expected at least one rasterized page")?;

    #[derive(schemars::JsonSchema)]
    struct VendorOnly {
        // Never read directly: only `schemars::JsonSchema`'s derive reflects over it.
        #[allow(dead_code)]
        vendor: Option<String>,
    }

    let client = RigClient::new(
        Provider::Ollama {
            base_url: "http://localhost:11434".to_owned(),
        },
        "gemma4:latest",
    );
    let request = LlmRequest {
        instructions: "This image is a one-page bill. Extract the vendor name as JSON.".to_owned(),
        parts: vec![
            Part::Text("Extract the vendor.".to_owned()),
            Part::Png(page),
        ],
        schema: schemars::schema_for!(VendorOnly),
    };

    let reply = client.complete(&request).await?;
    let parsed: serde_json::Value = serde_json::from_str(strip_markdown_fence(&reply))?;
    assert!(parsed.is_object());
    Ok(())
}
