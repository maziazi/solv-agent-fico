//! Boot-level integration test: drives the real serving path — bind,
//! `axum::serve`, HTTP over TCP — rather than the in-process router alone
//! (which `src/lib.rs` unit tests already cover).

use solv::state::Store;
use solv::tier1::{ClefClient, MockClefClient};
use solv::{AppState, build_router};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn healthz_answers_200_over_real_tcp() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state.json")).unwrap();
    let tier1: std::sync::Arc<dyn ClefClient> = std::sync::Arc::new(MockClefClient::default());
    let tier2: std::sync::Arc<dyn solv::tier2::ServClient> =
        std::sync::Arc::new(solv::tier2::MockServClient::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, build_router(AppState::new(store, tier1, tier2)))
            .await
            .unwrap();
    });

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let response = String::from_utf8_lossy(&response);
    assert!(
        response.starts_with("HTTP/1.1 200 OK\r\n"),
        "unexpected status line in: {response}"
    );
    assert!(response.ends_with("ok"), "unexpected body in: {response}");
}
