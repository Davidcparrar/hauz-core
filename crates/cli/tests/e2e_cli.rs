//! [e2e] acceptance tests for the `hauz` binary (feature #8), plus its `HAUZ_LLM_PROVIDER`
//! env wiring (#26). Every test shells out via `assert_cmd::Command::cargo_bin("hauz")`,
//! inspecting the process's exit code, stdout, stderr and the `SqliteStore` it leaves behind.

mod common;

use std::ffi::OsString;

use assert_cmd::Command;
use hauz_core::bill::{BillId, Status};
use hauz_core::store::{BillStore, SqliteStore};

#[tokio::test]
async fn ac1_creates_new_bill() -> common::Result<()> {
    let dir = common::tmp_dir();
    let bill = common::fixture("bill.eml");
    let db = dir.join("a.db");
    let hash = common::hash_hex(&std::fs::read(&bill)?);

    let output = Command::cargo_bin("hauz")?
        .current_dir(&dir)
        .arg("ingest")
        .arg(&bill)
        .arg("--db")
        .arg(&db)
        .output()?;

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout)?,
        format!("Created {hash}\n")
    );
    assert!(output.stderr.is_empty());
    assert_eq!(common::ids(&db).await?, vec![hash]);
    Ok(())
}

#[tokio::test]
async fn ac2_reports_duplicate_on_second_ingest() -> common::Result<()> {
    let dir = common::tmp_dir();
    let bill = common::fixture("bill.eml");
    let db = dir.join("a.db");
    let hash = common::hash_hex(&std::fs::read(&bill)?);

    let first = Command::cargo_bin("hauz")?
        .current_dir(&dir)
        .arg("ingest")
        .arg(&bill)
        .arg("--db")
        .arg(&db)
        .output()?;
    assert_eq!(first.status.code(), Some(0));

    let second = Command::cargo_bin("hauz")?
        .current_dir(&dir)
        .arg("ingest")
        .arg("--db")
        .arg(&db)
        .arg(&bill)
        .output()?;

    assert_eq!(second.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(second.stdout)?,
        format!("Duplicate {hash}\n")
    );
    assert_eq!(common::ids(&db).await?, vec![hash]);
    Ok(())
}

#[test]
fn ac3_fails_when_path_missing() -> common::Result<()> {
    let dir = common::tmp_dir();
    let missing = dir.join("missing.eml");
    let db = dir.join("a.db");

    let output = Command::cargo_bin("hauz")?
        .current_dir(&dir)
        .arg("ingest")
        .arg(&missing)
        .arg("--db")
        .arg(&db)
        .output()?;

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains(&missing.to_string_lossy().into_owned()));
    assert!(!db.exists());
    Ok(())
}

#[tokio::test]
async fn ac4_fails_on_malformed_message() -> common::Result<()> {
    let dir = common::tmp_dir();
    let malformed = common::fixture("malformed.eml");
    let db = dir.join("a.db");

    let output = Command::cargo_bin("hauz")?
        .current_dir(&dir)
        .arg("ingest")
        .arg(&malformed)
        .arg("--db")
        .arg(&db)
        .output()?;

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    assert!(common::ids(&db).await?.is_empty());
    Ok(())
}

#[test]
fn ac5_usage_errors_exit_2() -> common::Result<()> {
    let dir = common::tmp_dir();
    let bill = common::fixture("bill.eml");

    let cases: Vec<Vec<OsString>> = vec![
        vec![],
        vec!["frobnicate".into()],
        vec!["ingest".into()],
        vec![
            "ingest".into(),
            bill.clone().into_os_string(),
            "--verbose".into(),
        ],
    ];

    for args in cases {
        let output = Command::cargo_bin("hauz")?
            .current_dir(&dir)
            .args(&args)
            .output()?;
        assert_eq!(output.status.code(), Some(2), "args: {args:?}");
        assert!(output.stdout.is_empty(), "args: {args:?}");
        let stderr = String::from_utf8(output.stderr)?;
        assert!(stderr.contains("usage: hauz ingest"), "args: {args:?}");
    }
    Ok(())
}

#[tokio::test]
async fn ac6_defaults_db_to_cwd_hauz_db() -> common::Result<()> {
    let dir = common::tmp_dir();
    let bill = common::fixture("bill.eml");
    let hash = common::hash_hex(&std::fs::read(&bill)?);

    let output = Command::cargo_bin("hauz")?
        .current_dir(&dir)
        .arg("ingest")
        .arg(&bill)
        .output()?;

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout)?,
        format!("Created {hash}\n")
    );
    let db = dir.join("hauz.db");
    assert!(db.exists());
    assert_eq!(common::ids(&db).await?, vec![hash]);
    Ok(())
}

#[test]
fn ac7_help_flag_prints_usage() -> common::Result<()> {
    let dir = common::tmp_dir();

    for flag in ["--help", "-h"] {
        let output = Command::cargo_bin("hauz")?
            .current_dir(&dir)
            .arg(flag)
            .output()?;
        assert_eq!(output.status.code(), Some(0), "flag: {flag}");
        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("usage: hauz ingest"), "flag: {flag}");
        assert!(output.stderr.is_empty(), "flag: {flag}");
    }
    Ok(())
}

/// AC7 (#26): `HAUZ_LLM_PROVIDER=anthropic` with `HAUZ_LLM_MODEL` set but
/// `ANTHROPIC_API_KEY` removed exits 1, names the missing variable on stderr, and creates no
/// DB file (`Config::from_env` is read before the store opens).
#[test]
fn ac7_missing_llm_api_key_env_var_exits_1_and_creates_no_db() -> common::Result<()> {
    let dir = common::tmp_dir();
    let bill = common::fixture("bill.eml");
    let db = dir.join("a.db");

    let output = Command::cargo_bin("hauz")?
        .current_dir(&dir)
        .arg("ingest")
        .arg(&bill)
        .arg("--db")
        .arg(&db)
        .env("HAUZ_LLM_PROVIDER", "anthropic")
        .env("HAUZ_LLM_MODEL", "m")
        .env_remove("ANTHROPIC_API_KEY")
        .output()?;

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("ANTHROPIC_API_KEY"), "stderr: {stderr}");
    assert!(!db.exists());
    Ok(())
}

/// AC8 (#26): `HAUZ_LLM_PROVIDER=ollama` pointed at a refusing loopback port (so the escalated
/// `LlmExtractor` call fails fast rather than hanging) still exits 0, prints `Created <hash>`,
/// and stores the bill `NeedsReview` (the heuristic alone leaves no period, and the degraded
/// LLM pass contributes none either).
#[tokio::test]
async fn ac8_llm_client_failure_degrades_to_needs_review_exit_0() -> common::Result<()> {
    let dir = common::tmp_dir();
    let bill = common::fixture("bill.eml");
    let db = dir.join("a.db");
    let hash = common::hash_hex(&std::fs::read(&bill)?);

    let output = Command::cargo_bin("hauz")?
        .current_dir(&dir)
        .arg("ingest")
        .arg(&bill)
        .arg("--db")
        .arg(&db)
        .env("HAUZ_LLM_PROVIDER", "ollama")
        .env("HAUZ_LLM_MODEL", "m")
        .env("OLLAMA_API_BASE_URL", "http://127.0.0.1:1")
        .output()?;

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout)?,
        format!("Created {hash}\n")
    );

    let store = SqliteStore::open(&db).await?;
    let bill = store
        .get(&BillId::new(&hash)?)
        .await?
        .ok_or("missing bill")?;
    assert_eq!(bill.status(), Status::NeedsReview);
    Ok(())
}
