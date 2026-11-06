//! Wallet sign-in over real WebTransport: a signed-in agent is verified, a guest isn't, a resume
//! token reconnects without signing, and signing in again elsewhere takes the pilot over.

use std::time::Duration;

use bc_auth::LocalWallet;
use bc_bot::{BotClient, BotConfig};
use bc_client_core::{Identity, Phase};
use bc_proto::control::bye;
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};

fn wallet(k: u8) -> LocalWallet {
    let mut secret = [0u8; 32];
    secret[31] = k;
    LocalWallet::from_secret(&secret).expect("a valid key")
}

fn bot(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Argo, faction: Faction::Colonies }
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
        rules: Ruleset::Arcade,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 0,
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

// --- A Solana wallet and an Elo Pros pass, over the same real session. ---

mod elo {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Router;
    use axum::routing::get;
    use base64::Engine;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    use super::*;
    use bc_proto::auth::{PlayerId, SolanaSignature};

    /// Elo Pros' test key (`tests/testdata/account_token.json`, its `seed_hex`).
    const SEED: &str = "2f3e65db2d7c08d3d45cb88040eca8badb2b6287066b2d30a1861756938b6536";
    const ACCT: &str = "acct_0123456789abcdef0123";
    /// `signin_check.json`'s throwaway wallet, Ed25519 seed 07…07.
    const SOL: &str = "GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB";

    fn account_key() -> Ed25519KeyPair {
        Ed25519KeyPair::from_seed_unchecked(&hex::decode(SEED).unwrap()).unwrap()
    }

    fn mint(wallet: &str, game: &str, shard: &str, nonce: &str) -> String {
        let text = format!(
            "elo account\nid: {ACCT}\nfor: play:{game}\nshard: {shard}\nnonce: {nonce}\nwallet: {wallet}\nexp: {}",
            bc_server::pilots::unix_now() + 600
        );
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        format!("{}.{}", b64.encode(&text), b64.encode(account_key().sign(text.as_bytes())))
    }

