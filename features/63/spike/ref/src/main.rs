use std::{net::SocketAddr, path::PathBuf};
#[tokio::main]
async fn main() -> std::io::Result<()> {
    let db = PathBuf::from(std::env::args().nth(1).unwrap());
    let port: u16 = std::env::args().nth(2).unwrap().parse().unwrap();
    let store = hauz_core::store::SqliteStore::open_read_only(&db).await.unwrap();
    let l = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await?;
    topcoat::serve(l, spike_63::router(store)).await
}
