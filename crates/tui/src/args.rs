//! Hand-parsed grammar for `hauz-tui`: `[--db <path>]` or `-h|--help`.

use std::ffi::OsString;
use std::path::PathBuf;

/// Printed on usage errors (stderr, exit 2) and on `-h`/`--help` (stdout, exit 0).
pub(crate) const USAGE: &str = "usage: hauz-tui [--db <sqlite path>]";

const DEFAULT_DB: &str = "hauz.db";

/// A parsed invocation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Browse { db: PathBuf },
    Help,
}

/// The invocation did not match the grammar.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UsageError;

/// Parses argv (already past `argv[0]`).
pub(crate) fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Command, UsageError> {
    let mut db = PathBuf::from(DEFAULT_DB);
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => return Ok(Command::Help),
            "--db" => db = PathBuf::from(args.next().ok_or(UsageError)?),
            _ => return Err(UsageError),
        }
    }
    Ok(Command::Browse { db })
}
