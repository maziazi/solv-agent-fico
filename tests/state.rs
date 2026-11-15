//! Integration tests for the state layer (docs/SPEC.md §5.6, milestone M1):
//! state survives restart, an interrupted write leaves no corrupt file, and
//! the debounced snapshot worker persists after quiescence.

use std::time::Duration;

use solv::score::effective_score;
use solv::state::{LearnedRule, Profile, Receipt, RuleStatus, SeedOutcome, Store, StoreError};

fn sample_profile(name: &str) -> Profile {
    Profile {
        counterparty_name: name.to_string(),
        target_url: format!("https://{name}.example.com"),
        score: None,
    }
}

#[test]
fn snapshot_roundtrip_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("solv_state.json");

    let store = Store::open(&path).unwrap();
    store.upsert_profile("cp-1".to_string(), sample_profile("DataFeed Pro"));
    store.upsert_profile("cp-2".to_string(), sample_profile("Translate Bot"));
    assert!(store.insert_receipt(
        "slv_001".to_string(),
        Receipt {
            decision: "BLOCK".to_string(),
            amount_usd: 50.0,
            created_at_unix: 1_762_700_000,
            ..Receipt::default()
        },
    ));
    store.set_learned_rules(vec![LearnedRule {
        rule_id: "SOLV-RULE-001".to_string(),
        status: RuleStatus::Shadow,
        ..LearnedRule::default()
    }]);
    store.write_snapshot_now().unwrap();
    drop(store);

    let reopened = Store::open(&path).unwrap();
    assert_eq!(
        reopened.profile("cp-1"),
        Some(sample_profile("DataFeed Pro"))
    );
    assert_eq!(
        reopened.profile("cp-2"),
        Some(sample_profile("Translate Bot"))
    );
    assert_eq!(
        reopened.receipt("slv_001").map(|r| r.decision),
        Some("BLOCK".to_string())
    );
    assert_eq!(reopened.learned_rules().len(), 1);
    assert_eq!(reopened.learned_rules()[0].status, RuleStatus::Shadow);
}

#[test]
fn interrupted_write_leaves_previous_file_intact() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("solv_state.json");

    // A good snapshot exists on disk…
    let store = Store::open(&path).unwrap();
    store.upsert_profile("cp-1".to_string(), sample_profile("DataFeed Pro"));
    store.write_snapshot_now().unwrap();
    drop(store);

    // …then the process dies mid-write: the .tmp sibling holds a partial
    // payload and the rename never happens.
    let tmp = dir.path().join("solv_state.json.tmp");
    std::fs::write(
        &tmp,
        "{\"schema_version\":1,\"counterparties\":{\"cp-1\":{\"cou",
    )
    .unwrap();

    let reopened = Store::open(&path).unwrap();
    assert_eq!(
        reopened.profile("cp-1"),
        Some(sample_profile("DataFeed Pro")),
        "the destination file must still hold the last complete snapshot"
    );

    // The next successful write cleans the stale .tmp up.
    reopened.upsert_profile("cp-2".to_string(), sample_profile("Translate Bot"));
    reopened.write_snapshot_now().unwrap();
    assert!(!tmp.exists());
    assert_eq!(
        Store::open(&path).unwrap().profile("cp-2"),
        Some(sample_profile("Translate Bot"))
    );
}

#[test]
fn missing_state_file_starts_empty() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("does-not-exist.json")).unwrap();
    assert!(store.list_profiles().is_empty());
    assert!(store.learned_rules().is_empty());
}

#[test]
fn corrupt_state_file_is_rejected_loudly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("solv_state.json");
    std::fs::write(&path, b"definitely not json").unwrap();
    match Store::open(&path) {
        Err(StoreError::Corrupt(_)) => {}
        Err(other) => panic!("expected StoreError::Corrupt, got: {other}"),
        Ok(_) => panic!("expected StoreError::Corrupt, got Ok"),
    }
}

