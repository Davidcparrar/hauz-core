//! [e2e] acceptance tests for the `hauz` binary (feature #8), plus its `HAUZ_LLM_PROVIDER`
//! env wiring (#26). Every test shells out via `assert_cmd::Command::cargo_bin("hauz")`,
//! inspecting the process's exit code, stdout, stderr and the `SqliteStore` it leaves behind.

mod common;

use std::ffi::OsString;

use assert_cmd::Command;
use hauz_core::bill::{BillId, Currency, Money, Status};
use hauz_core::store::{BillStore, SqliteStore};

// `include_bytes!` of the `core` crate's own fixtures (#27): `cli` has no corpus of its own
// for DIAN zips, and the spec forbids duplicating them under `crates/cli/tests/fixtures/`.
const DIAN_FULL_EML: &[u8] = include_bytes!("../../core/tests/fixtures/ubl/dian_full.eml");
const DIAN_NO_PERIOD_EML: &[u8] =
    include_bytes!("../../core/tests/fixtures/ubl/dian_no_period.eml");
const BILL_WITH_BZIP2_EML: &[u8] =
    include_bytes!("../../core/tests/fixtures/zip/bill_with_bzip2.eml");
const DIAN_CORRUPT_EML: &[u8] = include_bytes!("../../core/tests/fixtures/ubl/dian_corrupt.eml");

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

/// AC8 (#27): `hauz ingest dian_full.eml --db <tmp>` exits 0, prints `Created <id>`, and
/// leaves an `Extracted` bill with the AC1 amount.
#[tokio::test]
async fn ac8_dian_full_eml_creates_extracted_bill() -> common::Result<()> {
    let dir = common::tmp_dir();
    let eml = dir.join("dian_full.eml");
    std::fs::write(&eml, DIAN_FULL_EML)?;
    let db = dir.join("a.db");
    let hash = common::hash_hex(DIAN_FULL_EML);

    let output = Command::cargo_bin("hauz")?
        .current_dir(&dir)
        .arg("ingest")
        .arg(&eml)
        .arg("--db")
        .arg(&db)
        .output()?;

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout)?,
        format!("Created {hash}\n")
    );
    assert!(output.stderr.is_empty());

    let store = SqliteStore::open(&db).await?;
    let bill = store
        .get(&BillId::new(&hash)?)
        .await?
        .ok_or("missing bill")?;
    assert_eq!(bill.status(), Status::Extracted);
    assert_eq!(
        bill.amount(),
        Some(&Money::new(18_435_000, Currency::new("COP")?))
    );
    Ok(())
}

/// AC9 (#27): `hauz ingest dian_corrupt.eml --db <tmp>` (a truncated DIAN zip attachment)
/// exits 1, writes stderr, and stores nothing.
#[tokio::test]
async fn ac9_dian_corrupt_eml_exits_1_stores_nothing() -> common::Result<()> {
    let dir = common::tmp_dir();
    let eml = dir.join("dian_corrupt.eml");
    std::fs::write(&eml, DIAN_CORRUPT_EML)?;
    let db = dir.join("a.db");

    let output = Command::cargo_bin("hauz")?
        .current_dir(&dir)
        .arg("ingest")
        .arg(&eml)
        .arg("--db")
        .arg(&db)
        .output()?;

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    assert!(common::ids(&db).await?.is_empty());
    Ok(())
}

/// AC11 (#37): `hauz ingest dian_no_period.eml --db <tmp>` exits 0, prints `Created <id>`, and
/// stores an `Extracted` bill with issued 2026-09-10.
#[tokio::test]
async fn ac11_dian_no_period_eml_is_extracted_with_issued() -> common::Result<()> {
    let dir = common::tmp_dir();
    let eml = dir.join("dian_no_period.eml");
    std::fs::write(&eml, DIAN_NO_PERIOD_EML)?;
    let db = dir.join("a.db");
    let hash = common::hash_hex(DIAN_NO_PERIOD_EML);

    let output = Command::cargo_bin("hauz")?
        .current_dir(&dir)
        .arg("ingest")
        .arg(&eml)
        .arg("--db")
        .arg(&db)
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
    assert_eq!(bill.status(), Status::Extracted);
    assert_eq!(
        bill.issued().map(|date| date.to_string()),
        Some("2026-09-10".to_owned())
    );
    Ok(())
}

