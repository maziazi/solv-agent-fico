//! Integration tests for the state layer (docs/SPEC.md §5.6, milestone M1):
//! state survives restart, an interrupted write leaves no corrupt file, and
//! the debounced snapshot worker persists after quiescence.

use std::time::Duration;

use solv::state::{LearnedRule, Profile, Receipt, RuleStatus, Store, StoreError};

fn sample_profile(name: &str) -> Profile {
    Profile {
        counterparty_name: name.to_string(),
        target_url: format!("https://{name}.example.com"),
    }
}

#[test]
fn snapshot_roundtrip_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("solv_state.json");

    let store = Store::open(&path).unwrap();
    store.upsert_profile("cp-1".to_string(), sample_profile("DataFeed Pro"));
    store.upsert_profile("cp-2".to_string(), sample_profile("Translate Bot"));
    store.insert_receipt(
        "slv_001".to_string(),
        Receipt {
            decision: "BLOCK".to_string(),
            amount_usd: 50.0,
            created_at_unix: 1_762_700_000,
        },
    );
    store.set_learned_rules(vec![LearnedRule {
        rule_id: "SOLV-RULE-001".to_string(),
        status: RuleStatus::Shadow,
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
    };
    assert!(store.insert_receipt("slv_001".to_string(), original.clone()));
    assert!(!store.insert_receipt(
        "slv_001".to_string(),
        Receipt {
            decision: "APPROVE".to_string(),
            amount_usd: 1.0,
            created_at_unix: 2_762_700_000,
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
    let server = tokio::spawn(solv::run_server(
        listener,
        solv::AppState::new(std::sync::Arc::clone(&store)),
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
