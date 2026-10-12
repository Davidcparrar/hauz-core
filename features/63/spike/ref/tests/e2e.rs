use http_body_util::BodyExt;
use topcoat::router::{Body, request::Request};

async fn get(r: &topcoat::router::Router, uri: &str) -> (u16, String) {
    let resp = r.handle(Request::get(uri).body(Body::default()).unwrap()).await;
    let s = resp.status().as_u16();
    let b = resp.into_body().collect().await.unwrap().to_bytes();
    (s, String::from_utf8(b.to_vec()).unwrap())
}

#[tokio::test]
async fn renders_in_process() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("t.db");
    hauz_core::store::SqliteStore::open(&p).await.unwrap();
    let store = hauz_core::store::SqliteStore::open_read_only(&p).await.unwrap();
    let r = spike_63::router(store);
    let (s, b) = get(&r, "/admin").await;
    assert_eq!(s, 200);
    assert!(b.contains("0 in db"), "{b}");
    let (s, b) = get(&r, "/admin/bills/42").await;
    assert_eq!((s, b.contains("Bill 42")), (200, true), "{b}");
    let (s, b) = get(&r, "/style").await;
    assert!(s == 200 && b.contains("--radius"));
}
