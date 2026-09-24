//! JevOracle against a local mock of `POST /v1/systemone`: the request contract, answer parsing,
//! timeouts, rate limiting, the circuit breaker, malformed responses, and the worker plumbing.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use bc_proto::{FrameId, NO_SLOT, PilotKind};
use bc_sim::zero::advice::{PICTURE_THREATS, ThreatBrief};
use bc_zero::{JevOracle, OracleError, TacticalOracle, TacticalPicture};
use serde_json::{Value, json};
use tokio::sync::Mutex;

#[derive(Clone, Copy, PartialEq)]
enum Behaviour {
    Ok,
    Slow,
    RateLimit,
    Overloaded,
    Garbage,
}

#[derive(Clone)]
struct Mock {
    behaviour: Arc<Mutex<Behaviour>>,
    last_body: Arc<Mutex<Option<Value>>>,
    last_auth: Arc<Mutex<Option<String>>>,
    calls: Arc<AtomicU32>,
}

async fn systemone(State(m): State<Mock>, headers: HeaderMap, Json(body): Json<Value>) -> impl IntoResponse {
    m.calls.fetch_add(1, Ordering::SeqCst);
    *m.last_auth.lock().await =
        headers.get("authorization").and_then(|v| v.to_str().ok()).map(str::to_string);
    *m.last_body.lock().await = Some(body.clone());
    let behaviour = *m.behaviour.lock().await;
    match behaviour {
        Behaviour::Slow => {
            tokio::time::sleep(Duration::from_secs(1)).await;
            (StatusCode::OK, Json(json!({}))).into_response()
        }
        Behaviour::RateLimit => (StatusCode::TOO_MANY_REQUESTS, "slow down").into_response(),
        Behaviour::Overloaded => (StatusCode::from_u16(529).unwrap(), "overloaded").into_response(),
        Behaviour::Garbage => (StatusCode::OK, "{\"answers\": 7").into_response(),
        Behaviour::Ok => {
            // Answer shaped exactly like Jev: choice with confidence + probabilities, score keyed by
            // level index, noul with only a probability.
            Json(json!({
                "model": "jev-1.13.0",
                "answers": {
                    "target": { "type": "choice", "choice": "t2", "confidence": 0.61,
                                "probabilities": { "t1": 0.25, "t2": 0.70, "t3": 0.05 } },
                    "next_t1": { "type": "choice", "choice": "LEFT", "confidence": 0.5,
                                 "probabilities": { "COAST": 0.05, "FWD": 0.05, "BACK": 0.05, "LEFT": 0.6, "RIGHT": 0.15, "UP": 0.05, "DOWN": 0.05 } },
                    "evade": { "type": "choice", "choice": "BREAK-HIGH", "confidence": 0.4,
                               "probabilities": { "HOLD": 0.1, "PUSH": 0.05, "BREAK-BACK": 0.05, "BREAK-LEFT": 0.1, "BREAK-RIGHT": 0.1, "BREAK-HIGH": 0.5, "BREAK-LOW": 0.1 } },
                    "threat": { "type": "score", "score": 2.1, "confidence": 0.5,
                                "probabilities": { "0": 0.05, "1": 0.15, "2": 0.45, "3": 0.35 },
                                "legend": { "0": "low", "1": "moderate", "2": "high", "3": "lethal" } },
                    "flanked": { "type": "noul", "noul": 0.82 }
                },
                "usage": { "input_tokens": 612, "output_tokens": 0 }
            }))
            .into_response()
        }
    }
}

async fn mock() -> (String, Mock) {
    let m = Mock {
        behaviour: Arc::new(Mutex::new(Behaviour::Ok)),
        last_body: Arc::new(Mutex::new(None)),
        last_auth: Arc::new(Mutex::new(None)),
        calls: Arc::new(AtomicU32::new(0)),
    };
    let app = Router::new().route("/v1/systemone", post(systemone)).with_state(m.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), m)
}

fn picture() -> TacticalPicture {
    let mut p = TacticalPicture {
        pilot: 5,
        tick: 900,
        frame: FrameId::WingZero,
        speed: 140.0,
        ..TacticalPicture::default()
    };
    let briefs = [
        ThreatBrief {
            slot: 11,
            frame: FrameId::Taurus,
            pilot: PilotKind::MobileDoll,
            distance: 850.0,
            closing_speed: 180.0,
            bearing_deg: 8.0,
            hull: 0.9,
            firing: true,
            aiming_at_me: true,
            accel_g: 3.0,
            ..ThreatBrief::default()
        },
        ThreatBrief {
            slot: 12,
            frame: FrameId::Virgo,
            pilot: PilotKind::MobileDoll,
            distance: 2_400.0,
            closing_speed: -20.0,
            bearing_deg: 95.0,
            hull: 0.4,
            ..ThreatBrief::default()
        },
        ThreatBrief {
            slot: 13,
            frame: FrameId::Leo,
            pilot: PilotKind::Human,
            distance: 4_100.0,
            bearing_deg: 170.0,
            ..ThreatBrief::default()
        },
    ];
    for (k, b) in briefs.iter().enumerate() {
        p.threats[k] = *b;
    }
    p.n = 3;
    p
}

