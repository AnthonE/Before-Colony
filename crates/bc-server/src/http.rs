//! The HTTP side: serves the built web client, `/cert-hash` for WebTransport's
//! `serverCertificateHashes` (its `hash` is null when the certificate is CA-issued: nothing to pin),
//! `/status` metrics, `/status.json`, the player count in the shape the Elo Pros launcher and
//! store poll, `/online.json`, the wallets on now for the store's playtime, and `/floor.json`, who
//! stands on the Exchange floor and what its boards show (`crate::floor`), and `/nonce`, what an Elo
//! Pros pass for this server must carry (`crate::elo::Nonces`). The JSON routes
//! answer pages on the store's origin too (CORS,
//! `Config::cors_origins`), since the store serves the game's page from its own site.
//!
//! With a data dir (`Config::data_dir`), `POST /bugs` takes a player's report, or a page's crash
//! (`crate::bugs`): the body raw, whatever its content type, so a page on another site can send it
//! without a preflight (`text/plain`) and a crashing tab as a beacon. Without one the route isn't
//! there at all.
//!
//! With the dev hooks on (`Config::dev_hooks`, never in a deployment), `POST /dev/incident` raises
//! one of the Watch's incidents by hand (`crate::watch::Watch::raise`), and `POST /dev/scare` has the
//! ring's town's crowd run from a place (`crate::street::Street::scare_at`). Without them the routes
//! aren't there at all.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

type StatusFn = Arc<dyn Fn() -> serde_json::Value + Send + Sync>;
/// Raises an incident as asked: its id, or why not.
pub type IncidentFn = Arc<dyn Fn(&IncidentAsk) -> Result<u32, String> + Send + Sync>;

/// `POST /dev/incident`'s body: where on the ring's floor (across it, round it, m), how bad (1 to
/// 5) and what for (an offence's slug; `hit_someone` if none).
#[derive(Clone, Debug, serde::Deserialize)]
pub struct IncidentAsk {
    pub x: f32,
    pub s: f32,
    pub level: u8,
    #[serde(default)]
    pub cause: Option<String>,
}

/// Has the town's crowd round a place run from it: how many were taken out of it, or why not.
pub type ScareFn = Arc<dyn Fn(&ScareAsk) -> Result<usize, String> + Send + Sync>;

/// `POST /dev/scare`'s body: where on the ring's floor (across it, round it, m), and how far round
/// it (m; 20 if not said).
#[derive(Clone, Debug, serde::Deserialize)]
pub struct ScareAsk {
    pub x: f32,
    pub s: f32,
    #[serde(default)]
    pub reach: Option<f32>,
}

#[derive(Clone)]
pub struct HttpState {
    /// The self-signed certificate's SHA-256 for browsers to pin; None with a CA-issued one.
    pub cert_hash_hex: Option<String>,
    pub wt_port: u16,
    pub web_dir: Option<PathBuf>,
    pub status: StatusFn,
    /// `/status.json`: `{players, max_players, tick, …}`.
    pub launcher_status: StatusFn,
    /// `/online.json`: the wallets on now, for the store's playtime (`crate::online`).
    pub online: StatusFn,
    /// `/floor.json`: the Exchange floor, for the store's trade terminal (`crate::floor`).
    pub floor: StatusFn,
    /// `/nonce`: a fresh nonce for a pass, or null when this server takes none.
    pub nonce: StatusFn,
    /// `POST /dev/incident` and `POST /dev/scare`, with the dev hooks on (none: no such routes).
    pub incident: Option<IncidentFn>,
    pub scare: Option<ScareFn>,
    /// `POST /bugs`, with a data dir (none: no such route).
    pub bugs: Option<Arc<crate::bugs::Intake>>,
    /// Origins allowed to read the JSON routes from another site.
    pub cors_origins: Arc<[String]>,
}

pub fn router(state: HttpState) -> Router {
    let web_dir = state.web_dir.clone();
    let origins = state.cors_origins.clone();
    let mut api = Router::new()
        .route("/cert-hash", get(cert_hash))
        .route("/status", get(status))
        .route("/status.json", get(launcher_status))
        .route("/online.json", get(online))
        .route("/floor.json", get(floor))
        .route("/nonce", get(nonce));
    if state.incident.is_some() {
        api = api.route("/dev/incident", post(incident));
    }
    if state.scare.is_some() {
        api = api.route("/dev/scare", post(scare));
    }
    if state.bugs.is_some() {
        api = api.route(
            "/bugs",
            post(bugs).options(preflight).layer(DefaultBodyLimit::max(crate::bugs::BODY_MAX)),
        );
    }
    let api = api.with_state(state).layer(middleware::from_fn(move |req: Request, next: Next| {
        let origins = origins.clone();
        async move { cors(&origins, req, next).await }
    }));
    let mut router = api;
    if let Some(dir) = web_dir {
        router = router.fallback_service(ServeDir::new(dir).precompressed_br().precompressed_gzip());
    }
    // Dev server: never let the browser run a stale wasm build.
    router.layer(SetResponseHeaderLayer::overriding(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    ))
}

