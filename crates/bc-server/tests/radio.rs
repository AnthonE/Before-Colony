//! The colony's radio over real WebTransport: two agents flying under the arcade rules (the radio
//! is everyone's, whatever the rules); what one says the other hears, by its callsign, cleaned and
//! cut to a line; a pilot who talks too fast is told to wait; `/status` counts the lines and never
//! says them.

use std::time::{Duration, Instant};

use bc_bot::{BotClient, BotConfig};
use bc_econ::wire::SAY_MAX_CHARS;
use bc_proto::{Faction, FrameId};
use bc_server::{Config, Mode, Ruleset};

async fn pilot(http: &str, name: &str) -> anyhow::Result<BotClient> {
    let cfg =
        BotConfig { server: http.into(), name: name.into(), frame: FrameId::Leo, faction: Faction::Colonies };
    BotClient::connect(&cfg).await
}

/// Steps both until `done` holds of them, for at most `secs`.
async fn until(
    a: &mut BotClient,
    b: &mut BotClient,
    secs: f64,
    done: impl Fn(&BotClient, &BotClient) -> bool,
) -> anyhow::Result<bool> {
    let end = Instant::now() + Duration::from_secs_f64(secs);
    while Instant::now() < end {
        if done(a, b) {
            return Ok(true);
        }
        a.step(&mut |_| Default::default()).await?;
        b.step(&mut |_| Default::default()).await?;
    }
    Ok(done(a, b))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn what_one_pilot_says_the_others_hear() -> anyhow::Result<()> {
    let cfg = Config {
        mode: Mode::Game,
        rules: Ruleset::Arcade,
        wt_port: 0,
        http_addr: "127.0.0.1:0".parse()?,
        mobile_dolls: 0,
        max_clients: 8,
        ..Config::default()
    };
    let server = bc_server::start(cfg).await?;
    let http = format!("http://{}", server.http_addr);
    let mut heero = pilot(&http, "Heero").await?;
    let mut duo = pilot(&http, "Duo").await?;
    until(&mut heero, &mut duo, 3.0, |a, b| a.world().own.is_some() && b.world().own.is_some()).await?;

    heero.say("o7 Duo").await?;
    let heard =
        until(&mut heero, &mut duo, 3.0, |a, b| !a.heard().is_empty() && !b.heard().is_empty()).await?;
    assert!(heard, "nobody heard it");
    assert_eq!(duo.heard()[0], ("Heero".to_string(), "o7 Duo".to_string()));
    assert_eq!(heero.heard()[0], duo.heard()[0], "the speaker hears it too");

    // Cleaned: no control characters, one line, cut to length.
    heero.say(&format!("\u{1b}[2J  mission\n\taccepted {}", "x".repeat(300))).await?;
    until(&mut heero, &mut duo, 3.0, |_, b| b.heard().len() >= 2).await?;
    let (_, text) = &duo.heard()[1];
    assert!(text.starts_with("[2J mission accepted x"), "{text:?}");
    assert_eq!(text.chars().count(), SAY_MAX_CHARS);
    assert!(!text.chars().any(char::is_control));

    // Five lines in ten seconds, then the radio's busy.
    for k in 0..5 {
        heero.say(&format!("line {k}")).await?;
    }
    until(&mut heero, &mut duo, 3.0, |a, b| {
        b.heard().len() >= 5 && a.core.hangar.notes.iter().filter(|(_, ok)| !ok).count() >= 2
    })
    .await?;
    // A moment more, for anything that shouldn't have got through.
    until(&mut heero, &mut duo, 0.5, |_, _| false).await?;
    let said: Vec<&str> = duo.heard().iter().map(|(_, t)| t.as_str()).collect();
    assert_eq!(said[2..], ["line 0", "line 1", "line 2"], "{said:?}");
    let refused = heero.core.hangar.notes.iter().filter(|(t, ok)| !ok && t.contains("busy")).count();
    assert_eq!(refused, 2, "{:?}", heero.core.hangar.notes);

    // Duo answers.
    duo.say("roger").await?;
    until(&mut heero, &mut duo, 3.0, |a, _| a.heard().iter().any(|(f, _)| f == "Duo")).await?;
    assert_eq!(heero.heard().last().unwrap(), &("Duo".to_string(), "roger".to_string()));

    // Counted, never said.
    let status = server.status();
    assert_eq!(status["game"]["radio_lines"], 6, "{}", status["game"]["radio_lines"]);
    assert!(!status.to_string().contains("o7 Duo"));
    heero.close().await;
    duo.close().await;
    server.shutdown();
    Ok(())
}
