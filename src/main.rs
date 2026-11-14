use solv::build_router;

#[tokio::main]
async fn main() {
    // A missing .env is not an error: CI boots the binary with none present.
    dotenvy::dotenv().ok();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .unwrap_or_else(|e| panic!("failed to bind 0.0.0.0:{port}: {e}"));

    println!("solv listening on 0.0.0.0:{port}");
    axum::serve(listener, build_router())
        .await
        .expect("server error");
}
