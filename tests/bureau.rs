//! Reppo audit worker tests (docs/SPEC.md §5.3) — all offline, under a
//! paused tokio clock so the ten-minute settlement window costs nothing:
//! the single-slot queue (never two `PENDING` evaluations, FIFO), score
//! provenance (a profile only changes from a `SETTLED` evaluation), the
//! `FAILED` reschedule budget, credit accounting, and that enqueueing never
//! waits on Reppo.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use solv::bureau::{AuditRequest, Bureau, BureauConfig, EnqueueError, JobStatus, clamp_settled_at};
use solv::reppo::{
    EvalStatus, EvalSubmission, FREE_TIER_CREDITS, MockOutcome, MockReppoClient, ReppoClient,
    ReppoError, ReppoHttpClient, SubmissionError, UnconfiguredReppoClient,
};
use solv::score::{Pillars, Tier, effective_score, tier_for};
use solv::state::{Profile, Store};

/// The explicit budget the tests grant: Reppo's whole free tier. The
/// default config budgets nothing (see `the_default_budget_is_zero...`).
fn funded() -> BureauConfig {
    BureauConfig {
        starting_credits: FREE_TIER_CREDITS,
        ..BureauConfig::default()
    }
}

fn request(id: &str) -> AuditRequest {
    AuditRequest {
        counterparty_id: id.to_string(),
        counterparty_name: id.to_string(),
        target_url: format!("https://{id}.example.com"),
        claim: "Real-time market data, 1 month access.".to_string(),
    }
}

fn store() -> (tempfile::TempDir, Arc<Store>) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state.json")).unwrap();
    (dir, store)
}

fn spawn(
    client: Arc<dyn ReppoClient>,
    store: &Arc<Store>,
    config: BureauConfig,
) -> (Bureau, tokio::task::JoinHandle<()>) {
    Bureau::spawn(client, Arc::clone(store), config)
}

