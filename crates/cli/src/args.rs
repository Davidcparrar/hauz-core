//! Hand-parsed grammar for `hauz`: `hauz ingest <path> [--db <path>]` (any order after
//! `ingest`) or `hauz -h|--help`. No `clap`: the grammar is three tokens.

use std::ffi::OsString;
use std::path::PathBuf;

/// Printed on usage errors (stderr, exit 2) and on `-h`/`--help` (stdout, exit 0).
pub(crate) const USAGE: &str = "usage: hauz ingest <file.eml> [--db <sqlite path>]";

/// `--db`'s default when the flag is absent.
const DEFAULT_DB: &str = "hauz.db";

/// A fully parsed invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    /// `hauz ingest <path> [--db <path>]`.
    Ingest { path: PathBuf, db: PathBuf },
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
        other => Err(UsageError::new(other.to_owned())),
    }
}

/// Parses the tokens after `ingest`: exactly one positional path and an optional `--db
/// <path>`, in any order.
fn parse_ingest(args: impl Iterator<Item = OsString>) -> Result<Command, UsageError> {
    let mut path: Option<PathBuf> = None;
    let mut db: Option<PathBuf> = None;

    let mut args = args;
    while let Some(arg) = args.next() {
        let arg_str = arg.to_string_lossy().into_owned();
        if arg_str == "--db" {
            let value = args.next().ok_or_else(|| UsageError::new("--db"))?;
            db = Some(PathBuf::from(value));
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
    Ok(Command::Ingest { path, db })
}
