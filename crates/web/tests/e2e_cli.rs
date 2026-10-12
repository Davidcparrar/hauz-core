//! [e2e] acceptance tests for the `hauz-web` binary (feature #63).

mod common;

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::process::Stdio;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use common::Result;

fn free_port() -> Result<u16> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    Ok(listener.local_addr()?.port())
}

fn http_get(port: u16, path: &str) -> Result<String> {
    let mut stream = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        Duration::from_secs(1),
    )?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

#[tokio::test]
async fn ac8_binary_serves_admin_on_given_port() -> Result<()> {
    let db = common::tmp_dir().join("t.db");
    common::seed(&db, &[common::extracted()?]).await?;
    let port = free_port()?;
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("hauz-web"))
        .arg("--db")
        .arg(&db)
        .arg("--port")
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let deadline = Instant::now() + Duration::from_secs(20);
    let outcome = loop {
        match http_get(port, "/admin") {
            Ok(response) => break Ok(response),
            Err(e) if Instant::now() >= deadline => break Err(e),
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    };
    child.kill()?;
    child.wait()?;

    let response = outcome?;
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("Acme Power"), "{response}");
    Ok(())
}

#[test]
fn ac9_missing_db_exits_1_and_bogus_flag_exits_2() -> Result<()> {
    let dir = common::tmp_dir();
    let db = dir.join("missing.db");
    let output = Command::cargo_bin("hauz-web")?
        .current_dir(&dir)
        .arg("--db")
        .arg(&db)
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stderr)?.contains(&db.display().to_string()));
    assert!(!db.exists());

    let output = Command::cargo_bin("hauz-web")?.arg("--bogus").output()?;
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8(output.stderr)?.contains("usage: hauz-web"));
    Ok(())
}