/// The `Access-Control-Allow-Origin` a request from `headers` gets: its own `Origin`, when that's
/// one of `allowed`.
pub fn allowed_origin(allowed: &[String], headers: &HeaderMap) -> Option<HeaderValue> {
    let origin = headers.get(header::ORIGIN)?;
    let text = origin.to_str().ok()?;
    allowed.iter().any(|a| a == text).then(|| origin.clone())
}

async fn cors(allowed: &[String], req: Request, next: Next) -> Response {
    let allow = allowed_origin(allowed, req.headers());
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(header::VARY, HeaderValue::from_static("Origin"));
    if let Some(origin) = allow {
        h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    }
    res
}

async fn cert_hash(State(s): State<HttpState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "algorithm": "sha-256",
        "hash": s.cert_hash_hex,
        "port": s.wt_port,
        "path": "/bc",
    }))
}

async fn status(State(s): State<HttpState>) -> Json<serde_json::Value> {
    Json((s.status)())
}

async fn launcher_status(State(s): State<HttpState>) -> Json<serde_json::Value> {
    Json((s.launcher_status)())
}

async fn online(State(s): State<HttpState>) -> Json<serde_json::Value> {
    Json((s.online)())
}

async fn floor(State(s): State<HttpState>) -> Json<serde_json::Value> {
    Json((s.floor)())
}

async fn nonce(State(s): State<HttpState>) -> Json<serde_json::Value> {
    Json((s.nonce)())
}

async fn incident(
    State(s): State<HttpState>,
    Json(ask): Json<IncidentAsk>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let raise = s.incident.as_ref().ok_or((StatusCode::NOT_FOUND, String::new()))?;
    let id = raise(&ask).map_err(|why| (StatusCode::BAD_REQUEST, why))?;
    Ok(Json(serde_json::json!({ "id": id })))
}

async fn scare(
    State(s): State<HttpState>,
    Json(ask): Json<ScareAsk>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let scare = s.scare.as_ref().ok_or((StatusCode::NOT_FOUND, String::new()))?;
    let n = scare(&ask).map_err(|why| (StatusCode::BAD_REQUEST, why))?;
    Ok(Json(serde_json::json!({ "promoted": n })))
}

/// `POST /bugs`: a report, read raw (any content type), kept or refused with the reason. The sender
/// counts for its share by [`crate::bugs::sender_of`] and is kept nowhere.
async fn bugs(
    State(s): State<HttpState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    use crate::bugs::No;
    let Some(intake) = s.bugs.clone() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let peer = peer.map_or(IpAddr::from([0, 0, 0, 0]), |Extension(ConnectInfo(a))| a.ip());
    let real_ip = headers.get("x-real-ip").and_then(|v| v.to_str().ok());
    let from = crate::bugs::sender_of(peer, real_ip);
    let unix_s =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let now = std::time::Instant::now();
    let filed = tokio::task::spawn_blocking(move || intake.file(from, &body, unix_s, now)).await;
    let refuse = |code: StatusCode, why: &str| {
        (code, Json(serde_json::json!({ "ok": false, "error": why }))).into_response()
    };
    match filed {
        Ok(Ok(receipt)) => {
            tracing::info!(id = %receipt.id, fingerprint = %receipt.fingerprint, seen = receipt.seen, "a report filed");
            Json(receipt.json()).into_response()
        }
        Ok(Err(No::Bad(why))) => refuse(StatusCode::BAD_REQUEST, &why),
        Ok(Err(No::Busy)) => {
            refuse(StatusCode::TOO_MANY_REQUESTS, "too many reports just now: try again later")
        }
        Ok(Err(No::Full)) => refuse(StatusCode::SERVICE_UNAVAILABLE, "no room for more reports today"),
        Err(_) => refuse(StatusCode::INTERNAL_SERVER_ERROR, "the report was lost"),
    }
}

/// A page that sends its report as anything but plain text asks first (CORS): say it may. The
/// `cors` layer names the origin, when it's one of ours.
async fn preflight() -> Response {
    let mut res = StatusCode::NO_CONTENT.into_response();
    let h = res.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("POST"));
    h.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("content-type"));
    h.insert(header::ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("600"));
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_listed_origins_are_answered() {
        let allowed = [crate::config::STORE_ORIGIN.to_string()];
        let mut h = HeaderMap::new();
        assert_eq!(allowed_origin(&allowed, &h), None);
        h.insert(header::ORIGIN, HeaderValue::from_static("https://elopros.com"));
        assert_eq!(allowed_origin(&allowed, &h), Some(HeaderValue::from_static("https://elopros.com")));
        h.insert(header::ORIGIN, HeaderValue::from_static("https://elopros.com.evil.example"));
        assert_eq!(allowed_origin(&allowed, &h), None);
        h.insert(header::ORIGIN, HeaderValue::from_static("http://elopros.com"));
        assert_eq!(allowed_origin(&allowed, &h), None);
    }
}