#[test]
fn future_schema_version_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("solv_state.json");
    std::fs::write(&path, br#"{"schema_version":99}"#).unwrap();
    match Store::open(&path) {
        Err(StoreError::Corrupt(msg)) => assert!(msg.contains("schema_version")),
        Err(other) => panic!("expected StoreError::Corrupt, got: {other}"),
        Ok(_) => panic!("expected StoreError::Corrupt, got Ok"),
    }
}

#[tokio::test]
async fn debounce_worker_persists_after_quiescence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("solv_state.json");
    let store = Store::open_with_debounce(&path, Duration::from_millis(100)).unwrap();
    store.spawn_snapshot_worker().await;

    store.upsert_profile("cp-1".to_string(), sample_profile("DataFeed Pro"));
    store.request_snapshot();
    // A second mutation during the window coalesces into the same snapshot.
    store.upsert_profile("cp-2".to_string(), sample_profile("Translate Bot"));
    store.request_snapshot();

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !path.exists() && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(path.exists(), "worker did not write the snapshot in time");

    let reopened = Store::open(&path).unwrap();
    assert_eq!(
        reopened.profile("cp-1"),
        Some(sample_profile("DataFeed Pro"))
    );
    assert_eq!(
        reopened.profile("cp-2"),
        Some(sample_profile("Translate Bot"))
    );
}

#[test]
fn duplicate_receipt_id_is_refused_and_original_kept() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state.json")).unwrap();
    let original = Receipt {
        decision: "BLOCK".to_string(),
        amount_usd: 50.0,
        created_at_unix: 1_762_700_000,
        ..Receipt::default()
    };
    assert!(store.insert_receipt("slv_001".to_string(), original.clone()));
    assert!(!store.insert_receipt(
        "slv_001".to_string(),
        Receipt {
            decision: "APPROVE".to_string(),
            amount_usd: 1.0,
            created_at_unix: 2_762_700_000,
            ..Receipt::default()
        }
    ));
    assert_eq!(store.receipt("slv_001"), Some(original));
}

