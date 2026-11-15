use solv::bureau::{Bureau, BureauConfig};
use solv::state::Store;
use solv::tier1::{ClefClient, CloudflareClefClient, UnconfiguredClefClient};
use solv::tier2::{ServClient, ServReasoningClient, UnconfiguredServClient};
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

    let tier1: std::sync::Arc<dyn ClefClient> = match CloudflareClefClient::from_env() {
        Some(client) => std::sync::Arc::new(client),
        None => {
            eprintln!(
                "solv: CLOUDFLARE_ACCOUNT_ID/CLOUDFLARE_API_TOKEN not set; \
                 pre-spend-check will fail closed until credentials exist"
            );
            std::sync::Arc::new(UnconfiguredClefClient)
        }
    };
    let tier2: std::sync::Arc<dyn ServClient> = match ServReasoningClient::from_env() {
        Some(client) => std::sync::Arc::new(client),
        None => {
            eprintln!("solv: SERV_API_KEY not set; escalated checks will fail closed");
            std::sync::Arc::new(UnconfiguredServClient)
        }
    };
    // The Reppo audit worker attaches only when the operator enabled Reppo
    // and gave it a key (AGENTS.md: credits are non-renewable, so a key in
    // `.env` alone must not let the service spend them). The starting
    // credit budget is explicit: SOLV_REPPO_STARTING_CREDITS, default 0,
    // which attaches the worker but refuses to queue anything — the
    // directory reports the live counter and the endpoint returns 402.
    let bureau = solv::reppo::ReppoHttpClient::from_env().map(|client| {
        let credits = match std::env::var("SOLV_REPPO_STARTING_CREDITS").ok().as_deref() {
            None | Some("") => 0,
            Some(raw) => raw.parse::<u32>().unwrap_or_else(|error| {
                eprintln!("solv: invalid SOLV_REPPO_STARTING_CREDITS value: {error}");
                std::process::exit(1);
            }),
        };
        let config = BureauConfig {
            starting_credits: credits,
            ..BureauConfig::default()
        };
        let (bureau, worker) = Bureau::spawn(
            std::sync::Arc::new(client),
            std::sync::Arc::clone(&store),
            config,
        );
        // The worker detaches: `run_worker` loops forever holding its own
        // store handle, so the task ends only when the runtime drops at
        // process exit — exactly the lifetime a queue worker wants.
        drop(worker);
        println!("solv: reppo audit worker attached ({credits} starting credits)");
        std::sync::Arc::new(bureau)
    });
    if bureau.is_none() {
        println!(
            "solv: reppo audit worker not attached (SOLV_REPPO_ENABLED=1 and REPPO_API_KEY required)"
        );
    }

    // The §2.1 latency ceiling stays the SPEC default unless the operator
    // raises it for a slower route (SOLV_LATENCY_CEILING_MS).
    let mut policy_config = solv::policy::PolicyConfig::default();
    match std::env::var("SOLV_LATENCY_CEILING_MS").ok().as_deref() {
        None | Some("") => {}
        Some(raw) => match raw.parse::<u64>() {
            Ok(ms) if ms > 0 => policy_config.latency_ceiling_ms = ms,
            _ => {
                eprintln!("solv: invalid SOLV_LATENCY_CEILING_MS value: {raw}");
                std::process::exit(1);
            }
        },
    }

    // Public-deploy guards (SPEC §8.2). Invalid values stop the boot: a
    // typo must never silently loosen a guard.
    let limits = solv::guard::LimitsConfig::from_vars(
        std::env::var("SOLV_RATE_LIMIT_PER_MIN").ok().as_deref(),
        std::env::var("SOLV_MAX_CONCURRENT_CHECKS").ok().as_deref(),
        std::env::var("SOLV_TRUST_PROXY").ok().as_deref(),
    )
    .unwrap_or_else(|error| {
        eprintln!("solv: {error}");
        std::process::exit(1);
    });
    let admin_token = std::env::var("SOLV_ADMIN_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());
    if let Some(token) = &admin_token {
        if token.chars().count() < solv::guard::MIN_ADMIN_TOKEN_CHARS {
            eprintln!(
                "solv: SOLV_ADMIN_TOKEN must be at least {} characters",
                solv::guard::MIN_ADMIN_TOKEN_CHARS
            );
            std::process::exit(1);
        }
    } else {
        println!("solv: SOLV_ADMIN_TOKEN not set; audit-counterparty is closed (401)");
    }

    let state = AppState::new(store, tier1, tier2)
        .with_bureau(bureau)
        .with_policy_config(policy_config)
        .with_limits(limits)
        .with_admin_token(admin_token.as_deref());

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
