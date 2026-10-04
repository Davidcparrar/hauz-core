//! Core library: all domain logic lives here and is tested through this crate's public API.
//! Binaries (`server`, `app`) only map I/O to calls into this crate.

use std::future::Future;
use std::pin::Pin;

pub mod bill;
pub mod email;
pub mod extract;
pub mod ingest;
pub mod llm;
pub mod store;
pub mod zip;

/// A future boxed for a dyn-compatible async trait: no `async-trait`, no `unsafe`. Used by
/// [`store::BillStore`] and [`extract::Extractor`], whose implementations box an `async move`
/// block (`store` re-exports this alias so existing paths keep compiling).
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
