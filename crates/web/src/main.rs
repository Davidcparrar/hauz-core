//! `hauz-web [--db <path>] [--port <n>]`: thin shell. Args, then the read-only DB open, then
//! serve on `127.0.0.1`. Covered via `tests/e2e_cli.rs`.

mod args;

use std::env;
use std::net::Ipv4Addr;

use anyhow::Context;
use hauz_core::store::SqliteStore;
use tokio::net::TcpListener;

use args::{USAGE, parse};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let Ok(args) = parse(env::args_os().skip(1)) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    let store = SqliteStore::open_read_only(&args.db)
        .await
        .with_context(|| format!("opening {}", args.db.display()))?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, args.port))
        .await
        .with_context(|| format!("binding 127.0.0.1:{}", args.port))?;
    topcoat::serve(listener, hauz_web::router(store)).await?;
    Ok(())
}
