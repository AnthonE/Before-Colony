//! Dev HTTP server: serves the built web client, `/cert-hash` for WebTransport's
//! `serverCertificateHashes`, and `/status` metrics.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::routing::get;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

type StatusFn = Arc<dyn Fn() -> serde_json::Value + Send + Sync>;

#[derive(Clone)]
pub struct HttpState {
    pub cert_hash_hex: String,
    pub wt_port: u16,
    pub web_dir: Option<PathBuf>,
    pub status: StatusFn,
}

pub fn router(state: HttpState) -> Router {
    let web_dir = state.web_dir.clone();
    let mut router =
        Router::new().route("/cert-hash", get(cert_hash)).route("/status", get(status)).with_state(state);
    if let Some(dir) = web_dir {
        router = router.fallback_service(ServeDir::new(dir).precompressed_br().precompressed_gzip());
    }
    // Dev server: never let the browser run a stale wasm build.
    router.layer(SetResponseHeaderLayer::overriding(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    ))
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
