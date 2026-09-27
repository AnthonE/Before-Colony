//! Sleeping in the cockpit over real WebTransport. A signed-in pilot who leaves stays in the
//! sector, asleep (listed on `/status`, flagged in everyone's roster), and wakes in the same suit
//! on the way back; a guest's suit goes with them; a sleeper cleared to make room is news when its
//! pilot returns.

use std::time::Duration;

use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_client_core::Identity;
use bc_proto::control::notice;
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

fn config(max_sleepers: usize) -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 0,
        max_clients: 8,
        max_sleepers,
        ..Config::default()
    })
}

async fn idle(b: &mut BotClient, secs: f64) -> anyhow::Result<()> {
    b.run_for(Duration::from_secs_f64(secs), &mut |_| bc_proto::InputCmd::default()).await
}

/// Idles `b` until `ok` holds (for up to 5 s).
async fn idle_until(b: &mut BotClient, what: &str, ok: impl Fn(&BotClient) -> bool) -> anyhow::Result<()> {
    for _ in 0..100 {
        if ok(b) {
            return Ok(());
        }
        idle(b, 0.05).await?;
    }
    anyhow::bail!("timed out waiting for {what}")
}

/// The names `/status` lists as asleep.
fn sleepers(server: &bc_server::ServerHandle) -> Vec<String> {
    let status = server.status();
    let list = status["game"]["sleepers"].as_array().cloned().unwrap_or_default();
    list.iter().filter_map(|s| s["name"].as_str().map(str::to_string)).collect()
}

fn flying(server: &bc_server::ServerHandle) -> Vec<String> {
    let status = server.status();
    let list = status["game"]["pilots"].as_array().cloned().unwrap_or_default();
    list.iter().filter_map(|s| s["name"].as_str().map(str::to_string)).collect()
}

async fn until(what: &str, mut ok: impl FnMut() -> bool) -> anyhow::Result<()> {
    for _ in 0..200 {
        if ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    anyhow::bail!("timed out waiting for {what}")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_signed_in_pilot_sleeps_and_wakes_in_the_same_suit() -> anyhow::Result<()> {
    let server = bc_server::start(config(256)?).await?;
    let http = format!("http://{}", server.http_addr);
    let w = wallet(9);

    let mut pilot = BotClient::connect_as(&bot(&http, "Sleeper-1"), Some(&w)).await?;
    idle(&mut pilot, 0.6).await?;
    let own = pilot.core.world.own.expect("flying");
    let token = pilot.core.resume_token.expect("a signed-in pilot gets a resume token");
    pilot.close().await;
    let me = "Sleeper-1".to_string();
    until("the sleeper", || sleepers(&server).contains(&me) && !flying(&server).contains(&me)).await?;

    // Someone else sees it there, flagged asleep.
    let mut guest = BotClient::connect(&bot(&http, "Guest-2")).await?;
    idle_until(&mut guest, "the sleeper in view", |g| {
        bc_client_core::asleep(&g.core.world, own.slot) && g.core.world.entity(own.slot).is_some()
    })
    .await?;

    // Back on the token: the same suit, where it was left, and awake.
    let mut back = BotClient::connect_with(
        &bot(&http, "Sleeper-1"),
        Identity::Wallet { address: w.address(), resume: Some(token) },
        None,
    )
    .await?;
    assert!(back.core.welcome.is_some_and(|w| w.woke), "it should wake in its suit");
    idle_until(&mut back, "its own suit", |b| b.core.world.own.is_some()).await?;
    let again = back.core.world.own.expect("flying again");
    assert_eq!((again.slot, again.generation), (own.slot, own.generation));
    assert!(again.pos.distance(own.pos) < 5.0, "{} vs {}", again.pos, own.pos);
    assert!(sleepers(&server).is_empty());
    idle_until(&mut guest, "the roster to say awake", |g| !bc_client_core::asleep(&g.core.world, own.slot))
        .await?;

    // A guest's suit goes with them.
    guest.close().await;
    until("the guest gone", || !flying(&server).contains(&"Guest-2".to_string())).await?;
    assert!(!sleepers(&server).contains(&"Guest-2".to_string()));

    back.close().await;
    server.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_sleeper_cleared_for_room_is_news_on_return() -> anyhow::Result<()> {
    // Room for one sleeper: the second to fall asleep clears the first.
    let server = bc_server::start(config(1)?).await?;
    let http = format!("http://{}", server.http_addr);
    let (wa, wb) = (wallet(11), wallet(12));

    let mut a = BotClient::connect_as(&bot(&http, "First"), Some(&wa)).await?;
    let mut b = BotClient::connect_as(&bot(&http, "Second"), Some(&wb)).await?;
    idle(&mut a, 0.4).await?;
    idle(&mut b, 0.2).await?;
    let token = a.core.resume_token.expect("a token");
    a.close().await;
    until("the first asleep", || sleepers(&server) == ["First"]).await?;
    b.close().await;
    until("only the second asleep", || sleepers(&server) == ["Second"]).await?;

    let mut back = BotClient::connect_with(
        &bot(&http, "First"),
        Identity::Wallet { address: wa.address(), resume: Some(token) },
        None,
    )
    .await?;
    assert!(!back.core.welcome.is_some_and(|w| w.woke), "its suit is gone");
    idle_until(&mut back, "the news", |b| !b.core.notices.is_empty()).await?;
    assert_eq!(back.core.notices.first().map(|n| n.0), Some(notice::SLEEPER_LOST));

    back.close().await;
    server.shutdown();
    Ok(())
}
