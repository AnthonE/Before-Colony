//! `POST /bugs` on a real server: a page's report lands under the data dir as `bc-report-*.json`
//! (and its `.png`), sent as plain text the way a page on another site, or a crashing tab's
//! beacon, sends it; each address gets its share (behind nginx, its `X-Real-IP`); a body over the
//! cap is refused; and nothing secret is ever written. Without a data dir there's no such route.

use std::path::{Path, PathBuf};

use base64::Engine;
use bc_server::{Config, Mode};
use serde_json::{Value, json};

const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const WALLET: &str = "0x52908400098527886E0F7030069857D2E4169EE7";

fn report() -> Value {
    json!({
        "format": 1,
        "kind": "look",
        "fingerprint": "0000000000000000",
        "issue_title": "[look] the canal is the wrong colour",
        "read_first": "docs/FREE_ROAM.md",
        "reported": "2026-11-23T10:00:00Z",
        "build": {"version": "0.1.0", "commit": "9fc2ff6abc", "platform": "web", "runtime": "browser"},
        "words": "the canal is the wrong colour",
        "place": {"screen": "playing", "place": "town", "town_feet": "1.0,0.0,-200.0"},
        "net": {"link": "connected", "rtt_ms": 21.5},
    })
}

async fn start(data: Option<PathBuf>) -> anyhow::Result<bc_server::ServerHandle> {
    let cfg = Config {
        mode: Mode::Echo,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        data_dir: data,
        ..Config::default()
    };
    bc_server::start(cfg).await
}

/// POSTs `body` as plain text (no preflight), as from `real_ip` behind nginx when given.
async fn post(base: &str, body: Vec<u8>, real_ip: Option<&str>) -> anyhow::Result<(u16, Value)> {
    let mut req = reqwest::Client::new()
        .post(format!("{base}/bugs"))
        .header("Content-Type", "text/plain;charset=UTF-8")
        .header("Origin", "https://elopros.com")
        .body(body);
    if let Some(ip) = real_ip {
        req = req.header("X-Real-IP", ip);
    }
    let res = req.send().await?;
    let status = res.status().as_u16();
    let text = res.text().await?;
    Ok((status, serde_json::from_str(&text).unwrap_or(Value::Null)))
}

