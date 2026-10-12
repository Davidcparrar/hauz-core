//! Hand-parsed grammar for `hauz-web`: `[--db <path>] [--port <n>]`.

use std::ffi::OsString;
use std::path::PathBuf;

/// Printed on usage errors (stderr, exit 2).
pub(crate) const USAGE: &str = "usage: hauz-web [--db <sqlite path>] [--port <n>]";

const DEFAULT_DB: &str = "hauz.db";
const DEFAULT_PORT: u16 = 3000;

/// A parsed invocation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Args {
    pub(crate) db: PathBuf,
    pub(crate) port: u16,
}

/// The invocation did not match the grammar.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UsageError;

/// Parses argv (already past `argv[0]`).
pub(crate) fn parse(mut args: impl Iterator<Item = OsString>) -> Result<Args, UsageError> {
    let mut db = PathBuf::from(DEFAULT_DB);
    let mut port = DEFAULT_PORT;
    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "--db" => db = PathBuf::from(args.next().ok_or(UsageError)?),
            "--port" => {
                port = args
                    .next()
                    .and_then(|p| p.to_string_lossy().parse().ok())
                    .ok_or(UsageError)?;
            }
            _ => return Err(UsageError),
        }
    }
    Ok(Args { db, port })
}

#[cfg(test)]
mod tests {
    use super::{Args, UsageError, parse};
    use std::ffi::OsString;

    fn run(args: &[&str]) -> Result<Args, UsageError> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn defaults_and_overrides() {
        assert_eq!(run(&[]).map(|a| a.port), Ok(3000));
        assert_eq!(run(&["--port", "8080"]).map(|a| a.port), Ok(8080));
        assert!(run(&["--port", "x"]).is_err());
        assert!(run(&["--db"]).is_err());
    }
}