#[test]
fn unwritable_target_reports_io_error_and_store_keeps_serving() {
    let dir = tempfile::tempdir().unwrap();
    // The destination is a directory: the .tmp sibling can be written, but
    // the rename onto a directory fails — the closest offline stand-in for
    // a disk-level write failure.
    let blocked = dir.path().join("state.json");
    let store = Store::open(&blocked).unwrap();
    std::fs::create_dir(&blocked).unwrap();

    assert!(matches!(store.write_snapshot_now(), Err(StoreError::Io(_))));
    store.upsert_profile("cp-1".to_string(), sample_profile("DataFeed Pro"));
    assert_eq!(store.profile("cp-1"), Some(sample_profile("DataFeed Pro")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_mutations_leave_parseable_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let store = Store::open_with_debounce(&path, Duration::from_millis(20)).unwrap();
    store.spawn_snapshot_worker().await;

    let mut tasks = Vec::new();
    for task_index in 0..8 {
        let store = std::sync::Arc::clone(&store);
        tasks.push(tokio::spawn(async move {
            for i in 0..25 {
                store.upsert_profile(
                    format!("cp-{task_index}-{i}"),
                    sample_profile("DataFeed Pro"),
                );
                store.request_snapshot();
            }
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
    store.write_snapshot_now().unwrap();

    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.list_profiles().len(), 8 * 25);
}

#[tokio::test]
async fn server_flushes_final_snapshot_on_graceful_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let store = Store::open(&path).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let tier1: std::sync::Arc<dyn solv::tier1::ClefClient> =
        std::sync::Arc::new(solv::tier1::MockClefClient::default());
    let tier2: std::sync::Arc<dyn solv::tier2::ServClient> =
        std::sync::Arc::new(solv::tier2::MockServClient::default());
    let server = tokio::spawn(solv::run_server(
        listener,
        solv::AppState::new(std::sync::Arc::clone(&store), tier1, tier2),
        async move {
            let _ = shutdown_rx.await;
        },
    ));

    store.upsert_profile("cp-1".to_string(), sample_profile("DataFeed Pro"));
    shutdown_tx.send(()).unwrap();
    server.await.unwrap().unwrap();

    assert!(path.exists(), "graceful shutdown must flush the snapshot");
    let reopened = Store::open(&path).unwrap();
    assert_eq!(
        reopened.profile("cp-1"),
        Some(sample_profile("DataFeed Pro"))
    );
}

#[test]
fn concurrent_snapshot_writers_never_corrupt_the_destination() {
    // Regression test for the worker-vs-shutdown-flush race: both writers
    // share one `.tmp` path, so unserialised writes mix payloads of
    // different lengths into the destination. The write mutex must make
    // every intermediate and final state parseable.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let store = Store::open(&path).unwrap();

    let short = "x".repeat(10);
    let long = "y".repeat(200);
    store.upsert_profile("cp-1".to_string(), sample_profile(&short));
    store.write_snapshot_now().unwrap();

    let mut writers = Vec::new();
    for _ in 0..4 {
        let store = std::sync::Arc::clone(&store);
        writers.push(std::thread::spawn(move || {
            for _ in 0..50 {
                store.write_snapshot_now().unwrap();
            }
        }));
    }
    // Mutate payload length while writers run.
    for round in 0..40 {
        let name = if round % 2 == 0 { &long } else { &short };
        store.upsert_profile("cp-1".to_string(), sample_profile(name));
    }
    for writer in writers {
        writer.join().unwrap();
    }

    // Every write must have left a parseable file; the final one must load.
    Store::open(&path)
        .unwrap_or_else(|error| panic!("destination corrupted: {error}"))
        .profile("cp-1")
        .expect("profile survived");
}

#[tokio::test]
async fn worker_survives_a_failing_write_and_keeps_serving() {
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("state.json");
    let store = Store::open_with_debounce(&blocked, Duration::from_millis(20)).unwrap();
    let worker = store.spawn_snapshot_worker().await;
    // Destination becomes a directory: every worker write fails with Io.
    std::fs::create_dir(&blocked).unwrap();

    store.upsert_profile("cp-1".to_string(), sample_profile("DataFeed Pro"));
    store.request_snapshot();
    tokio::time::sleep(Duration::from_millis(300)).await;

    // The worker logged the failure and kept looping.
    assert!(
        !worker.is_finished(),
        "the worker loop must survive a failing write"
    );
    assert_eq!(store.profile("cp-1"), Some(sample_profile("DataFeed Pro")));
    store.upsert_profile("cp-2".to_string(), sample_profile("Translate Bot"));
    assert_eq!(store.profile("cp-2"), Some(sample_profile("Translate Bot")));
}

// ---------------------------------------------------------------------------
// Demo seed (SPEC §5.6): opt-in, only into an empty store, validated whole.
// ---------------------------------------------------------------------------

fn seed_json(profile: serde_json::Value) -> String {
    serde_json::json!({
        "schema_version": 1,
        "counterparties": { "seeded-api": profile }
    })
    .to_string()
}

fn good_seed_profile() -> serde_json::Value {
    serde_json::json!({
        "counterparty_name": "seeded-api",
        "target_url": "https://seeded.example.com",
        "score": {
            "reppo_score": 90.0,
            "eval_id": "ev_seed_1",
            "settled_at_unix": 1_794_754_061u64,
            "pillars": {
                "reliability": 5, "track_record": 9, "safety": 5, "maturity": 5
            }
        }
    })
}

fn write_seed(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
    let path = dir.join("seed.json");
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn a_seed_loads_into_an_empty_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("solv_state.json")).unwrap();
    let seed = write_seed(dir.path(), &seed_json(good_seed_profile()));

    assert_eq!(store.seed_from_file(&seed).unwrap(), SeedOutcome::Loaded(1));
    let profile = store.profile("seeded-api").expect("seeded profile");
    let score = profile.score.expect("score kept");
    assert_eq!(score.eval_id, "ev_seed_1");
    assert_eq!(score.reppo_score, 90.0);
}

#[test]
fn a_seed_never_overwrites_existing_state() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("solv_state.json")).unwrap();
    store.upsert_profile("cp-1".to_string(), sample_profile("cp-1"));
    let seed = write_seed(dir.path(), &seed_json(good_seed_profile()));

    assert_eq!(store.seed_from_file(&seed).unwrap(), SeedOutcome::Skipped);
    assert!(
        store.profile("seeded-api").is_none(),
        "seed was not applied"
    );
    assert_eq!(store.list_profiles().len(), 1);
    assert_eq!(store.profile("cp-1").unwrap(), sample_profile("cp-1"));
}

#[test]
fn a_skipped_seed_is_not_even_read() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("solv_state.json")).unwrap();
    store.upsert_profile("cp-1".to_string(), sample_profile("cp-1"));
    let missing = dir.path().join("does-not-exist.json");
    assert_eq!(
        store.seed_from_file(&missing).unwrap(),
        SeedOutcome::Skipped
    );
}

