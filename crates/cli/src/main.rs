//! `hauz ingest <file.eml> [--db <sqlite path>]`: local dev/replay entry point. Reads one raw
//! message from disk and runs the same pipeline as `crates/server`
//! (`Chain([TextExtractor, PdfTextExtractor])` over a `SqliteStore`), printing the outcome and
//! bill id on one line. Untested in isolation by design; behavior is covered end-to-end via
//! `tests/e2e_cli.rs` (`assert_cmd`).

mod args;

use std::env;
use std::fs;
use std::path::Path;

use anyhow::Context;
use hauz_core::extract::{Chain, PdfTextExtractor, TextExtractor};
use hauz_core::ingest::{Outcome, ingest};
use hauz_core::store::SqliteStore;

use args::{Command, USAGE, parse};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Ok(command) = parse(env::args_os().skip(1)) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };

    match command {
        Command::Help => {
            println!("{USAGE}");
            Ok(())
        }
        Command::Ingest { path, db } => run_ingest(&path, &db).await,
    }
}

/// Reads `path` before opening `db`, so a bad path never creates an empty DB file.
async fn run_ingest(path: &Path, db: &Path) -> anyhow::Result<()> {
    let raw = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let store = SqliteStore::open(db).await?;
    let extractor = Chain::new(vec![Box::new(TextExtractor), Box::new(PdfTextExtractor)]);

    let outcome = ingest(&raw, &extractor, &store).await?;
    match outcome {
        Outcome::Created(id) => println!("Created {}", id.as_str()),
        Outcome::Duplicate(id) => println!("Duplicate {}", id.as_str()),
    }
    Ok(())
}
