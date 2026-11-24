//! Proximity voice over real WebTransport (`bc_server::voice`): the server relays a talker's frames
//! to the people near on foot and to nobody else (zero bytes past the reach, not quieter ones), never
//! to an agent, within the talker's own bucket, counting what it refuses; and in space over the
//! suits' radio, named by the suit, to suits in range and not to the bay, the town or a wreck.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use bc_bot::{BotClient, BotConfig};
use bc_proto::presence::{PersonPose, RING_DECK};
use bc_proto::voice::{VOICE_MAX_BYTES, Via, VoiceFrame};
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};
use bc_sim::colony::town;

fn config() -> anyhow::Result<Config> {
    Ok(Config {
        mode: Mode::Game,
        rules: Ruleset::Survival,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        drones: 0,
        max_clients: 8,
        ..Config::default()
    })
}

fn bot(http: &str, name: &str) -> BotConfig {
    BotConfig { server: http.into(), name: name.into(), frame: FrameId::Argo, faction: Faction::Colonies }
}

/// A person (or with `agent`, an agent) in their bay, then down in the town.
async fn down(http: &str, name: &str, agent: bool) -> anyhow::Result<BotClient> {
    let cfg = bot(http, name);
    let mut b = if agent { BotClient::connect(&cfg).await? } else { BotClient::connect_person(&cfg).await? };
    b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    b.enter_town().await?;
    Ok(b)
}

/// Standing in the town `dx` m across and `ds` m round from the step out of Hub Gate's lift.
fn standing(dx: f32, ds: f32) -> PersonPose {
    let (x, t) = town::out_of_the_lift(0);
    PersonPose {
        strip: RING_DECK,
        x: x + dx,
        s: town::deck_s(t) + ds,
        grounded: true,
        ..PersonPose::default()
    }
}

/// An Opus packet's shape: a TOC byte saying one 20 ms frame (SILK, code 0), then bytes.
fn packet(k: u16, len: usize) -> Vec<u8> {
    let mut p = vec![0x08u8];
    p.extend((1..len).map(|i| (k as usize * 31 + i) as u8));
    p
}

/// Steps every bot once, together (each waits up to 10 ms for what's come).
async fn step_all(bots: &mut [&mut BotClient]) -> anyhow::Result<()> {
    let steps = bots.iter_mut().map(|b| async move { b.step(&mut |_| Default::default()).await });
    for r in futures::future::join_all(steps).await {
        r?;
    }
    Ok(())
}

async fn run(bots: &mut [&mut BotClient], secs: f64) -> anyhow::Result<()> {
    let end = Instant::now() + Duration::from_secs_f64(secs);
    while Instant::now() < end {
        step_all(bots).await?;
    }
    Ok(())
}

/// `talker` says `frames` frames (seqs from `seq`), one each 20 ms, the last its END, with everyone
/// stepping meanwhile; then a little longer for the last to arrive.
async fn talk(
    talker: &mut BotClient,
    others: &mut [&mut BotClient],
    seq: u16,
    frames: u16,
) -> anyhow::Result<()> {
    let start = Instant::now();
    let mut k = 0;
    while k < frames {
        while k < frames && start.elapsed() >= Duration::from_millis(20 * u64::from(k)) {
            assert!(talker.send_voice(seq.wrapping_add(k), k + 1 == frames, &packet(k, 60)));
            k += 1;
        }
        talker.step(&mut |_| Default::default()).await?;
        step_all(others).await?;
    }
    let end = Instant::now() + Duration::from_millis(300);
    while Instant::now() < end {
        talker.step(&mut |_| Default::default()).await?;
        step_all(others).await?;
    }
    Ok(())
}