/// AC7 (#36): `hauz ingest bill_with_bzip2.eml --db <tmp>` exits 0, prints `Created <id>`, and
/// stores the bill with amount 1,234.56 EUR.
#[tokio::test]
async fn ac7_bill_with_unsupported_zip_is_created() -> common::Result<()> {
    let dir = common::tmp_dir();
    let eml = dir.join("bill_with_bzip2.eml");
    std::fs::write(&eml, BILL_WITH_BZIP2_EML)?;
    let db = dir.join("a.db");
    let hash = common::hash_hex(BILL_WITH_BZIP2_EML);

    let output = Command::cargo_bin("hauz")?
        .current_dir(&dir)
        .arg("ingest")
        .arg(&eml)
        .arg("--db")
        .arg(&db)
        .output()?;

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout)?,
        format!("Created {hash}\n")
    );
    assert!(output.stderr.is_empty());

    let store = SqliteStore::open(&db).await?;
    let bill = store
        .get(&BillId::new(&hash)?)
        .await?
        .ok_or("missing bill")?;
    assert_eq!(
        bill.amount(),
        Some(&Money::new(123_456, Currency::new("EUR")?))
    );
    Ok(())
}

/// A fake Google: `/token` answers `token`, the list serves `ids`, and `/messages/{id}`
/// serves the matching raw message (base64url).
async fn fake_gmail(
    token: (u16, &'static str),
    messages: Vec<(&'static str, Vec<u8>)>,
) -> common::Result<String> {
    use std::collections::HashMap;
    use std::sync::Arc;

    use axum::Router;
    use axum::extract::{Path, State};
    use axum::http::StatusCode;
    use axum::routing::{get, post};

    let list = format!(
        r#"{{"messages":[{}]}}"#,
        messages
            .iter()
            .map(|(id, _)| format!(r#"{{"id":"{id}"}}"#))
            .collect::<Vec<_>>()
            .join(",")
    );
    let raws: Arc<HashMap<String, String>> = Arc::new(
        messages
            .iter()
            .map(|(id, raw)| ((*id).to_owned(), base64url(raw)))
            .collect(),
    );
    let app = Router::new()
        .route(
            "/token",
            post(move || async move {
                (
                    StatusCode::from_u16(token.0).unwrap_or(StatusCode::BAD_GATEWAY),
                    token.1,
                )
            }),
        )
        .route(
            "/gmail/v1/users/me/messages",
            get(move || async move { list }),
        )
        .route(
            "/gmail/v1/users/me/messages/{id}",
            get(
                |State(raws): State<Arc<HashMap<String, String>>>, Path(id): Path<String>| async move {
                    let raw = raws.get(&id).cloned().unwrap_or_default();
                    format!(r#"{{"raw":"{raw}"}}"#)
                },
            ),
        )
        .with_state(raws);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(base)
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            let index = ((n >> (18 - 6 * i)) & 0x3f) as usize;
            out.extend(ALPHABET.get(index).copied().map(char::from));
        }
    }
    out
}

fn fetch_command(dir: &std::path::Path, base: &str) -> common::Result<Command> {
    let mut command = Command::cargo_bin("hauz")?;
    command
        .current_dir(dir)
        .env_remove("HAUZ_LLM_PROVIDER")
        .env("HAUZ_GMAIL_CLIENT_ID", "cid")
        .env("HAUZ_GMAIL_CLIENT_SECRET", "sec")
        .env("HAUZ_GMAIL_REFRESH_TOKEN", "REFRESH-VALUE")
        .env("HAUZ_GMAIL_LABEL", "bills")
        .env("HAUZ_GMAIL_TOKEN_URL", format!("{base}/token"))
        .env("HAUZ_GMAIL_API_BASE", base);
    Ok(command)
}

const TOKEN_OK: (u16, &str) = (200, r#"{"access_token":"at","expires_in":3600}"#);

#[tokio::test(flavor = "multi_thread")]
async fn ac7_fetch_creates_then_duplicates() -> common::Result<()> {
    let dir = common::tmp_dir();
    let db = dir.join("f.db");
    let first = std::fs::read(common::fixture("bill.eml"))?;
    let second = String::from_utf8(first.clone())?
        .replace("Your bill", "Your other bill")
        .into_bytes();
    let base = fake_gmail(
        TOKEN_OK,
        vec![("m1", first.clone()), ("m2", second.clone())],
    )
    .await?;
    let expected = format!(
        "{}{}",
        format_args!("Created {}\n", common::hash_hex(&first)),
        format_args!("Created {}\n", common::hash_hex(&second)),
    );

    let output = fetch_command(&dir, &base)?
        .arg("fetch")
        .arg("--db")
        .arg(&db)
        .output()?;
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8(output.stdout)?, expected);

    let rerun = fetch_command(&dir, &base)?
        .arg("fetch")
        .arg("--db")
        .arg(&db)
        .output()?;
    assert_eq!(rerun.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(rerun.stdout)?,
        expected.replace("Created", "Duplicate")
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn ac8_fetch_config_and_auth_failures_exit_1() -> common::Result<()> {
    let dir = common::tmp_dir();
    let db = dir.join("missing.db");
    let base = fake_gmail(TOKEN_OK, vec![]).await?;

    let output = fetch_command(&dir, &base)?
        .env_remove("HAUZ_GMAIL_REFRESH_TOKEN")
        .arg("fetch")
        .arg("--db")
        .arg(&db)
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stderr)?.contains("HAUZ_GMAIL_REFRESH_TOKEN"));
    assert!(!db.exists());

    let rejecting = fake_gmail((400, r#"{"error":"invalid_grant"}"#), vec![]).await?;
    let auth = fetch_command(&dir, &rejecting)?
        .arg("fetch")
        .arg("--db")
        .arg(dir.join("auth.db"))
        .output()?;
    assert_eq!(auth.status.code(), Some(1));
    let stderr = String::from_utf8(auth.stderr)?;
    assert!(stderr.contains("auth"), "{stderr}");
    assert!(!stderr.contains("REFRESH-VALUE"), "{stderr}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn ac10_fetch_bounds_query_by_date_range_or_exits_2() -> common::Result<()> {
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::extract::Query;
    use axum::routing::{get, post};

    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let recorder = Arc::clone(&seen);
    let app = Router::new()
        .route("/token", post(|| async { TOKEN_OK.1 }))
        .route(
            "/gmail/v1/users/me/messages",
            get(
                move |Query(params): Query<std::collections::HashMap<String, String>>| async move {
                    if let (Ok(mut queries), Some(q)) = (recorder.lock(), params.get("q")) {
                        queries.push(q.clone());
                    }
                    "{}"
                },
            ),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let dir = common::tmp_dir();
    let output = fetch_command(&dir, &base)?
        .args(["fetch", "--before", "2026-10-01", "--after", "2026-09-01"])
        .arg("--db")
        .arg(dir.join("ok.db"))
        .output()?;
    assert_eq!(output.status.code(), Some(0));
    let queries = seen.lock().map_err(|e| e.to_string())?.clone();
    assert_eq!(
        queries,
        vec!["label:bills after:1788220800 before:1790812800".to_owned()]
    );

    for (after, before) in [
        ("2026-9-01", "2026-10-01"),
        ("2026-10-01", "2026-10-01"),
        ("2026-10-02", "2026-10-01"),
    ] {
        let db = dir.join("bad.db");
        let bad = fetch_command(&dir, &base)?
            .args(["fetch", "--after", after, "--before", before])
            .arg("--db")
            .arg(&db)
            .output()?;
        assert_eq!(bad.status.code(), Some(2), "{after} {before}");
        assert!(!String::from_utf8(bad.stderr)?.is_empty());
        assert!(!db.exists());
    }
    Ok(())
}
