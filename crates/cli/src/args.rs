//! Hand-parsed grammar for `hauz`: `hauz ingest [--reextract] <path> [--db <path>]` (any order after
//! `ingest`), `hauz fetch [--db <path>] [--after <day>] [--before <day>]`, or `hauz -h|--help`. No `clap`: the grammar is three tokens.

use std::ffi::OsString;
use std::path::PathBuf;

/// Printed on usage errors (stderr, exit 2) and on `-h`/`--help` (stdout, exit 0).
pub(crate) const USAGE: &str = "usage: hauz ingest [--reextract] <file.eml> [--db <sqlite path>]\n       hauz fetch [--db <sqlite path>] [--after <YYYY-MM-DD>] [--before <YYYY-MM-DD>]";

/// `--db`'s default when the flag is absent.
const DEFAULT_DB: &str = "hauz.db";

/// A fully parsed invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    /// `hauz ingest [--reextract] <path> [--db <path>]`.
    Ingest {
        path: PathBuf,
        db: PathBuf,
        reextract: bool,
    },
    /// `hauz fetch [--db <path>] [--after <day>] [--before <day>]`; days are validated later.
    Fetch {
        db: PathBuf,
        after: Option<String>,
        before: Option<String>,
    },
    /// `hauz -h` or `hauz --help`.
    Help,
}

/// The invocation did not match the grammar; carries the offending token (empty when a
/// required token was simply missing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsageError {
    pub(crate) token: String,
}

impl UsageError {
    fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

/// Parses `args` (the process's argv, `argv[0]` already skipped) against the grammar.
pub(crate) fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Command, UsageError> {
    let Some(first) = args.next() else {
        return Err(UsageError::new(String::new()));
    };
    let first = first.to_string_lossy().into_owned();
    match first.as_str() {
        "-h" | "--help" => Ok(Command::Help),
        "ingest" => parse_ingest(args),
        "fetch" => parse_fetch(args),
        other => Err(UsageError::new(other.to_owned())),
    }
}

/// Parses the tokens after `ingest`: exactly one positional path, an optional `--db <path>`
/// and an optional `--reextract`, in any order.
fn parse_ingest(args: impl Iterator<Item = OsString>) -> Result<Command, UsageError> {
    let mut path: Option<PathBuf> = None;
    let mut db: Option<PathBuf> = None;
    let mut reextract = false;

    let mut args = args;
    while let Some(arg) = args.next() {
        let arg_str = arg.to_string_lossy().into_owned();
        if arg_str == "--db" {
            let value = args.next().ok_or_else(|| UsageError::new("--db"))?;
            db = Some(PathBuf::from(value));
        } else if arg_str == "--reextract" {
            reextract = true;
        } else if arg_str.starts_with('-') {
            return Err(UsageError::new(arg_str));
        } else if path.is_none() {
            path = Some(PathBuf::from(arg));
        } else {
            return Err(UsageError::new(arg_str));
        }
    }

    let path = path.ok_or_else(|| UsageError::new(String::new()))?;
    let db = db.unwrap_or_else(|| PathBuf::from(DEFAULT_DB));
    Ok(Command::Ingest {
        path,
        db,
        reextract,
    })
}

/// Parses the tokens after `fetch`: optional `--db <path>`, `--after <day>`, `--before
/// <day>`, in any order.
fn parse_fetch(mut args: impl Iterator<Item = OsString>) -> Result<Command, UsageError> {
    let mut db: Option<PathBuf> = None;
    let mut after: Option<String> = None;
    let mut before: Option<String> = None;
    while let Some(arg) = args.next() {
        let arg_str = arg.to_string_lossy().into_owned();
        let mut value = || args.next().ok_or_else(|| UsageError::new(arg_str.clone()));
        match arg_str.as_str() {
            "--db" => db = Some(PathBuf::from(value()?)),
            "--after" => after = Some(value()?.to_string_lossy().into_owned()),
            "--before" => before = Some(value()?.to_string_lossy().into_owned()),
            _ => return Err(UsageError::new(arg_str.clone())),
        }
    }
    let db = db.unwrap_or_else(|| PathBuf::from(DEFAULT_DB));
    Ok(Command::Fetch { db, after, before })
}