#[test]
fn a_bad_seed_fails_loudly_and_leaves_the_store_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let mut no_eval = good_seed_profile();
    no_eval["score"]["eval_id"] = serde_json::json!("  ");
    let mut out_of_range = good_seed_profile();
    out_of_range["score"]["reppo_score"] = serde_json::json!(101.0);
    let mut name_mismatch = good_seed_profile();
    name_mismatch["counterparty_name"] = serde_json::json!("someone-else");
    let mut no_url = good_seed_profile();
    no_url["target_url"] = serde_json::json!("");

    let bad_bodies = [
        "not json at all".to_string(),
        String::new(),
        r#"{"schema_version":2,"counterparties":{"a":{"counterparty_name":"a","target_url":"https://a.example"}}}"#
            .to_string(),
        r#"{"schema_version":1,"counterparties":{}}"#.to_string(),
        r#"{"schema_version":1,"counterparties":{},"receipts":{}}"#.to_string(),
        seed_json(no_eval),
        seed_json(out_of_range),
        seed_json(name_mismatch),
        seed_json(no_url),
    ];
    for body in &bad_bodies {
        let store = Store::open(dir.path().join("solv_state.json")).unwrap();
        let seed = write_seed(dir.path(), body);
        match store.seed_from_file(&seed) {
            Err(StoreError::Seed(_)) => {}
            other => panic!("expected StoreError::Seed for {body:?}, got {other:?}"),
        }
        assert!(store.list_profiles().is_empty(), "nothing partially loaded");
        // The store is still usable: no poisoned lock, no panic.
        store.upsert_profile("cp-1".to_string(), sample_profile("cp-1"));
        assert_eq!(store.list_profiles().len(), 1);
    }

    let store = Store::open(dir.path().join("solv_state.json")).unwrap();
    match store.seed_from_file(&dir.path().join("missing.json")) {
        Err(StoreError::Seed(message)) => assert!(message.contains("cannot read")),
        other => panic!("expected StoreError::Seed, got {other:?}"),
    }
}

#[test]
fn the_committed_demo_seed_is_one_real_evaluation_that_ages_and_expires() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("solv_state.json")).unwrap();
    let seed = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("seed/demo_profiles.json");

    assert_eq!(store.seed_from_file(&seed).unwrap(), SeedOutcome::Loaded(1));
    let profile = store.profile("github-rest-api").expect("demo profile");
    assert_eq!(profile.target_url, "https://api.github.com");
    let score = profile.score.expect("demo score");
    assert_eq!(score.eval_id, "cmwhy4whj000707jptcchz6fz");
    assert_eq!(score.reppo_score, 100.0);

    // Same pipeline as any score: aged from settled_at at read time.
    let fresh = effective_score(&score, score.settled_at_unix).unwrap();
    assert_eq!((fresh.fico_raw, fresh.fico_effective), (850, 850));
    let day = 86_400;
    let aged = effective_score(&score, score.settled_at_unix + 60 * day).unwrap();
    assert!(aged.fico_effective < 850, "decays after 30 days");
    assert!(
        effective_score(&score, score.settled_at_unix + 91 * day).is_none(),
        "discarded after 90 days: the seed stops rating around 2027-02-13"
    );
}
