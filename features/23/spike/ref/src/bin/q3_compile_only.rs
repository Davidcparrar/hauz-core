//! Spike for #23, Q3: compile-only, no network. Anthropic and OpenAI clients
//! built with an explicit key and base URL; a Document(PDF) part and an
//! Image part both type-check in a completion request to each. Never run
//! (no `main` network call); `cargo build` is the check.

use rig_core::completion::message::{
    Document, DocumentMediaType, DocumentSourceKind, Image, ImageMediaType, Message, UserContent,
};
use rig_core::completion::CompletionRequest;
use rig_core::providers::anthropic::wire::AnthropicConfig;
use rig_core::providers::openai::wire::OpenAIConfig;

fn pdf_part() -> UserContent {
    UserContent::Document(Document {
        data: DocumentSourceKind::base64("<redacted>"),
        media_type: Some(DocumentMediaType::PDF),
        additional_params: None,
    })
}

fn image_part() -> UserContent {
    UserContent::Image(Image {
        data: DocumentSourceKind::base64("<redacted>"),
        media_type: Some(ImageMediaType::PNG),
        detail: None,
        additional_params: None,
    })
}

fn request_with_both_parts() -> CompletionRequest {
    let message = Message::User {
        content: vec![pdf_part(), image_part()],
    };
    CompletionRequest::new(message)
}

fn main() {
    // Explicit key string + explicit base URL, both providers, no network call.
    let _anthropic = AnthropicConfig::new("sk-ant-fake-key").with_base_url("https://example.invalid/anthropic");
    let _openai = OpenAIConfig::new("sk-fake-key").with_base_url("https://example.invalid/openai");
    let _request = request_with_both_parts();
    println!("compiled: Document(PDF) and Image parts type-check in one CompletionRequest");
}