    /// A stand-in for `GET /api/items/sol/of/<wallet>`: `SOL` holds the Pill, everyone else nothing.
    async fn items_origin(asked: Arc<AtomicUsize>) -> String {
        let app = Router::new().route(
            "/api/items/sol/of/{wallet}",
            get(move |axum::extract::Path(wallet): axum::extract::Path<String>| {
                asked.fetch_add(1, Ordering::SeqCst);
                async move {
                    let items = if wallet == SOL {
                        serde_json::json!([{"title": "before-colony", "item": "pill", "name": "The Pill"}])
                    } else {
                        serde_json::json!([])
                    };
                    axum::Json(serde_json::json!({"reachable": true, "wallet": wallet, "items": items}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://{addr}")
    }

    /// What everyone is told of the suit this pilot flies.
    fn shows_pill(b: &BotClient) -> bool {
        let w = b.world();
        w.own.is_some_and(|o| bc_client_core::has_pill(w, o.slot))
    }

    async fn nonce(http: &str) -> anyhow::Result<String> {
        let doc: serde_json::Value = reqwest::get(format!("{http}/nonce")).await?.json().await?;
        assert_eq!(doc["game"], "before-colony");
        Ok(doc["nonce"].as_str().expect("this server takes passes").to_string())
    }

    async fn online(http: &str) -> anyhow::Result<Vec<String>> {
        let doc: serde_json::Value = reqwest::get(format!("{http}/online.json")).await?.json().await?;
        Ok(doc["online"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["wallet"].as_str().unwrap().to_string())
            .collect())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_solana_wallet_and_a_pass_sign_in_and_the_pill_is_read() -> anyhow::Result<()> {
        let asked = Arc::new(AtomicUsize::new(0));
        let cfg = Config {
            mode: Mode::Game,
            rules: Ruleset::Arcade,
            wt_port: 0,
            http_addr: "127.0.0.1:0".parse()?,
            drones: 0,
            max_clients: 8,
            siwe_domains: vec!["elopros.com".into()],
            elo_pubkey: Some(account_key().public_key().as_ref().try_into().unwrap()),
            elo_origin: Some(items_origin(asked.clone()).await),
            ..Config::default()
        };
        let server = bc_server::start(cfg).await?;
        let http = format!("http://{}", server.http_addr);

        // A Solana wallet: its address exact on the roster, the Pill on its suit.
        let pair = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let key: [u8; 32] = pair.public_key().as_ref().try_into().unwrap();
        assert_eq!(PlayerId::solana(&key).as_str(), SOL);
        let sign = |text: &[u8]| {
            let mut s = [0u8; 64];
            s.copy_from_slice(pair.sign(text).as_ref());
            SolanaSignature(s)
        };
        let mut sol = BotClient::connect_solana(&bot(&http, "Sol"), key, None, &sign).await?;
        assert!(sol.core.welcome.is_some_and(|w| w.signed_in));
        idle(&mut sol, 0.5).await?;
        assert_eq!(asked.load(Ordering::SeqCst), 1, "read once as it signed in");
        assert_eq!(online(&http).await?, [SOL]);
        assert!(shows_pill(&sol), "the server's word, on the roster");

        // Someone else's signature for that wallet.
        let other = Ed25519KeyPair::from_seed_unchecked(&[8; 32]).unwrap();
        let forged = |text: &[u8]| {
            let mut s = [0u8; 64];
            s.copy_from_slice(other.sign(text).as_ref());
            SolanaSignature(s)
        };
        assert!(BotClient::connect_solana(&bot(&http, "Forger"), key, None, &forged).await.is_err());

        // Back on the token, no signature asked; and the Pill's read again.
        idle(&mut sol, 0.2).await?;
        let token = sol.core.resume_token.expect("a resume token");
        sol.close().await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let refuse = |_: &[u8]| -> SolanaSignature { panic!("a token doesn't sign") };
        let mut sol = BotClient::connect_solana(&bot(&http, "Sol"), key, Some(token), &refuse).await?;
        assert!(sol.core.welcome.is_some_and(|w| w.signed_in));
        idle(&mut sol, 0.5).await?;
        assert!(shows_pill(&sol), "waking in the suit it left");

        // A pass with no wallet plays as its account, in a slot of its own, and shows nothing.
        let n = nonce(&http).await?;
        let mut acct =
            BotClient::connect_pass(&bot(&http, "Acct"), &mint("none", "before-colony", "elopros.com", &n))
                .await?;
        assert!(acct.core.welcome.is_some_and(|w| w.signed_in));
        idle(&mut acct, 0.5).await?;
        assert!(online(&http).await?.contains(&ACCT.to_string()));
        assert_eq!(asked.load(Ordering::SeqCst), 2, "an account has no wallet to read");
        assert!(!shows_pill(&acct));
        // Used, the nonce is gone, even on a pass for it that's otherwise good.
        let again = mint("none", "before-colony", "elopros.com", &n);
        assert!(BotClient::connect_pass(&bot(&http, "Acct2"), &again).await.is_err());
        assert!(acct.core.resume_token.is_some());

        // A pass that names a Solana wallet flies as that wallet, kept as written (taking its seat).
        let n = nonce(&http).await?;
        let named = mint(SOL, "before-colony", "elopros.com", &n);
        let mut named = BotClient::connect_pass(&bot(&http, "Named"), &named).await?;
        idle(&mut named, 0.5).await?;
        assert!(online(&http).await?.contains(&SOL.to_string()));
        assert!(shows_pill(&named));

        // Passes that were never for here.
        for (game, shard) in [("gates", "elopros.com"), ("before-colony", "evil.example")] {
            let n = nonce(&http).await?;
            assert!(
                BotClient::connect_pass(&bot(&http, "Elsewhere"), &mint("none", game, shard, &n))
                    .await
                    .is_err()
            );
        }
        let made_up = "0123456789abcdef".repeat(4);
        let made_up = mint("none", "before-colony", "elopros.com", &made_up);
        assert!(BotClient::connect_pass(&bot(&http, "NoNonce"), &made_up).await.is_err());
        assert!(BotClient::connect_pass(&bot(&http, "Garbage"), "not a pass").await.is_err());
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn with_no_account_key_no_pass_is_taken() -> anyhow::Result<()> {
        let cfg = Config {
            mode: Mode::Game,
            rules: Ruleset::Arcade,
            wt_port: 0,
            http_addr: "127.0.0.1:0".parse()?,
            drones: 0,
            max_clients: 4,
            ..Config::default()
        };
        let server = bc_server::start(cfg).await?;
        let http = format!("http://{}", server.http_addr);
        let doc: serde_json::Value = reqwest::get(format!("{http}/nonce")).await?.json().await?;
        assert!(doc["nonce"].is_null());
        let pass = mint("none", "before-colony", &server.http_addr.ip().to_string(), &"0".repeat(64));
        assert!(BotClient::connect_pass(&bot(&http, "Acct"), &pass).await.is_err());
        // A wallet still signs in.
        assert!(BotClient::connect_as(&bot(&http, "W"), Some(&wallet(3))).await.is_ok());
        Ok(())
    }
}
