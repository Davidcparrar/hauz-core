//! `hauz ingest [--reextract] <file.eml> [--db <sqlite path>]`: local dev/replay entry point. Reads one raw
//! message from disk and runs the same pipeline as `crates/server`
//! (`Chain([XmlInvoiceExtractor, TextExtractor, PdfTextExtractor])`, escalating to an
//! `LlmExtractor` when `HAUZ_LLM_PROVIDER` is set, over a `SqliteStore`), printing the
//! outcome and bill id on one line (`--reextract` overwrites an already-stored row instead of
//! reporting `Duplicate`). Untested in isolation by design; behavior is covered
//! end-to-end via `tests/e2e_cli.rs` (`assert_cmd`).

mod args;

use std::env;
use std::fs;
use std::path::Path;

use anyhow::Context;
use hauz_core::extract::{
    Chain, Escalate, Extractor, PdfTextExtractor, TextExtractor, XmlInvoiceExtractor,
};
use hauz_core::ingest::{EXTRACTED_MIN_CONFIDENCE, Outcome, Reextracted, ingest, reextract};
use hauz_core::llm::{Config, LlmExtractor, LlmOptions, Pdftoppm, RigClient};
use hauz_core::mail;
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
        Command::Ingest {
            path,
            db,
            reextract,
        } => run_ingest(&path, &db, reextract).await,
        Command::Fetch { db, after, before } => {
            let range = match mail::DateRange::parse(after.as_deref(), before.as_deref()) {
                Ok(range) => range,
                Err(e) => {
                    eprintln!("{e}\n{USAGE}");
                    std::process::exit(2);
                }
            };
            run_fetch(&db, &range).await
        }
    }
}

/// Reads `path`, then resolves the LLM config before opening `db`, so neither a bad path nor
/// a bad LLM config ever creates an empty DB file.
async fn run_ingest(path: &Path, db: &Path, reextract_row: bool) -> anyhow::Result<()> {
    let raw = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let config = Config::from_env(|key| env::var(key).ok())?;
    let extractor = build_extractor(config);
    let store = SqliteStore::open(db).await?;

    if reextract_row {
        match reextract(&raw, &*extractor, &store).await? {
            Reextracted::Created(id) => println!("Created {}", id.as_str()),
            Reextracted::Updated(id) => println!("Updated {}", id.as_str()),
            Reextracted::Unchanged(id) => println!("Unchanged {}", id.as_str()),
        }
        return Ok(());
    }

    let outcome = ingest(&raw, &*extractor, &store).await?;
    match outcome {
        Outcome::Created(id) => println!("Created {}", id.as_str()),
        Outcome::Duplicate(id) => println!("Duplicate {}", id.as_str()),
    }
    Ok(())
}

/// Resolves the LLM and Gmail config before opening `db`, downloads every message under the
/// configured label (within `range`) and ingests it in-process. Prints one line per message; exits 1 when any
/// message failed, and (through `?`) when the run aborted.
async fn run_fetch(db: &Path, range: &mail::DateRange) -> anyhow::Result<()> {
    let llm = Config::from_env(|key| env::var(key).ok())?;
    let gmail = mail::Config::from_env(|key| env::var(key).ok())?
        .context("HAUZ_GMAIL_CLIENT_ID is not set: Gmail is not configured")?;
    let extractor = build_extractor(llm);
    let store = SqliteStore::open(db).await?;

    let source = gmail.source();
    let fetched = mail::fetch(&source, &gmail.query(range), &*extractor, &store).await?;
    let mut failed = false;
    for item in &fetched {
        match &item.outcome {
            Ok(Outcome::Created(id)) => println!("Created {}", id.as_str()),
            Ok(Outcome::Duplicate(id)) => println!("Duplicate {}", id.as_str()),
            Err(e) => {
                failed = true;
                println!("Failed {}: {e}", item.id.as_str());
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

/// `None` (no `HAUZ_LLM_PROVIDER`) is today's `Chain([XmlInvoiceExtractor, TextExtractor,
/// PdfTextExtractor])`; `Some(config)` wraps it in `Escalate` with an `LlmExtractor` as the
/// secondary.
fn build_extractor(config: Option<Config>) -> Box<dyn Extractor> {
    let chain = Chain::new(vec![
        Box::new(XmlInvoiceExtractor),
        Box::new(TextExtractor),
        Box::new(PdfTextExtractor),
    ]);
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
