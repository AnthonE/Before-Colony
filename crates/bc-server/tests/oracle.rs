//! `--oracle jev` end to end: the sector sends tactical pictures for a ZERO pilot, the worker asks a
//! (mock) Jev endpoint, advice flows back into the tick, and the pilot's ZERO info shows it.

use std::time::{Duration, Instant};

use axum::Json;
use axum::Router;
use axum::routing::post;
use bc_bot::{BotClient, BotConfig, DollBrain};
use bc_proto::buttons::ZERO;
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, OracleKind};
use serde_json::{Value, json};

async fn fake_jev(Json(body): Json<Value>) -> Json<Value> {
    // Echo a confident answer for every threat the picture mentions.
    let mut answers = serde_json::Map::new();
    if let Some(q) = body["questions"].as_object() {
        if let Some(crit) = q.get("target").and_then(|t| t["criteria"].as_object()) {
            let n = crit.len() as f64;
            let probs: serde_json::Map<String, Value> =
                crit.keys().map(|k| (k.clone(), json!(1.0 / n))).collect();
            answers.insert("target".into(), json!({ "type": "choice", "probabilities": probs }));
        }
        for k in ["next_t1", "next_t2"] {
            if q.contains_key(k) {
                answers.insert(k.into(), json!({ "type": "choice", "probabilities": { "COAST": 0.1, "FWD": 0.4, "BACK": 0.1, "LEFT": 0.1, "RIGHT": 0.1, "UP": 0.1, "DOWN": 0.1 } }));
            }
        }
    }
    answers.insert(
        "threat".into(),
        json!({ "type": "score", "probabilities": { "0": 0.1, "1": 0.2, "2": 0.4, "3": 0.3 } }),
    );
    answers.insert("flanked".into(), json!({ "type": "noul", "noul": 0.3 }));
    Json(json!({ "model": "jev-mock", "answers": answers }))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn jev_advice_reaches_the_zero_pilot() -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let jev_url = format!("http://{}", listener.local_addr()?);
    tokio::spawn(
        async move { axum::serve(listener, Router::new().route("/v1/systemone", post(fake_jev))).await },
    );

    let cfg = Config {
        mode: Mode::Game,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 8,
        oracle: OracleKind::Jev,
        jev_key: Some("test-key".into()),
        jev_url: Some(jev_url),
        ..Config::default()
    };
    let server = bc_server::start(cfg).await?;
    let mut bot = BotClient::connect(&BotConfig {
        server: format!("http://{}", server.http_addr),
        name: "Zero-Pilot".into(),
        frame: FrameId::WingZero,
        faction: Faction::Colonies,
    })
    .await?;
    let mut brain = DollBrain::new(99);
    let mut saw_jev = false;
    let end = Instant::now() + Duration::from_secs(60);
    while Instant::now() < end && !saw_jev {
        bot.step(&mut |ctx| {
            let mut cmd = brain.decide(ctx);
            cmd.buttons |= ZERO;
            cmd
        })
        .await?;
        saw_jev = bot.world().zero.is_some_and(|z| z.source_jev);
    }
    let status = server.status();
    println!("pictures {} advice {}", status["game"]["pictures"], status["game"]["advice"]);
    assert!(status["game"]["pictures"].as_u64().unwrap() > 0, "sector sent no pictures");
    assert!(status["game"]["advice"].as_u64().unwrap() > 0, "no advice came back");
    assert!(saw_jev, "the pilot's ZERO display never showed Jev-blended advice");
    bot.close();
    server.shutdown();
    Ok(())
}
