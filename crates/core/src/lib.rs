//! Core library: all domain logic lives here and is tested through this crate's public API.
//! Binaries (`server`, `app`) only map I/O to calls into this crate.

pub mod bill;
pub mod email;
pub mod extract;
pub mod store;
