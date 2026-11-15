//! `GET /api/v1/agent/{id}` (docs/SPEC.md §7.5): a profile read with no
//! model call. The providers here panic if touched, so every passing test
//! also proves the lookup never reaches Clef, SERV or Reppo.

use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use solv::score::{Pillars, SettledScore};
use solv::state::{Profile, Store};
use solv::tier1::{ClefClient, ClefError, ClefRequest, ClefResponse};
use solv::tier2::{ServAsk, ServClient, ServError, ServVerdict};
use solv::{AppState, build_router};
use tower::ServiceExt;

const DAY: u64 = 86_400;

struct NeverClef;

#[async_trait]
impl ClefClient for NeverClef {
    async fn ask(&self, _: &ClefRequest) -> Result<ClefResponse, ClefError> {
        panic!("the agent lookup must not call Clef");
    }
}

struct NeverServ;

#[async_trait]
impl ServClient for NeverServ {
    async fn reason(&self, _: &ServAsk) -> Result<ServVerdict, ServError> {
        panic!("the agent lookup must not call SERV");
    }
}

fn router(profiles: Vec<(&str, Option<SettledScore>)>) -> (axum::Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state.json")).unwrap();
    for (id, score) in profiles {
        store.upsert_profile(
            id.to_string(),
            Profile {
                counterparty_name: id.to_string(),
                target_url: format!("https://{id}.example.com"),
                score,
            },
        );
    }
    let state = AppState::new(store, Arc::new(NeverClef), Arc::new(NeverServ));
    (build_router(state), dir)
}

fn settled(reppo_score: f64, safety: u8, age_days: u64) -> SettledScore {
    SettledScore {
        reppo_score,
        eval_id: "ev_lookup".to_string(),
        settled_at_unix: solv::api::now_unix() - age_days * DAY,
        pillars: Pillars {
            reliability: 8,
            track_record: 9,
            safety,
            maturity: 7,
        },
    }
}

async fn get(router: &axum::Router, uri: &str) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn a_rated_counterparty_returns_band_pillars_source_and_cap() {
    let (router, _dir) = router(vec![("datafeed", Some(settled(100.0, 5, 1)))]);
    let (status, json) = get(&router, "/api/v1/agent/datafeed").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["rated"], true);
    assert_eq!(json["tier"], "EXCEPTIONAL");
    assert_eq!(json["fico_score"], 850);
    assert_eq!(json["score_age_days"], 1);
    assert_eq!(json["pillars"]["track_record"], 9);
    assert_eq!(json["score_source"]["provider"], "reppo");
    assert_eq!(json["score_source"]["eval_id"], "ev_lookup");
    assert_eq!(json["max_safe_spend"], 500.0);
    assert_eq!(json["target_url"], "https://datafeed.example.com");
}

#[tokio::test]
async fn an_unknown_id_is_ok_and_unrated_with_no_spend() {
    let (router, _dir) = router(vec![]);
    let (status, json) = get(&router, "/api/v1/agent/nobody").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["rated"], false);
    assert_eq!(json["tier"], "UNRATED");
    assert_eq!(json["fico_score"], Value::Null);
    assert_eq!(json["pillars"], Value::Null);
    assert_eq!(json["score_source"], Value::Null);
    assert_eq!(json["max_safe_spend"], 0.0);
}

#[tokio::test]
async fn a_profile_without_a_score_and_an_expired_score_are_both_unrated() {
    let (router, _dir) = router(vec![
        ("fresh", None),
        ("stale", Some(settled(100.0, 9, 91))),
    ]);
    for id in ["fresh", "stale"] {
        let (status, json) = get(&router, &format!("/api/v1/agent/{id}")).await;
        assert_eq!(status, StatusCode::OK, "{id}");
        assert_eq!(json["rated"], false, "{id}");
        assert_eq!(json["tier"], "UNRATED", "{id}");
        assert_eq!(
            json["pillars"],
            Value::Null,
            "{id}: no expired evidence shown as current"
        );
        assert_eq!(json["max_safe_spend"], 0.0, "{id}");
    }
}

#[tokio::test]
async fn a_safety_pillar_at_the_floor_advertises_no_spend() {
    let (router, _dir) = router(vec![("risky", Some(settled(100.0, 3, 1)))]);
    let (_, json) = get(&router, "/api/v1/agent/risky").await;
    assert_eq!(json["tier"], "EXCEPTIONAL");
    assert_eq!(
        json["max_safe_spend"], 0.0,
        "row 5 would block this counterparty"
    );
}

#[tokio::test]
async fn an_oversized_id_is_refused_and_a_hostile_id_is_returned_as_data() {
    let (router, _dir) = router(vec![]);
    let (status, _) = get(&router, &format!("/api/v1/agent/{}", "a".repeat(2049))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, json) = get(&router, "/api/v1/agent/%3Cscript%3Ealert(1)%3C%2Fscript%3E").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], "<script>alert(1)</script>");
    assert_eq!(json["rated"], false);
}
