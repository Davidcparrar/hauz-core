//! `hauz ingest <file.eml> [--db <sqlite path>]`: local dev/replay entry point. Reads one raw
//! message from disk and runs the same pipeline as `crates/server`
//! (`Chain([TextExtractor, PdfTextExtractor])`, escalating to an `LlmExtractor` when
//! `HAUZ_LLM_PROVIDER` is set, over a `SqliteStore`), printing the outcome and bill id on one
//! line. Untested in isolation by design; behavior is covered end-to-end via `tests/e2e_cli.rs`
//! (`assert_cmd`).

mod args;

use std::env;
use std::fs;
use std::path::Path;

use anyhow::Context;
use hauz_core::extract::{Chain, Escalate, Extractor, PdfTextExtractor, TextExtractor};
use hauz_core::ingest::{EXTRACTED_MIN_CONFIDENCE, Outcome, ingest};
use hauz_core::llm::{Config, LlmExtractor, LlmOptions, Pdftoppm, RigClient};
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

/// Reads `path`, then resolves the LLM config before opening `db`, so neither a bad path nor
/// a bad LLM config ever creates an empty DB file.
async fn run_ingest(path: &Path, db: &Path) -> anyhow::Result<()> {
    let raw = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let config = Config::from_env(|key| env::var(key).ok())?;
    let extractor = build_extractor(config);
    let store = SqliteStore::open(db).await?;

    let outcome = ingest(&raw, &*extractor, &store).await?;
    match outcome {
        Outcome::Created(id) => println!("Created {}", id.as_str()),
        Outcome::Duplicate(id) => println!("Duplicate {}", id.as_str()),
    }
    Ok(())
}

/// `None` (no `HAUZ_LLM_PROVIDER`) is today's `Chain([TextExtractor, PdfTextExtractor])`;
/// `Some(config)` wraps it in `Escalate` with an `LlmExtractor` as the secondary.
fn build_extractor(config: Option<Config>) -> Box<dyn Extractor> {
    let chain = Chain::new(vec![Box::new(TextExtractor), Box::new(PdfTextExtractor)]);
    let Some(config) = config else {
        return Box::new(chain);
    };
    let llm_extractor = LlmExtractor::new(
        Box::new(RigClient::new(config.provider, &config.model)),
        Box::new(Pdftoppm::new(150)),
        LlmOptions::default(),
    );
    Box::new(Escalate::new(
        Box::new(chain),
        Box::new(llm_extractor),
        EXTRACTED_MIN_CONFIDENCE,
    ))
}
