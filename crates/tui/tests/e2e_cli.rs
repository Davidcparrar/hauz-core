//! [e2e] acceptance tests for the `hauz-tui` binary's non-terminal paths (feature #41).

mod common;

use assert_cmd::Command;
use common::Result;

#[test]
fn ac10_missing_db_exits_1_naming_path_creating_nothing() -> Result<()> {
    let dir = common::tmp_dir();
    let db = dir.join("missing.db");

    let output = Command::cargo_bin("hauz-tui")?
        .current_dir(&dir)
        .arg("--db")
        .arg(&db)
        .output()?;

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stderr)?.contains(&db.display().to_string()));
    assert!(!db.exists());
    Ok(())
}

#[test]
fn ac11_usage_errors_exit_2_and_help_exits_0() -> Result<()> {
    for args in [vec!["--bogus"], vec!["--db"], vec!["extra"]] {
        let output = Command::cargo_bin("hauz-tui")?.args(&args).output()?;
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8(output.stderr)?.contains("usage: hauz-tui"));
    }
    for flag in ["-h", "--help"] {
        let output = Command::cargo_bin("hauz-tui")?.arg(flag).output()?;
        assert_eq!(output.status.code(), Some(0));
        assert!(String::from_utf8(output.stdout)?.contains("usage: hauz-tui"));
    }
    Ok(())
}
