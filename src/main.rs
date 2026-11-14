use solv::{build_router, resolve_port};

#[tokio::main]
async fn main() {
    // A missing .env is not an error: CI boots the binary with none present.
    dotenvy::dotenv().ok();

    let port = resolve_port(std::env::var("PORT").ok().as_deref()).unwrap_or_else(|error| {
        eprintln!("solv: {error}");
        std::process::exit(1);
    });

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .unwrap_or_else(|e| panic!("failed to bind 0.0.0.0:{port}: {e}"));

    // Log the bound address, not the requested port: with PORT=0 the real
    // port is assigned by the OS.
    let addr = listener
        .local_addr()
        .expect("bound listener has an address");
    println!("solv listening on {addr}");
    axum::serve(listener, build_router())
        .await
        .expect("server error");
}