fn body(report: Value, shot: Option<&[u8]>) -> Vec<u8> {
    let mut b = json!({ "game": "before-colony", "report": report });
    if let Some(png) = shot {
        b["shot_png_b64"] = base64::engine::general_purpose::STANDARD.encode(png).into();
    }
    serde_json::to_vec(&b).unwrap()
}

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|d| d.flatten().map(|e| e.file_name().into_string().unwrap()).collect())
        .unwrap_or_default();
    names.sort();
    names
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bc-bugs-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_report_lands_under_the_data_dir() -> anyhow::Result<()> {
    let data = scratch("lands");
    let server = start(Some(data.clone())).await?;
    let base = format!("http://{}", server.http_addr);
    let png = [&b"\x89PNG\r\n\x1a\n"[..], &[7u8; 100][..]].concat();

    let (status, got) = post(&base, body(report(), Some(&png)), None).await?;
    assert_eq!(status, 200, "{got}");
    assert_eq!((got["seen"].as_u64(), got["shot"].as_bool()), (Some(1), Some(true)), "{got}");
    let id = got["id"].as_str().unwrap();
    let fp = got["fingerprint"].as_str().unwrap();
    assert_eq!(fp.len(), 16, "{got}");
    let reports = data.join("reports");
    assert_eq!(files(&reports), [format!("bc-report-{id}.json"), format!("bc-report-{id}.png")]);
    let kept: Value = serde_json::from_slice(&std::fs::read(reports.join(format!("bc-report-{id}.json")))?)?;
    assert_eq!(kept["fingerprint"], fp);
    assert_eq!(kept["place"]["town_feet"], "1.0,0.0,-200.0");
    assert_eq!(kept["intake"]["shot"], format!("bc-report-{id}.png"));
    assert_eq!(std::fs::read(reports.join(format!("bc-report-{id}.png")))?, png);
    // The same bug again: one more of it.
    let (_, again) = post(&base, body(report(), None), None).await?;
    assert_eq!((again["seen"].as_u64(), again["fingerprint"].as_str()), (Some(2), Some(fp)));

    // A page on the store's site may ask first, and read the answer.
    let pre = reqwest::Client::new()
        .request(reqwest::Method::OPTIONS, format!("{base}/bugs"))
        .header("Origin", "https://elopros.com")
        .header("Access-Control-Request-Method", "POST")
        .send()
        .await?;
    assert_eq!(pre.status().as_u16(), 204);
    assert_eq!(pre.headers()["access-control-allow-origin"], "https://elopros.com");

    // What isn't a report is refused, with the reason.
    let (status, why) =
        post(&base, b"{\"game\":\"before-colony\",\"report\":{\"format\":2}}".to_vec(), None).await?;
    assert_eq!(status, 400, "{why}");
    assert!(why["error"].as_str().is_some_and(|e| e.contains("format")), "{why}");
    let (status, _) = post(&base, b"not json".to_vec(), None).await?;
    assert_eq!(status, 400);
    server.shutdown();
    let _ = std::fs::remove_dir_all(&data);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_address_gets_its_share() -> anyhow::Result<()> {
    let data = scratch("share");
    let server = start(Some(data.clone())).await?;
    let base = format!("http://{}", server.http_addr);
    let share = bc_server::bugs::Limits::default().per_address;
    // Behind nginx every peer is loopback: its X-Real-IP is who's counted.
    for _ in 0..share {
        let (status, got) = post(&base, body(report(), None), Some("198.51.100.7")).await?;
        assert_eq!(status, 200, "{got}");
    }
    let (status, got) = post(&base, body(report(), None), Some("198.51.100.7")).await?;
    assert_eq!(status, 429, "past its share: {got}");
    let (status, _) = post(&base, body(report(), None), Some("198.51.100.8")).await?;
    assert_eq!(status, 200, "another address still may");
    // A refusal for its shape costs no share.
    let (status, _) = post(&base, b"[]".to_vec(), Some("198.51.100.9")).await?;
    assert_eq!(status, 400);
    assert_eq!(files(&data.join("reports")).len(), share as usize + 1);
    server.shutdown();
    let _ = std::fs::remove_dir_all(&data);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_body_over_the_cap_is_refused() -> anyhow::Result<()> {
    let data = scratch("cap");
    let server = start(Some(data.clone())).await?;
    let base = format!("http://{}", server.http_addr);
    let mut big = report();
    big["words"] = "x".repeat(bc_server::bugs::BODY_MAX + 1).into();
    let (status, _) = post(&base, body(big, None), None).await?;
    assert_eq!(status, 413);
    // Under the body's cap but over the report's.
    let mut padded = report();
    padded["place"]["junk"] = "y".repeat(bc_server::bugs::REPORT_MAX).into();
    let (status, why) = post(&base, body(padded, None), None).await?;
    assert_eq!(status, 400, "{why}");
    // A screenshot that isn't a PNG.
    let (status, _) = post(&base, body(report(), Some(b"GIF89a")), None).await?;
    assert_eq!(status, 400);
    assert!(files(&data.join("reports")).is_empty());
    server.shutdown();
    let _ = std::fs::remove_dir_all(&data);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_secret_is_written() -> anyhow::Result<()> {
    let data = scratch("secrets");
    let server = start(Some(data.clone())).await?;
    let base = format!("http://{}", server.http_addr);
    let mut r = report();
    r["words"] = "legal winner thank year wave sausage worth useful legal winner thank yellow".into();
    r["resume_token"] = true.into();
    r["net"]["resume"] = KEY.into();
    r["net"]["signature"] = format!("0x{KEY}{KEY}").into();
    r["net"]["last_error"] = format!("signed in as {WALLET} with {KEY}").into();
    r["machine"] = json!({ "ua": "Mozilla/5.0", "pass": KEY });
    let (status, got) = post(&base, body(r, None), None).await?;
    assert_eq!(status, 200, "{got}");
    let reports = data.join("reports");
    let name = files(&reports).into_iter().find(|n| n.ends_with(".json")).unwrap();
    let text = std::fs::read_to_string(reports.join(name))?;
    for secret in [KEY, WALLET, "legal winner", "resume", "signature"] {
        assert!(!text.contains(secret), "{secret} written: {text}");
    }
    let kept: Value = serde_json::from_str(&text)?;
    assert_eq!(kept["words"], bc_server::bugs::SECRET_REMOVED);
    assert_eq!(kept["net"]["last_error"], "signed in as [wallet] with [removed]");
    server.shutdown();
    let _ = std::fs::remove_dir_all(&data);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_data_dir_there_is_no_route() -> anyhow::Result<()> {
    let server = start(None).await?;
    let base = format!("http://{}", server.http_addr);
    let (status, _) = post(&base, body(report(), None), None).await?;
    assert_eq!(status, 404);
    server.shutdown();
    Ok(())
}
