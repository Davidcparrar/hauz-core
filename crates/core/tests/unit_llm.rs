//! [unit] tests for the `llm` module's public API: `Config::from_env`, `RigClient`'s
//! PDF-on-Ollama rejection, and `Provider`'s redacted `Debug`. One file per level per module.
//! Test fn names carry the spec criterion they satisfy: `acN_<behavior>`.

mod common;

use common::Result;
use hauz_core::llm::{
    Error, LlmClient, LlmRequest, Part, Pdftoppm, Provider, Rasterizer, RigClient,
};
use std::collections::HashMap;
use std::path::PathBuf;

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
