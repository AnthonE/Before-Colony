//! TypeSafe **Jev** as a ZERO System oracle.
//!
//! Jev ("System One" model) answers typed questions: `choice`, `score` and `noul`, each with
//! calibrated probabilities. One `POST /v1/systemone` carries every question for a pilot at once.
//! Questions never see each other, so asking up front costs nothing extra ("speculative fan-out").
//!
//! Jev is weak at numeric precision, so the state it sees is named buckets ("close, closing fast",
//! "left arm destroyed"). All arithmetic stays in the simulation.
//!
//! Failures are expected and harmless. The call has a 400 ms timeout; 429/529/5xx and timeouts
//! count toward a circuit breaker (3 in a row → 10 s cool-down). The API key never leaves the
//! server.

mod questions;
mod schema;
mod state;

use std::sync::Mutex;
use std::time::{Duration, Instant};

pub use questions::{MANEUVER_LABELS, OWN_LABELS, THREAT_LEVEL_LABELS, build_request};
pub use schema::parse_advice;

use crate::{OracleError, TacticalAdvice, TacticalOracle, TacticalPicture};

pub const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";
pub const DEFAULT_MODEL: &str = "jev-latest";
/// Past this the advice would be stale before it arrived.
pub const TIMEOUT: Duration = Duration::from_millis(400);
const BREAKER_THRESHOLD: u32 = 3;
const BREAKER_COOLDOWN: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Breaker {
    failures: u32,
    open_until: Option<Instant>,
}

pub struct JevOracle {
    http: reqwest::Client,
    key: String,
    base_url: String,
    model: String,
    breaker: Mutex<Breaker>,
}

impl JevOracle {
    pub fn new(key: String) -> Self {
        Self::with_endpoint(key, DEFAULT_BASE_URL.into(), DEFAULT_MODEL.into())
    }

    /// Points the oracle at another endpoint (tests use a local mock).
    pub fn with_endpoint(key: String, base_url: String, model: String) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .pool_max_idle_per_host(8)
            .build()
            .expect("reqwest client");
        Self {
            http,
            key,
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
            breaker: Mutex::new(Breaker::default()),
        }
    }

    fn breaker_open(&self) -> bool {
        let b = self.breaker.lock().unwrap_or_else(|e| e.into_inner());
        b.open_until.is_some_and(|t| Instant::now() < t)
    }

    fn record(&self, ok: bool) {
        let mut b = self.breaker.lock().unwrap_or_else(|e| e.into_inner());
        if ok {
            b.failures = 0;
            b.open_until = None;
        } else {
            b.failures += 1;
            if b.failures >= BREAKER_THRESHOLD {
                b.open_until = Some(Instant::now() + BREAKER_COOLDOWN);
                b.failures = 0;
                tracing::warn!("Jev circuit breaker open for {BREAKER_COOLDOWN:?}");
            }
        }
    }

    async fn call(&self, picture: &TacticalPicture) -> Result<TacticalAdvice, OracleError> {
        let body = build_request(picture, &self.model);
        let res = self
            .http
            .post(format!("{}/v1/systemone", self.base_url))
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() { OracleError::Timeout } else { OracleError::Transport(e.to_string()) }
            })?;
        let status = res.status().as_u16();
        match status {
            200 => {}
            401 => return Err(OracleError::Unauthorized),
            422 => return Err(OracleError::BadRequest(res.text().await.unwrap_or_default())),
            429 => return Err(OracleError::RateLimited),
            529 | 503 => return Err(OracleError::Overloaded(status)),
            s => return Err(OracleError::Http(s)),
        }
        let json: serde_json::Value = res.json().await.map_err(|e| {
            if e.is_timeout() { OracleError::Timeout } else { OracleError::Malformed(e.to_string()) }
        })?;
        parse_advice(picture, &json)
    }
}

impl TacticalOracle for JevOracle {
    fn name(&self) -> &'static str {
        "jev"
    }

    async fn assess(&self, picture: &TacticalPicture) -> Result<TacticalAdvice, OracleError> {
        if self.breaker_open() {
            return Err(OracleError::CircuitOpen);
        }
        let result = self.call(picture).await;
        let transient = matches!(
            result,
            Err(OracleError::Timeout
                | OracleError::RateLimited
                | OracleError::Overloaded(_)
                | OracleError::Http(_)
                | OracleError::Transport(_))
        );
        self.record(!transient);
        result
    }
}
