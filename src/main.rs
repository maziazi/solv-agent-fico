use solv::state::Store;
use solv::{AppState, resolve_port, resolve_state_file, run_server};

/// Resolves on Ctrl-C (any platform) or SIGTERM (Unix), whichever first.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("ctrl_c handler installed");
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        signal(SignalKind::terminate())
            .expect("SIGTERM handler installed")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

#[tokio::main]
async fn main() {
    // A missing .env is not an error: CI boots the binary with none present.
    dotenvy::dotenv().ok();

    let port = resolve_port(std::env::var("PORT").ok().as_deref()).unwrap_or_else(|error| {
        eprintln!("solv: {error}");
        std::process::exit(1);
    });

    let state_file = resolve_state_file(std::env::var("STATE_FILE").ok().as_deref());
    let store = Store::open(&state_file).unwrap_or_else(|error| {
        eprintln!(
            "solv: failed to load state from {}: {error}",
            state_file.display()
        );
        std::process::exit(1);
    });
    store.spawn_snapshot_worker().await;
    let state = AppState::new(store);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .unwrap_or_else(|e| panic!("failed to bind 0.0.0.0:{port}: {e}"));

    // Log the bound address, not the requested port: with PORT=0 the real
    // port is assigned by the OS.
    let addr = listener
        .local_addr()
        .expect("bound listener has an address");
    println!("solv listening on {addr}");
    run_server(listener, state, shutdown_signal())
        .await
        .expect("server error");
}