fn voice(status: &serde_json::Value, what: &str) -> u64 {
    status["game"]["voice"][what].as_u64().unwrap_or_else(|| panic!("voice.{what}: {status}"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn on_foot_a_voice_carries_thirty_metres_to_people_and_no_further() -> anyhow::Result<()> {
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    // Maren talks (an agent may talk); Pagan stands 10 m off, Ines 60 m, and an agent 4 m (spots
    // the town's own test walks and drives).
    let mut maren = down(&http, "Maren", true).await?;
    let mut pagan = down(&http, "Pagan", false).await?;
    let mut ines = down(&http, "Ines", false).await?;
    let mut agent = down(&http, "Unit", true).await?;
    maren.set_pose(standing(0.0, 0.0));
    pagan.set_pose(standing(-10.0, 0.0));
    ines.set_pose(standing(-60.0, 4.0));
    agent.set_pose(standing(-3.0, 2.0));
    run(&mut [&mut maren, &mut pagan, &mut ines, &mut agent], 1.5).await?;
    let seen: Vec<String> = maren.people().into_iter().map(|p| p.1).collect();
    assert_eq!(seen.len(), 3, "everyone's poses were taken: {seen:?}");
    let maren_id = pagan.people().into_iter().find(|p| p.1 == "Maren").expect("Pagan sees Maren").0;
    for b in [&mut maren, &mut pagan, &mut ines, &mut agent] {
        b.take_voice();
    }

    // A second of talk, paced: Pagan hears (almost) all of it, from Maren, through the air, its last
    // frame the END; Ines and the agent hear nothing at all, nor does Maren hear herself.
    talk(&mut maren, &mut [&mut pagan, &mut ines, &mut agent], 100, 50).await?;
    let heard = pagan.take_voice();
    let seqs: BTreeSet<u16> = heard.iter().map(|f| f.seq).collect();
    assert!(seqs.len() >= 45, "{} of 50", seqs.len());
    assert!(seqs.iter().all(|s| (100..150).contains(s)), "{seqs:?}");
    assert!(heard.iter().all(|f| f.talker == maren_id && f.via == Via::Air), "{heard:?}");
    assert!(heard.iter().all(|f| f.payload() == &packet(f.seq - 100, 60)[..]), "relayed as sent");
    assert!(heard.iter().any(|f| f.end && f.seq == 149) || !seqs.contains(&149));
    assert!(ines.take_voice().is_empty(), "60 m off: not a byte");
    assert!(agent.take_voice().is_empty(), "agents aren't sent people's voices");
    assert!(maren.take_voice().is_empty(), "nobody hears themselves");
    // And Pagan still sees the people round him meanwhile.
    assert_eq!(pagan.people().len(), 3);

    // A hundred at once: no more than the bucket holds get through.
    let before = server.status();
    for k in 0..100u16 {
        maren.send_voice(1_000 + k, false, &packet(k, 40));
    }
    run(&mut [&mut maren, &mut pagan, &mut ines, &mut agent], 0.4).await?;
    let burst = pagan.take_voice().len();
    assert!((8..=20).contains(&burst), "{burst} of a burst of 100");
    let after = server.status();
    assert!(voice(&after, "limited") >= voice(&before, "limited") + 70, "{after}");

    // What isn't one 20 ms frame (a 60 ms one; two frames; too long; nothing) is refused, counted.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let bad = voice(&after, "bad");
    assert!(maren.send_voice(2_000, false, &[0x18, 1, 2, 3]));
    assert!(maren.send_voice(2_001, false, &[0x09, 1, 2, 3]));
    assert!(!maren.send_voice(2_002, false, &[0x08; VOICE_MAX_BYTES + 1]), "not even sent");
    run(&mut [&mut maren, &mut pagan], 0.3).await?;
    assert!(pagan.take_voice().is_empty());
    let status = server.status();
    assert_eq!(voice(&status, "bad"), bad + 2, "{status}");
    assert!(voice(&status, "relayed") >= 45);
    assert_eq!(status["game"]["hot_path_allocations"], 0);

    // Ines walks up beside them (the road the town's test drives): she hears now.
    for k in 1..=10 {
        ines.set_pose(standing(-60.0 + 5.0 * k as f32, 4.0));
        run(&mut [&mut maren, &mut pagan, &mut ines, &mut agent], 0.45).await?;
    }
    ines.take_voice();
    talk(&mut maren, &mut [&mut pagan, &mut ines], 300, 10).await?;
    assert!(ines.take_voice().len() >= 8, "beside her now");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_kill_switch_drops_every_frame() -> anyhow::Result<()> {
    let server = bc_server::start(Config { voice: false, ..config()? }).await?;
    let http = format!("http://{}", server.http_addr);
    let mut a = down(&http, "Maren", false).await?;
    let mut b = down(&http, "Pagan", false).await?;
    a.set_pose(standing(0.0, 0.0));
    b.set_pose(standing(-3.0, 2.0));
    run(&mut [&mut a, &mut b], 1.0).await?;
    talk(&mut a, &mut [&mut b], 0, 10).await?;
    assert!(b.take_voice().is_empty());
    let status = server.status();
    assert_eq!(voice(&status, "off"), 10, "{status}");
    assert_eq!(voice(&status, "relayed"), 0);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn in_space_the_radio_reaches_suits_in_range_named_by_the_suit() -> anyhow::Result<()> {
    let server = bc_server::start(config()?).await?;
    let http = format!("http://{}", server.http_addr);
    let cfg = |n: &str| bot(&http, n);
    // Two pilots launch; one stays in the bay; one is down in the town.
    let mut kai = BotClient::connect(&cfg("Kai")).await?;
    let mut nico = BotClient::connect_person(&cfg("Nico")).await?;
    let mut bay = BotClient::connect_person(&cfg("Bay")).await?;
    for b in [&mut kai, &mut nico, &mut bay] {
        b.wait_until(5.0, "the hangar", |c| c.hangar.in_hangar() && c.hangar.view.is_some()).await?;
    }
    let mut street = down(&http, "Street", false).await?;
    street.set_pose(standing(0.0, 0.0));
    kai.launch().await?;
    nico.launch().await?;
    run(&mut [&mut kai, &mut nico, &mut bay, &mut street], 1.0).await?;
    let kai_suit = kai.world().own_slot().expect("Kai flies");
    for b in [&mut kai, &mut nico, &mut bay, &mut street] {
        b.take_voice();
    }

    talk(&mut kai, &mut [&mut nico, &mut bay, &mut street], 7, 25).await?;
    let heard = nico.take_voice();
    assert!(heard.len() >= 22, "{} of 25", heard.len());
    assert!(heard.iter().all(|f| f.via == Via::Radio && f.talker == kai_suit), "{heard:?}");
    assert!(bay.take_voice().is_empty(), "the bay hears no radio");
    assert!(street.take_voice().is_empty(), "nor does the street");

    // And the other way: the street's voice never reaches a suit.
    talk(&mut street, &mut [&mut kai, &mut nico], 0, 10).await?;
    assert!(kai.take_voice().is_empty() && nico.take_voice().is_empty());

    // Ejected, Kai's radio is dead.
    kai.eject().await?;
    run(&mut [&mut kai, &mut nico], 0.5).await?;
    nico.take_voice();
    let nowhere = voice(&server.status(), "nowhere");
    talk(&mut kai, &mut [&mut nico], 50, 10).await?;
    assert!(nico.take_voice().is_empty(), "nothing from a wreck");
    assert!(voice(&server.status(), "nowhere") >= nowhere + 10);
    assert_eq!(server.status()["game"]["hot_path_allocations"], 0);
    Ok(())
}

#[test]
fn a_frame_carries_no_more_than_one_datagram() {
    let f = VoiceFrame::new(1, true, &[0x08; VOICE_MAX_BYTES]).expect("a full frame");
    let mut buf = [0u8; bc_proto::MAX_DATAGRAM];
    assert!(f.relayed(3, Via::Radio).encode_down(&mut buf).is_some_and(|n| n <= bc_proto::MAX_DATAGRAM));
}