#[tokio::test]
async fn request_contract_and_parsing() {
    let (url, m) = mock().await;
    let jev = JevOracle::with_endpoint("test-key".into(), url, "jev-latest".into());
    let advice = jev.assess(&picture()).await.expect("advice");
    // Request contract.
    assert_eq!(m.last_auth.lock().await.as_deref(), Some("Bearer test-key"));
    let body = m.last_body.lock().await.clone().unwrap();
    assert_eq!(body["model"], "jev-latest");
    let q = &body["questions"];
    assert_eq!(q["target"]["type"], "choice");
    assert_eq!(q["threat"]["type"], "score");
    assert_eq!(q["threat"]["criteria"], json!(["low", "moderate", "high", "lethal"]));
    assert_eq!(q["flanked"]["type"], "noul");
    assert!(q["flanked"].get("criteria").is_none(), "noul questions take no criteria");
    assert_eq!(q["next_t1"]["criteria"].as_object().unwrap().len(), 7);
    // The state speaks in named buckets, not raw numbers.
    let t1 = &body["state"]["threats"][0];
    assert_eq!(t1["distance"], "close (under 1 km)");
    assert_eq!(t1["range_rate"], "closing fast");
    assert_eq!(t1["pilot"], "Mobile Doll (unmanned, predictable, no G limit)");
    // Parsing.
    assert!(advice.from_jev);
    assert_eq!(advice.pilot, 5);
    assert_eq!(advice.computed_at_tick, 900);
    assert_eq!(&advice.target_slots[..3], &[11, 12, 13]);
    assert!((advice.target_probs[1] - 0.70).abs() < 1e-5);
    assert_eq!(advice.maneuver_slots[0], 11);
    assert!((advice.maneuver_probs[0][3] - 0.6).abs() < 1e-5, "LEFT is hypothesis 3");
    assert_eq!(advice.maneuver_slots[1], NO_SLOT, "next_t2 was not answered: no opinion");
    assert!((advice.own_action_probs[5] - 0.5).abs() < 1e-5, "BREAK-HIGH is hypothesis 5");
    assert!((advice.threat_probs[2] - 0.45).abs() < 1e-5);
    assert!((advice.flanked - 0.82).abs() < 1e-5);
    let sum: f32 = advice.target_probs[..PICTURE_THREATS].iter().sum();
    assert!((sum - 1.0).abs() < 1e-4);
}

#[tokio::test]
async fn slow_answers_time_out_quickly() {
    let (url, m) = mock().await;
    *m.behaviour.lock().await = Behaviour::Slow;
    let jev = JevOracle::with_endpoint("k".into(), url, "jev-latest".into());
    let started = Instant::now();
    let err = jev.assess(&picture()).await.unwrap_err();
    assert!(matches!(err, OracleError::Timeout), "{err}");
    assert!(started.elapsed() < Duration::from_millis(700), "took {:?}", started.elapsed());
}

#[tokio::test]
async fn rate_limits_open_the_circuit_breaker() {
    let (url, m) = mock().await;
    *m.behaviour.lock().await = Behaviour::RateLimit;
    let jev = JevOracle::with_endpoint("k".into(), url.clone(), "jev-latest".into());
    for _ in 0..3 {
        assert!(matches!(jev.assess(&picture()).await, Err(OracleError::RateLimited)));
    }
    let before = m.calls.load(Ordering::SeqCst);
    assert!(matches!(jev.assess(&picture()).await, Err(OracleError::CircuitOpen)));
    assert_eq!(m.calls.load(Ordering::SeqCst), before, "an open breaker must not call out");

    *m.behaviour.lock().await = Behaviour::Overloaded;
    let jev = JevOracle::with_endpoint("k".into(), url, "jev-latest".into());
    assert!(matches!(jev.assess(&picture()).await, Err(OracleError::Overloaded(529))));
}

#[tokio::test]
async fn garbage_is_rejected_not_trusted() {
    let (url, m) = mock().await;
    *m.behaviour.lock().await = Behaviour::Garbage;
    let jev = JevOracle::with_endpoint("k".into(), url, "jev-latest".into());
    assert!(matches!(jev.assess(&picture()).await, Err(OracleError::Malformed(_))));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_moves_pictures_to_advice() {
    let (url, _m) = mock().await;
    let (mut pic_tx, pic_rx) = rtrb::RingBuffer::new(16);
    let (adv_tx, mut adv_rx) = rtrb::RingBuffer::new(16);
    let _worker =
        bc_zero::spawn_worker(pic_rx, adv_tx, JevOracle::with_endpoint("k".into(), url, "jev-latest".into()))
            .unwrap();
    pic_tx.push(picture()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(a) = adv_rx.pop() {
            assert!(a.from_jev);
            assert_eq!(a.pilot, 5);
            break;
        }
        assert!(Instant::now() < deadline, "no advice arrived");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
