//! Wallet sign-in over real WebTransport: a signed-in agent is verified, a guest isn't, a resume
//! token reconnects without signing, and signing in again elsewhere takes the pilot over.

use std::time::Duration;

use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_client_core::{Identity, Phase};
use bc_proto::control::bye;
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode};

fn wallet(k: u8) -> LocalWallet {
    let mut secret = [0u8; 32];
    secret[31] = k;
    LocalWallet::from_secret(&secret).expect("a valid key")
}

fn bot(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Leo, faction: Faction::Colonies }
}

fn pilot<'a>(status: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    status["game"]["pilots"].as_array()?.iter().find(|p| p["name"] == name)
}

async fn idle(b: &mut BotClient, secs: f64) -> anyhow::Result<()> {
    b.run_for(Duration::from_secs_f64(secs), &mut |_| bc_proto::InputCmd::default()).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wallets_sign_in_resume_and_take_over() -> anyhow::Result<()> {
    let cfg = Config {
        mode: Mode::Game,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 0,
        max_clients: 8,
        ..Config::default()
    };
    let server = bc_server::start(cfg).await?;
    let http = format!("http://{}", server.http_addr);
    let w = wallet(7);

    // A guest and a signed-in pilot, side by side.
    let mut guest = BotClient::connect(&bot(&http, "Guest-1")).await?;
    let mut signed = BotClient::connect_as(&bot(&http, "Wallet-1"), Some(&w)).await?;
    assert!(signed.core.welcome.is_some_and(|w| w.signed_in));
    assert!(!guest.core.welcome.is_some_and(|w| w.signed_in));
    idle(&mut signed, 0.5).await?;
    idle(&mut guest, 0.2).await?;
    let token = signed.core.resume_token.expect("a signed-in pilot gets a resume token");
    let status = server.status();
    assert_eq!(pilot(&status, "Wallet-1").map(|p| p["verified"].clone()), Some(serde_json::json!(true)));
    assert_eq!(pilot(&status, "Guest-1").map(|p| p["verified"].clone()), Some(serde_json::json!(false)));

    // Leaving, then coming back on the token: no signature asked.
    signed.close().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let back = BotClient::connect_with(
        &bot(&http, "Wallet-1"),
        Identity::Wallet { address: w.address(), resume: Some(token) },
        None,
    )
    .await?;
    assert!(back.core.welcome.is_some_and(|w| w.signed_in));
    // A token works once.
    let again = BotClient::connect_with(
        &bot(&http, "Wallet-1b"),
        Identity::Wallet { address: w.address(), resume: Some(token) },
        None,
    )
    .await;
    assert!(again.is_err(), "a spent token must not sign in");

    // Signing in again elsewhere takes the pilot over: the first session is told, and ends.
    let mut first = back;
    let second = BotClient::connect_as(&bot(&http, "Wallet-1c"), Some(&w)).await?;
    let mut ended = false;
    for _ in 0..300 {
        if first.step(&mut |_| bc_proto::InputCmd::default()).await.is_err() {
            ended = true;
            break;
        }
    }
    assert!(ended, "the older session should have been ended");
    assert_eq!(first.core.phase, Phase::Closed);
    assert_eq!(first.core.bye_reason, Some(bye::TAKEN_OVER));
    assert!(second.core.welcome.is_some_and(|w| w.signed_in));

    guest.close().await;
    second.close().await;
    server.shutdown();
    Ok(())
}
