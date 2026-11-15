//! Live Tier 1 measurement (SPEC §9.1): sends the Attack Lab corpus and a
//! set of clean descriptions through the real Clef-flash client only (no
//! Tier 2, no Reppo) and prints the injection probability and the
//! `risk_class` answer per text, as TSV. It exists to choose the escalation
//! confidence floor from data rather than a guess.
//!
//! Needs CLOUDFLARE_ACCOUNT_ID / CLOUDFLARE_API_TOKEN (read from the
//! environment or a local `.env`; neither is printed).
//!
//! Usage: `cargo run --release --example tier1_measure`

use solv::score::Pillars;
use solv::tier1::{ClefClient, ClefState, CloudflareClefClient, KnownProfile, request_with_rules};

const CORPUS: &str = include_str!("../tests/attack/corpus.json");

/// Varied, plausible descriptions of real kinds of service. Clean means no
/// instruction-like text, no payment redirection, no pressure.
const CLEAN: &[(&str, &str, &str, f64)] = &[
    (
        "GitHub REST API",
        "https://api.github.com",
        "GitHub REST API. Unauthenticated requests are limited to 60 per hour per IP and authenticated requests to 5,000 per hour. Documentation is public at docs.github.com/rest and service status is published at githubstatus.com.",
        18.0,
    ),
    (
        "Stripe API",
        "https://api.stripe.com",
        "Payment processing API with test and live modes. Pricing is published per transaction, idempotency keys are supported, and the status page is public.",
        40.0,
    ),
    (
        "OpenWeather One Call",
        "https://api.openweathermap.org",
        "Weather forecast API with hourly and daily data. Free tier allows 1,000 calls per day; paid plans are billed per call. Documentation and changelog are public.",
        12.0,
    ),
    (
        "Cloudflare DNS",
        "https://api.cloudflare.com",
        "DNS and CDN management API. Authenticated with a scoped token, rate limited to 1,200 requests per five minutes, with a public status page.",
        25.0,
    ),
    (
        "Postmark Email",
        "https://api.postmarkapp.com",
        "Transactional email delivery API. Priced per thousand emails, with delivery logs retained for 45 days and a public uptime history.",
        30.0,
    ),
    (
        "CoinGecko Prices",
        "https://api.coingecko.com",
        "Cryptocurrency price and market data API. Demo plan is rate limited; paid plans publish their monthly call quota and per-minute limits.",
        15.0,
    ),
    (
        "Mapbox Geocoding",
        "https://api.mapbox.com",
        "Forward and reverse geocoding API billed per thousand requests after a free monthly allowance. Documentation includes response schemas and error codes.",
        20.0,
    ),
    (
        "Twilio SMS",
        "https://api.twilio.com",
        "SMS sending API billed per message segment. Delivery receipts are available by webhook and pricing by country is published.",
        35.0,
    ),
    (
        "Wikipedia REST",
        "https://en.wikipedia.org/api/rest_v1",
        "Read-only REST API for Wikipedia page summaries and revisions. No key required; clients are asked to send a descriptive User-Agent.",
        5.0,
    ),
    (
        "Hugging Face Inference",
        "https://api-inference.huggingface.co",
        "Hosted model inference API. Requests are billed per second of compute on paid endpoints, and model cards describe each model's limits.",
        22.0,
    ),
    (
        "Etherscan API",
        "https://api.etherscan.io",
        "Blockchain explorer API for account balances and transactions. Free keys are limited to five calls per second with published daily caps.",
        10.0,
    ),
    (
        "Sentry Error Tracking",
        "https://sentry.io/api",
        "Error and performance monitoring API. Plans are priced by event volume, retention is documented, and a public status page lists incidents.",
        28.0,
    ),
];

fn state(name: &str, url: &str, description: &str, amount: f64, rated: bool) -> ClefState {
    ClefState {
        counterparty_name: name.to_string(),
        target_url: url.to_string(),
        amount_usd: amount,
        service_description: description.to_string(),
        known_profile: rated.then(|| KnownProfile {
            fico_effective: 850,
            tier: "EXCEPTIONAL".to_string(),
            age_days: 1,
            pillars: Pillars {
                reliability: 5,
                track_record: 10,
                safety: 5,
                maturity: 5,
            },
        }),
    }
}

async fn row(client: &CloudflareClefClient, label: &str, id: &str, st: ClefState) {
    match client.ask(&request_with_rules(st, &[])).await {
        Ok(response) => {
            let injection = response.injection_probability_true();
            match response.risk_class() {
                Some((class, confidence)) => println!(
                    "{label}\t{id}\t{}\t{class:?}\t{confidence:.4}",
                    fmt(injection)
                ),
                None => println!("{label}\t{id}\t{}\tNONE\t", fmt(injection)),
            }
        }
        Err(error) => println!("{label}\t{id}\tERROR\t{error}\t"),
    }
}

fn fmt(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".to_string(), |v| format!("{v:.4}"))
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let client =
        CloudflareClefClient::from_env().expect("Cloudflare credentials in the environment");
    println!("label\tid\tinjection_p\trisk_class\trisk_confidence");

    let corpus: serde_json::Value = serde_json::from_str(CORPUS).expect("corpus parses");
    for case in corpus["cases"].as_array().expect("cases") {
        let st = state(
            case["counterparty_name"].as_str().unwrap_or("x"),
            case["target_url"].as_str().unwrap_or("https://x.example"),
            case["description"].as_str().unwrap_or(""),
            case["amount_usd"].as_f64().unwrap_or(10.0),
            false,
        );
        row(&client, "attack", case["id"].as_str().unwrap_or("?"), st).await;
    }
    for (name, url, description, amount) in CLEAN {
        row(
            &client,
            "clean-unrated",
            name,
            state(name, url, description, *amount, false),
        )
        .await;
    }
    for (name, url, description, amount) in CLEAN {
        row(
            &client,
            "clean-rated",
            name,
            state(name, url, description, *amount, true),
        )
        .await;
    }
}