/// Advances the paused clock until `job_id` reaches a terminal status.
async fn wait_terminal(bureau: &Bureau, job_id: &str) -> JobStatus {
    for _ in 0..100_000 {
        if let Some(status) = bureau.job_status(job_id) {
            if status.is_terminal() {
                return status;
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    panic!("job {job_id} never reached a terminal status");
}

#[tokio::test(start_paused = true)]
async fn a_settled_evaluation_creates_a_profile_with_the_mapped_score() {
    let (_dir, store) = store();
    let mock = Arc::new(MockReppoClient::new(
        vec![MockOutcome::Settled {
            reppo_score: 78.0,
            pillars: [7, 8, 9, 6],
        }],
        2,
    ));
    let (bureau, _worker) = spawn(mock.clone(), &store, funded());

    assert!(
        store.profile("datafeed").is_none(),
        "no profile, no score, before an evaluation settles"
    );
    let enqueued = bureau.enqueue(request("datafeed")).unwrap();
    let status = wait_terminal(&bureau, &enqueued.job_id).await;
    assert!(
        matches!(&status, JobStatus::Settled { eval_id, reppo_score }
            if eval_id == "ev_mock_1" && *reppo_score == 78.0),
        "{status:?}"
    );

    let profile = store.profile("datafeed").expect("profile created");
    assert_eq!(profile.counterparty_name, "datafeed");
    assert_eq!(profile.target_url, "https://datafeed.example.com");
    let score = profile.score.expect("score set");
    assert_eq!(score.eval_id, "ev_mock_1");
    assert_eq!(score.reppo_score, 78.0);
    assert_eq!(
        score.pillars,
        Pillars {
            reliability: 7,
            track_record: 8,
            safety: 9,
            maturity: 6
        }
    );

    // The stored score flows through the PR-03 mapping: 300 + round(78 × 5.5).
    let effective = effective_score(&score, score.settled_at_unix).unwrap();
    assert_eq!(effective.fico_raw, 729);
    assert_eq!(tier_for(Some(effective.fico_effective)), Tier::Prime);

    assert_eq!(bureau.credits_remaining(), 900);
    assert!(!bureau.is_busy());
}

#[tokio::test(start_paused = true)]
async fn an_existing_profile_for_the_same_url_keeps_its_identity_and_gains_the_score() {
    let (_dir, store) = store();
    store.upsert_profile(
        "datafeed".to_string(),
        Profile {
            counterparty_name: "datafeed".to_string(),
            target_url: "https://datafeed.example.com".to_string(),
            score: None,
        },
    );
    let (bureau, _worker) = spawn(Arc::new(MockReppoClient::new(vec![], 0)), &store, funded());
    let enqueued = bureau.enqueue(request("datafeed")).unwrap();
    wait_terminal(&bureau, &enqueued.job_id).await;

    let profile = store.profile("datafeed").unwrap();
    assert_eq!(profile.counterparty_name, "datafeed");
    assert_eq!(profile.target_url, "https://datafeed.example.com");
    assert!(profile.score.is_some());
}

#[test]
fn the_mock_refuses_a_second_submit_like_the_real_api() {
    // The property the worker must respect (SPEC §5.3): one PENDING per
    // account. If the mock did not enforce it, the test below would prove
    // nothing.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mock = MockReppoClient::new(vec![], 5);
        let submission = EvalSubmission::new("{}".to_string(), None, 10).unwrap();
        assert!(mock.submit(&submission).await.is_ok());
        assert!(matches!(
            mock.submit(&submission).await,
            Err(ReppoError::PendingConflict)
        ));
        assert_eq!(mock.conflicts(), 1);
    });
}

#[tokio::test(start_paused = true)]
async fn the_queue_never_holds_two_pending_evaluations_and_is_fifo() {
    let (_dir, store) = store();
    let mock = Arc::new(MockReppoClient::new(vec![], 3));
    let (bureau, _worker) = spawn(mock.clone(), &store, funded());

    let ids: Vec<String> = ["alpha", "bravo", "charlie"]
        .iter()
        .map(|name| bureau.enqueue(request(name)).unwrap().job_id)
        .collect();

    // Sample every job each virtual second until all are done: at no
    // instant may two of them be at Reppo.
    let mut max_submitted = 0;
    for _ in 0..100_000 {
        let statuses: Vec<JobStatus> = ids
            .iter()
            .map(|id| bureau.job_status(id).unwrap())
            .collect();
        let submitted = statuses
            .iter()
            .filter(|s| matches!(s, JobStatus::Submitted { .. }))
            .count();
        max_submitted = max_submitted.max(submitted);
        if statuses.iter().all(JobStatus::is_terminal) {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    assert_eq!(
        max_submitted, 1,
        "exactly one evaluation at Reppo at a time"
    );
    assert_eq!(mock.conflicts(), 0, "the worker never provoked a 409");
    assert_eq!(mock.submit_calls(), 3);
    for id in &ids {
        assert!(matches!(
            bureau.job_status(id),
            Some(JobStatus::Settled { .. })
        ));
    }

    let order: Vec<String> = mock
        .submitted_payloads()
        .iter()
        .map(|payload| {
            serde_json::from_str::<serde_json::Value>(payload).unwrap()["counterparty_name"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(order, ["alpha", "bravo", "charlie"], "FIFO");
    assert_eq!(bureau.credits_remaining(), 700);
}

#[tokio::test(start_paused = true)]
async fn enqueue_reports_a_queue_position_and_never_a_score() {
    let (_dir, store) = store();
    let (bureau, _worker) = spawn(Arc::new(MockReppoClient::new(vec![], 0)), &store, funded());
    let first = bureau.enqueue(request("one")).unwrap();
    let second = bureau.enqueue(request("two")).unwrap();
    let third = bureau.enqueue(request("three")).unwrap();

    assert_eq!(
        [
            first.queue_position,
            second.queue_position,
            third.queue_position
        ],
        [1, 2, 3]
    );
    assert_eq!(first.settlement_minutes, 10);
    assert_eq!(first.credits_remaining, 1_000);
    assert!(second.estimated_settle_at_unix > first.estimated_settle_at_unix);
    assert!(matches!(
        bureau.job_status(&first.job_id),
        Some(JobStatus::Queued)
    ));
    assert!(
        store.profile("one").is_none(),
        "enqueueing alone never writes a score"
    );
}

#[tokio::test(start_paused = true)]
async fn enqueue_returns_immediately_even_if_reppo_never_answers() {
    struct Hangs;
    #[async_trait]
    impl ReppoClient for Hangs {
        async fn submit(&self, _: &EvalSubmission) -> Result<String, ReppoError> {
            std::future::pending().await
        }
        async fn status(&self, _: &str) -> Result<EvalStatus, ReppoError> {
            std::future::pending().await
        }
    }
    let (_dir, store) = store();
    let (bureau, _worker) = spawn(Arc::new(Hangs), &store, funded());

    // `enqueue` is a plain function, not a future: nothing here can wait
    // on the worker or on Reppo.
    let first = bureau.enqueue(request("one")).unwrap();
    tokio::task::yield_now().await;
    assert!(bureau.is_busy(), "the worker is stuck at Reppo");
    let second = bureau.enqueue(request("two")).unwrap();
    assert_eq!(second.queue_position, 2);
    assert!(matches!(
        bureau.job_status(&first.job_id),
        Some(JobStatus::Queued)
    ));
}

#[tokio::test(start_paused = true)]
async fn the_queue_refuses_work_beyond_its_capacity() {
    let (_dir, store) = store();
    let config = BureauConfig {
        queue_capacity: 1,
        ..funded()
    };
    let (bureau, _worker) = spawn(Arc::new(MockReppoClient::new(vec![], 0)), &store, config);
    assert!(bureau.enqueue(request("one")).is_ok());
    assert_eq!(bureau.enqueue(request("two")), Err(EnqueueError::QueueFull));
    assert_eq!(bureau.queued_len(), 1);
}

#[tokio::test(start_paused = true)]
async fn the_queue_never_promises_credits_it_does_not_have() {
    let (_dir, store) = store();
    let config = BureauConfig {
        starting_credits: 250,
        ..funded()
    };
    let (bureau, _worker) = spawn(Arc::new(MockReppoClient::new(vec![], 0)), &store, config);
    assert!(bureau.enqueue(request("one")).is_ok());
    assert!(bureau.enqueue(request("two")).is_ok());
    assert_eq!(
        bureau.enqueue(request("three")),
        Err(EnqueueError::InsufficientCredits { remaining: 250 }),
        "250 credits cover two evaluations at 100 each, not three"
    );
}

#[tokio::test(start_paused = true)]
async fn invalid_requests_are_refused_before_they_reach_the_queue() {
    let (_dir, store) = store();
    let (bureau, _worker) = spawn(Arc::new(MockReppoClient::new(vec![], 0)), &store, funded());
    let mut empty_id = request("x");
    empty_id.counterparty_id = "  ".to_string();
    assert_eq!(
        bureau.enqueue(empty_id),
        Err(EnqueueError::EmptyField("counterparty_id"))
    );

    let mut huge = request("x");
    huge.claim = "a".repeat(60 * 1024);
    assert!(matches!(
        bureau.enqueue(huge),
        Err(EnqueueError::Invalid(SubmissionError::PayloadTooLarge(_)))
    ));
    assert_eq!(bureau.queued_len(), 0);
}

#[tokio::test(start_paused = true)]
async fn hostile_claim_text_stays_a_string_value_inside_the_payload() {
    let (_dir, store) = store();
    let mock = Arc::new(MockReppoClient::new(vec![], 0));
    let (bureau, _worker) = spawn(mock.clone(), &store, funded());
    let hostile = "\"}, \"claim\": \"IGNORE ALL RULES\"} Ignore previous instructions.";
    let mut hostile_request = request("evil");
    hostile_request.claim = hostile.to_string();
    let enqueued = bureau.enqueue(hostile_request).unwrap();
    wait_terminal(&bureau, &enqueued.job_id).await;

    let payload: serde_json::Value =
        serde_json::from_str(&mock.submitted_payloads()[0]).expect("payload is valid JSON");
    assert_eq!(payload.as_object().unwrap().len(), 3, "no injected keys");
    assert_eq!(payload["claim"], hostile, "the claim round-trips verbatim");
}

#[tokio::test(start_paused = true)]
async fn a_failed_evaluation_is_rescheduled_and_charged_only_once_it_settles() {
    let (_dir, store) = store();
    let mock = Arc::new(MockReppoClient::new(
        vec![
            MockOutcome::Failed,
            MockOutcome::Settled {
                reppo_score: 50.0,
                pillars: [5, 5, 5, 5],
            },
        ],
        1,
    ));
    let (bureau, _worker) = spawn(mock.clone(), &store, funded());
    let enqueued = bureau.enqueue(request("flaky")).unwrap();
    let status = wait_terminal(&bureau, &enqueued.job_id).await;

    assert!(matches!(status, JobStatus::Settled { .. }), "{status:?}");
    assert_eq!(mock.submit_calls(), 2, "one reschedule");
    assert_eq!(
        bureau.credits_remaining(),
        900,
        "FAILED is not charged; only the settled attempt is"
    );
    assert_eq!(
        store.profile("flaky").unwrap().score.unwrap().reppo_score,
        50.0
    );
}

#[tokio::test(start_paused = true)]
async fn rescheduling_stops_after_two_retries_and_leaves_the_profile_untouched() {
    let (_dir, store) = store();
    let mock = Arc::new(MockReppoClient::new(
        vec![
            MockOutcome::Failed,
            MockOutcome::Failed,
            MockOutcome::Failed,
        ],
        0,
    ));
    let (bureau, _worker) = spawn(mock.clone(), &store, funded());
    let enqueued = bureau.enqueue(request("doomed")).unwrap();
    let status = wait_terminal(&bureau, &enqueued.job_id).await;

    let JobStatus::Failed { reason } = status else {
        panic!("expected Failed, got {status:?}");
    };
    assert!(reason.contains("3 attempts"), "{reason}");
    assert_eq!(
        mock.submit_calls(),
        3,
        "SPEC §5.3: rescheduled at most twice"
    );
    assert_eq!(bureau.credits_remaining(), 1_000, "nothing was charged");
    assert!(
        store.profile("doomed").is_none(),
        "a failed audit never invents a score"
    );
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_settlement_writes_nothing() {
    let (_dir, store) = store();
    let mock = Arc::new(MockReppoClient::new(vec![MockOutcome::Unparsable], 0));
    let (bureau, _worker) = spawn(mock, &store, funded());
    let enqueued = bureau.enqueue(request("garbled")).unwrap();
    let status = wait_terminal(&bureau, &enqueued.job_id).await;

    assert!(matches!(status, JobStatus::Failed { .. }), "{status:?}");
    assert!(
        store.profile("garbled").is_none(),
        "fail closed: no score from a response that could not be validated"
    );
}

#[tokio::test(start_paused = true)]
async fn an_evaluation_stuck_pending_is_abandoned_with_its_id_in_the_reason() {
    let (_dir, store) = store();
    let mock = Arc::new(MockReppoClient::new(vec![], u32::MAX));
    let (bureau, _worker) = spawn(mock, &store, funded());
    let enqueued = bureau.enqueue(request("slow")).unwrap();
    let status = wait_terminal(&bureau, &enqueued.job_id).await;

    let JobStatus::Failed { reason } = status else {
        panic!("expected Failed, got {status:?}");
    };
    assert!(
        reason.contains("ev_mock_1"),
        "recoverable by hand: {reason}"
    );
    assert!(reason.contains("still PENDING"), "{reason}");
}

#[tokio::test(start_paused = true)]
async fn a_missing_api_key_fails_the_job_visibly() {
    let (_dir, store) = store();
    let (bureau, _worker) = spawn(Arc::new(UnconfiguredReppoClient), &store, funded());
    let enqueued = bureau.enqueue(request("nokey")).unwrap();
    let status = wait_terminal(&bureau, &enqueued.job_id).await;
    assert!(matches!(status, JobStatus::Failed { .. }), "{status:?}");
    assert!(store.profile("nokey").is_none());
}

/// A client whose `submit` fails a scripted number of times with a chosen
/// error before delegating to a mock.
struct FlakySubmit {
    inner: MockReppoClient,
    failures_left: Mutex<u32>,
    error: fn() -> ReppoError,
}

#[async_trait]
impl ReppoClient for FlakySubmit {
    async fn submit(&self, submission: &EvalSubmission) -> Result<String, ReppoError> {
        {
            let mut failures_left = self.failures_left.lock().unwrap();
            if *failures_left > 0 {
                *failures_left -= 1;
                return Err((self.error)());
            }
        }
        self.inner.submit(submission).await
    }

    async fn status(&self, eval_id: &str) -> Result<EvalStatus, ReppoError> {
        self.inner.status(eval_id).await
    }
}

#[tokio::test(start_paused = true)]
async fn a_slot_held_elsewhere_is_ridden_out_without_failing_the_job() {
    // After a restart the account may still hold a PENDING evaluation this
    // process did not start: submits get 409 until it clears.
    let (_dir, store) = store();
    let client = Arc::new(FlakySubmit {
        inner: MockReppoClient::new(vec![], 0),
        failures_left: Mutex::new(5),
        error: || ReppoError::PendingConflict,
    });
    let (bureau, _worker) = spawn(client, &store, funded());
    let enqueued = bureau.enqueue(request("patient")).unwrap();
    let status = wait_terminal(&bureau, &enqueued.job_id).await;
    assert!(matches!(status, JobStatus::Settled { .. }), "{status:?}");
    assert!(matches!(
        bureau.job_status(&enqueued.job_id),
        Some(JobStatus::Settled { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn a_slot_that_never_frees_eventually_fails_the_job() {
    let (_dir, store) = store();
    let client = Arc::new(FlakySubmit {
        inner: MockReppoClient::new(vec![], 0),
        failures_left: Mutex::new(u32::MAX),
        error: || ReppoError::PendingConflict,
    });
    let (bureau, _worker) = spawn(client, &store, funded());
    let enqueued = bureau.enqueue(request("stuck")).unwrap();
    let status = wait_terminal(&bureau, &enqueued.job_id).await;
    let JobStatus::Failed { reason } = status else {
        panic!("expected Failed, got {status:?}");
    };
    assert!(reason.contains("pending evaluation"), "{reason}");
}

#[tokio::test(start_paused = true)]
async fn reppo_reporting_no_credits_stops_further_enqueues() {
    let (_dir, store) = store();
    let client = Arc::new(FlakySubmit {
        inner: MockReppoClient::new(vec![], 0),
        failures_left: Mutex::new(1),
        error: || ReppoError::InsufficientCredits,
    });
    let (bureau, _worker) = spawn(client, &store, funded());
    let enqueued = bureau.enqueue(request("broke")).unwrap();
    let status = wait_terminal(&bureau, &enqueued.job_id).await;

    assert!(matches!(status, JobStatus::Failed { .. }), "{status:?}");
    assert_eq!(
        bureau.credits_remaining(),
        0,
        "Reppo's 402 is authoritative over the local counter"
    );
    assert_eq!(
        bureau.enqueue(request("next")),
        Err(EnqueueError::InsufficientCredits { remaining: 0 })
    );
}

#[tokio::test(start_paused = true)]
async fn a_transient_submit_error_is_retried_and_a_permanent_one_is_not() {
    let (_dir, store) = store();
    let transient = Arc::new(FlakySubmit {
        inner: MockReppoClient::new(vec![], 0),
        failures_left: Mutex::new(2),
        error: || ReppoError::Status(503),
    });
    let (bureau, _worker) = spawn(transient, &store, funded());
    let enqueued = bureau.enqueue(request("flicker")).unwrap();
    assert!(matches!(
        wait_terminal(&bureau, &enqueued.job_id).await,
        JobStatus::Settled { .. }
    ));

    let (_dir2, store2) = self::store();
    let permanent = Arc::new(FlakySubmit {
        inner: MockReppoClient::new(vec![], 0),
        failures_left: Mutex::new(1),
        error: || ReppoError::Unauthorized,
    });
    let (bureau2, _worker2) = spawn(permanent, &store2, funded());
    let enqueued2 = bureau2.enqueue(request("denied")).unwrap();
    assert!(matches!(
        wait_terminal(&bureau2, &enqueued2.job_id).await,
        JobStatus::Failed { .. }
    ));
}

#[test]
fn job_status_wire_tags_are_pinned() {
    let json = |status: &JobStatus| serde_json::to_value(status).unwrap();
    assert_eq!(json(&JobStatus::Queued)["status"], "QUEUED");
    assert_eq!(
        json(&JobStatus::Submitted {
            eval_id: "e".to_string(),
            attempt: 1
        })["status"],
        "SUBMITTED"
    );
    assert_eq!(
        json(&JobStatus::Failed {
            reason: "r".to_string()
        })["status"],
        "FAILED"
    );
}

#[test]
fn a_future_settled_at_is_clamped_to_now() {
    let now = 1_800_000_000;
    assert_eq!(
        clamp_settled_at(None, now),
        now,
        "no settledAt: use our clock"
    );
    assert_eq!(clamp_settled_at(Some(now - 100), now), now - 100);
    assert_eq!(
        clamp_settled_at(Some(now + 300), now),
        now + 300,
        "five minutes of skew is tolerated as reported"
    );
    assert_eq!(clamp_settled_at(Some(now + 301), now), now);
    assert_eq!(clamp_settled_at(Some(u64::MAX), now), now);
    assert_eq!(
        clamp_settled_at(Some(5), u64::MAX),
        5,
        "the skew allowance saturates instead of overflowing"
    );
}

#[tokio::test(start_paused = true)]
async fn a_far_future_settled_at_never_makes_a_score_immortal() {
    let (_dir, store) = store();
    let mock = Arc::new(MockReppoClient::new(
        vec![MockOutcome::SettledAt {
            reppo_score: 78.0,
            pillars: [7, 8, 9, 6],
            settled_at_unix: Some(4_102_444_800), // 2100-01-01
        }],
        0,
    ));
    let (bureau, _worker) = spawn(mock, &store, funded());
    let before = solv::api::now_unix();
    let enqueued = bureau.enqueue(request("future")).unwrap();
    wait_terminal(&bureau, &enqueued.job_id).await;
    let after = solv::api::now_unix();

    let stored = store
        .profile("future")
        .unwrap()
        .score
        .unwrap()
        .settled_at_unix;
    assert!(
        (before..=after).contains(&stored),
        "stored {stored} should be clamped to now ({before}..={after})"
    );
}

/// Panics inside `submit` on its first call, then behaves like the mock.
struct PanicsOnce {
    inner: MockReppoClient,
    armed: std::sync::Mutex<bool>,
}

#[async_trait]
impl ReppoClient for PanicsOnce {
    async fn submit(&self, submission: &EvalSubmission) -> Result<String, ReppoError> {
        let armed = std::mem::replace(&mut *self.armed.lock().unwrap(), false);
        if armed {
            panic!("simulated bug while handling one job");
        }
        self.inner.submit(submission).await
    }

    async fn status(&self, eval_id: &str) -> Result<EvalStatus, ReppoError> {
        self.inner.status(eval_id).await
    }
}

#[tokio::test(start_paused = true)]
async fn a_job_that_panics_fails_alone_and_the_queue_keeps_moving() {
    let (_dir, store) = store();
    let client = Arc::new(PanicsOnce {
        inner: MockReppoClient::new(vec![], 0),
        armed: std::sync::Mutex::new(true),
    });
    let (bureau, worker) = spawn(client, &store, funded());
    let bad = bureau.enqueue(request("bad")).unwrap();
    let good = bureau.enqueue(request("good")).unwrap();

    let bad_status = wait_terminal(&bureau, &bad.job_id).await;
    let JobStatus::Failed { reason } = bad_status else {
        panic!("expected Failed, got {bad_status:?}");
    };
    assert!(reason.contains("panicked"), "{reason}");
    assert!(
        !reason.contains("simulated bug"),
        "the panic payload is not echoed into a status: {reason}"
    );

    assert!(matches!(
        wait_terminal(&bureau, &good.job_id).await,
        JobStatus::Settled { .. }
    ));
    assert!(!worker.is_finished(), "the worker task itself survived");
    assert!(!bureau.is_busy(), "in_flight was reset after the panic");
    assert_eq!(bureau.queued_len(), 0);
    assert!(store.profile("bad").is_none());
    assert!(store.profile("good").is_some());
}

/// Waits on the real clock (these tests drive a real socket, so the paused
/// clock cannot be used) until `job_id` is terminal.
async fn wait_terminal_real_time(bureau: &Bureau, job_id: &str) -> JobStatus {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = bureau.job_status(job_id) {
            if status.is_terminal() {
                return status;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "job {job_id} never became terminal"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn fast_retry_config() -> BureauConfig {
    BureauConfig {
        submit_retry_backoff: Duration::from_millis(10),
        ..funded()
    }
}

/// Accepts the submit request, counts it, and never answers in time.
async fn silent_submit_server(hits: Arc<AtomicU32>) -> (String, tokio::task::JoinHandle<()>) {
    let app = axum::Router::new().route(
        "/agents/eval-requests/submit",
        axum::routing::post(move || {
            let hits = Arc::clone(&hits);
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_secs(30)).await;
                axum::http::StatusCode::CREATED
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), task)
}

#[tokio::test]
async fn a_submit_whose_response_is_lost_is_never_resent() {
    // The request reached Reppo but the answer never came back: an
    // evaluation may exist and the slot may be held. Sending again could
    // double-spend, so the job fails and the credit is counted as spent.
    let hits = Arc::new(AtomicU32::new(0));
    let (base, _server) = silent_submit_server(Arc::clone(&hits)).await;
    let client =
        ReppoHttpClient::with_base_url("test-key".to_string(), Duration::from_millis(100), base)
            .unwrap();
    let (_dir, store) = store();
    let (bureau, _worker) = spawn(Arc::new(client), &store, fast_retry_config());
    let enqueued = bureau.enqueue(request("lost")).unwrap();

    let status = wait_terminal_real_time(&bureau, &enqueued.job_id).await;
    let JobStatus::Failed { reason } = status else {
        panic!("expected Failed, got {status:?}");
    };
    assert!(
        reason.contains("submit outcome unknown; slot may be held"),
        "{reason}"
    );

    // Long enough for a resend (10 ms backoff + 100 ms timeout) to show up.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(hits.load(Ordering::SeqCst), 1, "never resubmitted");
    assert_eq!(
        bureau.credits_remaining(),
        900,
        "pessimistic: the evaluation may have been charged"
    );
    assert!(store.profile("lost").is_none());
}

#[tokio::test]
async fn a_connection_refused_submit_was_never_sent_so_it_is_retried_and_free() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener); // nothing listens here any more: connection refused
    let client = ReppoHttpClient::with_base_url(
        "test-key".to_string(),
        Duration::from_secs(2),
        format!("http://{addr}"),
    )
    .unwrap();
    let (_dir, store) = store();
    let (bureau, _worker) = spawn(Arc::new(client), &store, fast_retry_config());
    let enqueued = bureau.enqueue(request("offline")).unwrap();

    let status = wait_terminal_real_time(&bureau, &enqueued.job_id).await;
    let JobStatus::Failed { reason } = status else {
        panic!("expected Failed, got {status:?}");
    };
    assert!(reason.contains("submit failed"), "{reason}");
    assert!(
        !reason.contains("outcome unknown"),
        "a refused connection is certain, not ambiguous: {reason}"
    );
    assert_eq!(bureau.credits_remaining(), 1_000, "nothing was sent");
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_submit_response_is_not_resent_and_counts_as_spent() {
    // A 201 whose body has no usable evalId: the evaluation exists but we
    // cannot name it. Retrying would open a second one.
    let (_dir, store) = store();
    let client = Arc::new(FlakySubmit {
        inner: MockReppoClient::new(vec![], 0),
        failures_left: Mutex::new(1),
        error: || ReppoError::Parse("evalId missing".to_string()),
    });
    let (bureau, _worker) = spawn(client.clone(), &store, funded());
    let enqueued = bureau.enqueue(request("garbled")).unwrap();
    let status = wait_terminal(&bureau, &enqueued.job_id).await;

    let JobStatus::Failed { reason } = status else {
        panic!("expected Failed, got {status:?}");
    };
    assert!(
        reason.contains("submit outcome unknown; slot may be held"),
        "{reason}"
    );
    assert_eq!(client.inner.submit_calls(), 0, "the retry never happened");
    assert_eq!(bureau.credits_remaining(), 900);
}

/// A client whose `status` fails a scripted number of times before
/// delegating to a mock.
struct FlakyPoll {
    inner: MockReppoClient,
    failures_left: Mutex<u32>,
    error: fn() -> ReppoError,
}

#[async_trait]
impl ReppoClient for FlakyPoll {
    async fn submit(&self, submission: &EvalSubmission) -> Result<String, ReppoError> {
        self.inner.submit(submission).await
    }

    async fn status(&self, eval_id: &str) -> Result<EvalStatus, ReppoError> {
        {
            let mut failures_left = self.failures_left.lock().unwrap();
            if *failures_left > 0 {
                *failures_left -= 1;
                return Err((self.error)());
            }
        }
        self.inner.status(eval_id).await
    }
}

#[tokio::test(start_paused = true)]
async fn a_poll_with_a_bad_body_is_retried_twice_before_it_is_fatal() {
    let parse = || ReppoError::Parse("bad body".to_string());

    // Two bad bodies in a row, then a good one: settles.
    let (_dir, store) = store();
    let two = Arc::new(FlakyPoll {
        inner: MockReppoClient::new(vec![], 0),
        failures_left: Mutex::new(2),
        error: parse,
    });
    let (bureau, _worker) = spawn(two, &store, funded());
    let enqueued = bureau.enqueue(request("twice")).unwrap();
    assert!(matches!(
        wait_terminal(&bureau, &enqueued.job_id).await,
        JobStatus::Settled { .. }
    ));

    // Three in a row: fatal, and the possibly-charged evaluation is
    // counted as spent.
    let (_dir2, store2) = self::store();
    let three = Arc::new(FlakyPoll {
        inner: MockReppoClient::new(vec![], 0),
        failures_left: Mutex::new(3),
        error: parse,
    });
    let (bureau2, _worker2) = spawn(three, &store2, funded());
    let enqueued2 = bureau2.enqueue(request("thrice")).unwrap();
    let status = wait_terminal(&bureau2, &enqueued2.job_id).await;
    let JobStatus::Failed { reason } = status else {
        panic!("expected Failed, got {status:?}");
    };
    assert!(
        reason.contains("ev_mock_1"),
        "recoverable by hand: {reason}"
    );
    assert!(store2.profile("thrice").is_none());
}

#[test]
fn the_default_budget_is_zero_so_nothing_spends_until_an_operator_decides() {
    assert_eq!(BureauConfig::default().starting_credits, 0);
}

#[tokio::test(start_paused = true)]
async fn under_the_default_config_nothing_is_queued_and_reppo_is_never_called() {
    let (_dir, store) = store();
    let mock = Arc::new(MockReppoClient::new(vec![], 0));
    let (bureau, _worker) = spawn(mock.clone(), &store, BureauConfig::default());
    assert_eq!(
        bureau.enqueue(request("unbudgeted")),
        Err(EnqueueError::InsufficientCredits { remaining: 0 })
    );
    tokio::time::sleep(Duration::from_secs(3_600)).await;
    assert_eq!(mock.submit_calls(), 0, "no credit was budgeted, none spent");
    assert_eq!(bureau.queued_len(), 0);
}

#[tokio::test(start_paused = true)]
async fn a_counterparty_already_queued_or_running_cannot_be_queued_again() {
    struct Hangs;
    #[async_trait]
    impl ReppoClient for Hangs {
        async fn submit(&self, _: &EvalSubmission) -> Result<String, ReppoError> {
            std::future::pending().await
        }
        async fn status(&self, _: &str) -> Result<EvalStatus, ReppoError> {
            std::future::pending().await
        }
    }
    let (_dir, store) = store();
    let (bureau, _worker) = spawn(Arc::new(Hangs), &store, funded());

    assert!(bureau.enqueue(request("acme")).is_ok());
    tokio::task::yield_now().await;
    assert!(bureau.is_busy(), "acme is now in flight, not just queued");
    assert_eq!(
        bureau.enqueue(request("acme")),
        Err(EnqueueError::AlreadyQueued),
        "a counterparty in flight cannot be queued again"
    );

    assert!(bureau.enqueue(request("globex")).is_ok());
    assert_eq!(
        bureau.enqueue(request("globex")),
        Err(EnqueueError::AlreadyQueued),
        "nor one still waiting in the queue"
    );
    assert_eq!(bureau.queued_len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_counterparty_can_be_audited_again_once_its_job_is_done() {
    let (_dir, store) = store();
    let (bureau, _worker) = spawn(Arc::new(MockReppoClient::new(vec![], 0)), &store, funded());
    let first = bureau.enqueue(request("repeat")).unwrap();
    wait_terminal(&bureau, &first.job_id).await;
    let second = bureau
        .enqueue(request("repeat"))
        .expect("slot is free again");
    assert!(matches!(
        wait_terminal(&bureau, &second.job_id).await,
        JobStatus::Settled { .. }
    ));
    assert_eq!(bureau.credits_remaining(), 800);
}

#[tokio::test(start_paused = true)]
async fn enqueue_refuses_an_id_that_differs_from_the_name() {
    // The hot path reads profiles by counterparty_name while SPEC §7.2 names
    // the audit field counterparty_id. Until the spec unifies the key, the
    // two must be the same text or a score would land where nobody reads it.
    let (_dir, store) = store();
    let (bureau, _worker) = spawn(Arc::new(MockReppoClient::new(vec![], 0)), &store, funded());
    let mut mismatched = request("acme");
    mismatched.counterparty_name = "Acme Corp".to_string();
    assert_eq!(
        bureau.enqueue(mismatched),
        Err(EnqueueError::IdNameMismatch)
    );
    assert_eq!(bureau.queued_len(), 0);
}

#[tokio::test(start_paused = true)]
async fn enqueue_bounds_the_identifiers_it_will_store_and_send() {
    let (_dir, store) = store();
    let (bureau, _worker) = spawn(Arc::new(MockReppoClient::new(vec![], 0)), &store, funded());

    let too_long = |field: &'static str, max: usize| EnqueueError::TooLong { field, max };

    let mut long_id = request("x");
    long_id.counterparty_id = "a".repeat(257);
    long_id.counterparty_name = "a".repeat(257);
    assert_eq!(
        bureau.enqueue(long_id),
        Err(too_long("counterparty_id", 256))
    );

    let mut long_name = request("x");
    long_name.counterparty_name = "b".repeat(257);
    assert_eq!(
        bureau.enqueue(long_name),
        Err(too_long("counterparty_name", 256)),
        "length is judged before the id/name comparison"
    );

    let mut long_url = request("x");
    long_url.target_url = format!("https://{}", "c".repeat(2_041));
    assert_eq!(long_url.target_url.len(), 2_049);
    assert_eq!(bureau.enqueue(long_url), Err(too_long("target_url", 2_048)));

    // Exactly at the limits is accepted.
    let mut at_limits = request("x");
    at_limits.counterparty_id = "d".repeat(256);
    at_limits.counterparty_name = "d".repeat(256);
    at_limits.target_url = format!("https://{}", "e".repeat(2_040));
    assert_eq!(at_limits.target_url.len(), 2_048);
    assert!(bureau.enqueue(at_limits).is_ok());
}

#[tokio::test(start_paused = true)]
async fn a_score_is_never_written_onto_a_profile_for_a_different_url() {
    // The evaluation judged https://acme.example.com; the stored profile is
    // for another site. Attaching the score would vouch for the wrong party.
    let (_dir, store) = store();
    let existing = Profile {
        counterparty_name: "acme".to_string(),
        target_url: "https://the-real-acme.example".to_string(),
        score: None,
    };
    store.upsert_profile("acme".to_string(), existing.clone());
    let (bureau, _worker) = spawn(Arc::new(MockReppoClient::new(vec![], 0)), &store, funded());
    let enqueued = bureau.enqueue(request("acme")).unwrap();
    let status = wait_terminal(&bureau, &enqueued.job_id).await;

    let JobStatus::Failed { reason } = status else {
        panic!("expected Failed, got {status:?}");
    };
    assert!(reason.contains("target_url mismatch"), "{reason}");
    assert!(
        reason.contains("ev_mock_1"),
        "the evaluation id stays recoverable: {reason}"
    );
    assert_eq!(store.profile("acme"), Some(existing), "profile untouched");
    assert_eq!(
        bureau.credits_remaining(),
        900,
        "the evaluation settled, so Reppo charged for it anyway"
    );
}

#[tokio::test(start_paused = true)]
async fn a_poll_that_gives_up_counts_the_credit_as_possibly_spent() {
    // The evaluation may settle and be charged after we stop watching it.
    let (_dir, store) = store();
    let mock = Arc::new(MockReppoClient::new(vec![], u32::MAX));
    let (bureau, _worker) = spawn(mock, &store, funded());
    let enqueued = bureau.enqueue(request("slow")).unwrap();
    let status = wait_terminal(&bureau, &enqueued.job_id).await;

    assert!(matches!(status, JobStatus::Failed { .. }), "{status:?}");
    assert_eq!(
        bureau.credits_remaining(),
        900,
        "pessimistic: a PENDING evaluation we abandoned may still be charged"
    );
}

/// The first status read resolves the evaluation at the mock (so Reppo's
/// slot frees up) but reports a permanent error to the worker.
struct FatalOnFirstPoll {
    inner: MockReppoClient,
    armed: Mutex<bool>,
}

#[async_trait]
impl ReppoClient for FatalOnFirstPoll {
    async fn submit(&self, submission: &EvalSubmission) -> Result<String, ReppoError> {
        self.inner.submit(submission).await
    }

    async fn status(&self, eval_id: &str) -> Result<EvalStatus, ReppoError> {
        let armed = std::mem::replace(&mut *self.armed.lock().unwrap(), false);
        let result = self.inner.status(eval_id).await;
        if armed {
            return Err(ReppoError::Unauthorized);
        }
        result
    }
}

#[tokio::test(start_paused = true)]
async fn a_fatal_poll_counts_the_credit_and_the_next_job_still_runs() {
    let (_dir, store) = store();
    let client = Arc::new(FatalOnFirstPoll {
        inner: MockReppoClient::new(vec![], 0),
        armed: Mutex::new(true),
    });
    let (bureau, _worker) = spawn(client, &store, funded());
    let first = bureau.enqueue(request("fatal")).unwrap();
    let second = bureau.enqueue(request("healthy")).unwrap();

    let first_status = wait_terminal(&bureau, &first.job_id).await;
    let JobStatus::Failed { reason } = first_status else {
        panic!("expected Failed, got {first_status:?}");
    };
    assert!(reason.contains("ev_mock_1"), "{reason}");
    assert!(store.profile("fatal").is_none());

    assert!(matches!(
        wait_terminal(&bureau, &second.job_id).await,
        JobStatus::Settled { .. }
    ));
    assert!(store.profile("healthy").is_some());
    assert!(!bureau.is_busy());
    assert_eq!(
        bureau.credits_remaining(),
        800,
        "the abandoned job and the healthy one each count 100"
    );
}

#[tokio::test(start_paused = true)]
async fn a_denied_evaluation_is_fatal_and_counted() {
    struct Denies {
        inner: MockReppoClient,
    }
    #[async_trait]
    impl ReppoClient for Denies {
        async fn submit(&self, submission: &EvalSubmission) -> Result<String, ReppoError> {
            self.inner.submit(submission).await
        }
        async fn status(&self, _: &str) -> Result<EvalStatus, ReppoError> {
            Ok(EvalStatus::Denied)
        }
    }
    let (_dir, store) = store();
    let client = Arc::new(Denies {
        inner: MockReppoClient::new(vec![], 0),
    });
    let (bureau, _worker) = spawn(client, &store, funded());
    let enqueued = bureau.enqueue(request("denied")).unwrap();
    let status = wait_terminal(&bureau, &enqueued.job_id).await;
    let JobStatus::Failed { reason } = status else {
        panic!("expected Failed, got {status:?}");
    };
    assert!(reason.contains("denied"), "{reason}");
    assert_eq!(bureau.credits_remaining(), 900);
    assert!(store.profile("denied").is_none());
}

/// Panics inside `status`, after the evaluation was submitted.
struct PanicsOnPoll {
    inner: MockReppoClient,
}

#[async_trait]
impl ReppoClient for PanicsOnPoll {
    async fn submit(&self, submission: &EvalSubmission) -> Result<String, ReppoError> {
        self.inner.submit(submission).await
    }

    async fn status(&self, _: &str) -> Result<EvalStatus, ReppoError> {
        panic!("simulated bug while polling");
    }
}

#[tokio::test(start_paused = true)]
async fn a_job_that_panics_after_submitting_counts_its_credit() {
    let (_dir, store) = store();
    let client = Arc::new(PanicsOnPoll {
        inner: MockReppoClient::new(vec![], 0),
    });
    let (bureau, _worker) = spawn(client, &store, funded());
    let enqueued = bureau.enqueue(request("boom")).unwrap();
    let status = wait_terminal(&bureau, &enqueued.job_id).await;

    let JobStatus::Failed { reason } = status else {
        panic!("expected Failed, got {status:?}");
    };
    assert!(reason.contains("panicked"), "{reason}");
    assert_eq!(
        bureau.credits_remaining(),
        900,
        "an evaluation was submitted before the panic, so it may be charged"
    );
    assert!(!bureau.is_busy());
}
