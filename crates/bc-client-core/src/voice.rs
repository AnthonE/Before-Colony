//! Proximity voice on the client (`bc_proto::voice`; the server's `voice`): what the page's
//! `web/voice.js` is told and asked, worked out here where it's tested natively.
//!
//! - [`VoiceRing`]: relayed frames waiting to be played, the newest [`VOICE_RING`], the oldest
//!   dropped (and counted) so a stalled tab can't pile them up. The browser splits voice off at
//!   arrival into its own (`bc-client`'s transport); [`crate::ClientCore`] keeps one for clients that
//!   feed it every datagram (the bots).
//! - [`falloff`]: how loud a voice through the air is at a distance, full near and nothing at the
//!   server's reach (the cull is the server's; this is only how it sounds).
//! - [`TalkMode`] and [`Keyed`]: when the pilot is on the air.
//! - [`Mute`]: the `/mute` and `/unmute` lines, handled on the client and never sent.

use std::collections::VecDeque;

use bc_proto::voice::{Via, VoiceFrame};

/// Relayed frames kept waiting at most: 1.3 s of one talker's (a stalled tab's backlog is useless).
pub const VOICE_RING: usize = 64;

/// How far a voice carries through the air, m: the server's reach (`bc_server::voice::FOOT_M`).
pub const REACH_M: f32 = 30.0;
/// Full volume inside this, m.
pub const NEAR_M: f32 = 2.0;
/// A talker is "talking" this long after their last frame, s (their END ends it at once).
pub const TALKING_S: f64 = 0.35;

/// Relayed frames, oldest first, with how many were dropped for room.
#[derive(Debug, Default)]
pub struct VoiceRing {
    frames: VecDeque<VoiceFrame>,
    pub dropped: u64,
}

impl VoiceRing {
    pub fn push(&mut self, f: VoiceFrame) {
        if self.frames.len() >= VOICE_RING {
            self.frames.pop_front();
            self.dropped += 1;
        }
        self.frames.push_back(f);
    }

    /// Takes every frame waiting, oldest first.
    pub fn drain(&mut self) -> impl Iterator<Item = VoiceFrame> + '_ {
        self.frames.drain(..)
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

/// How loud a voice through the air is from `d` m away, 0..1: full inside [`NEAR_M`], nothing at
/// [`REACH_M`], cubed between (after Gates' `ears()`: plain across a square, a murmur at its edge).
pub fn falloff(d: f32) -> f32 {
    let g = (1.0 - (d - NEAR_M).max(0.0) / (REACH_M - NEAR_M)).clamp(0.0, 1.0);
    g * g * g
}

/// How the talk key works.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TalkMode {
    /// On the air while it's held.
    #[default]
    Hold,
    /// A press goes on the air, the next comes off.
    Toggle,
}

impl TalkMode {
    pub const ALL: [TalkMode; 2] = [TalkMode::Hold, TalkMode::Toggle];
    pub const NAMES: [&'static str; 2] = ["hold", "toggle"];

    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        Self::NAMES.iter().position(|n| *n == s).map(|i| Self::ALL[i])
    }
}

/// Whether the pilot is on the air, frame by frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Keyed {
    toggled: bool,
}

impl Keyed {
    /// This frame: `can` (voice on, somewhere a voice carries, no line or panel owning the keys),
    /// the talk key `held` and whether it was `pressed` this frame. A toggle drops when it can't.
    pub fn step(&mut self, mode: TalkMode, can: bool, held: bool, pressed: bool) -> bool {
        if mode != TalkMode::Toggle || !can {
            self.toggled = false;
        } else if pressed {
            self.toggled = !self.toggled;
        }
        can && match mode {
            TalkMode::Hold => held,
            TalkMode::Toggle => self.toggled,
        }
    }
}

/// A `/mute` or `/unmute` line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mute {
    /// `/mute NAME`: mute whoever's called that; `/mute` alone: whoever's talking now.
    Mute(Option<String>),
    /// `/unmute NAME`; `/unmute` alone: everyone.
    Unmute(Option<String>),
}

impl Mute {
    /// `line` as a mute command, if it's one (`/muted` and `/mutex` aren't).
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.trim();
        let (mute, rest) = match line.strip_prefix("/mute") {
            Some(r) => (true, r),
            None => (false, line.strip_prefix("/unmute")?),
        };
        if !rest.is_empty() && !rest.starts_with(' ') {
            return None;
        }
        let who = rest.trim();
        let who = (!who.is_empty()).then(|| who.to_string());
        Some(if mute { Mute::Mute(who) } else { Mute::Unmute(who) })
    }
}

/// Who a callsign `name` names among `names` (case aside, a part of one will do: the first that has
/// it, a whole match before a part).
pub fn find<'a>(name: &str, names: impl Iterator<Item = &'a str> + Clone) -> Option<&'a str> {
    let want = name.trim().to_lowercase();
    if want.is_empty() {
        return None;
    }
    names
        .clone()
        .find(|n| n.to_lowercase() == want)
        .or_else(|| names.into_iter().find(|n| n.to_lowercase().contains(&want)))
}

/// A talker as the page keys them: how it came and who (`air:12`, `radio:301`).
pub fn talker_key(talker: u16, via: Via) -> String {
    match via {
        Via::Air => format!("air:{talker}"),
        Via::Radio => format!("radio:{talker}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_voice_is_full_near_and_gone_at_the_reach() {
        assert_eq!(falloff(0.0), 1.0);
        assert_eq!(falloff(NEAR_M), 1.0);
        assert_eq!(falloff(REACH_M), 0.0);
        assert_eq!(falloff(100.0), 0.0);
        let mut last = 1.0;
        for k in 0..=300 {
            let g = falloff(k as f32 * 0.1);
            assert!(g <= last, "monotonic at {k}");
            last = g;
        }
        // Plain across a square (about -9 dB at 10 m), a murmur near the edge.
        assert!((0.33..0.40).contains(&falloff(10.0)), "{}", falloff(10.0));
        assert!(falloff(25.0) < 0.02);
    }

    #[test]
    fn the_ring_keeps_the_newest_sixty_four_and_counts_the_rest() {
        let mut r = VoiceRing::default();
        for k in 0..100u16 {
            r.push(VoiceFrame::new(k, false, &[8]).unwrap());
        }
        assert_eq!((r.len(), r.dropped), (VOICE_RING, 36));
        let seqs: Vec<u16> = r.drain().map(|f| f.seq).collect();
        assert_eq!(seqs.first(), Some(&36));
        assert_eq!(seqs.last(), Some(&99));
        assert!(r.is_empty());
    }

    #[test]
    fn the_talk_key_holds_or_toggles_and_lets_go_when_it_cant() {
        let mut k = Keyed::default();
        assert!(!k.step(TalkMode::Hold, true, false, false));
        assert!(k.step(TalkMode::Hold, true, true, true));
        assert!(!k.step(TalkMode::Hold, false, true, false), "a panel took the keys");
        let mut k = Keyed::default();
        assert!(k.step(TalkMode::Toggle, true, true, true));
        assert!(k.step(TalkMode::Toggle, true, false, false), "still on after letting go");
        assert!(!k.step(TalkMode::Toggle, true, true, true), "the next press is off");
        assert!(k.step(TalkMode::Toggle, true, true, true));
        assert!(!k.step(TalkMode::Toggle, false, false, false));
        assert!(!k.step(TalkMode::Toggle, true, false, false), "dropped while it couldn't");
        for m in TalkMode::ALL {
            assert_eq!(TalkMode::parse(m.name()), Some(m));
        }
    }

    #[test]
    fn mute_lines_are_the_clients_and_muted_isnt_one() {
        assert_eq!(Mute::parse("/mute"), Some(Mute::Mute(None)));
        assert_eq!(Mute::parse("  /mute   Pagan "), Some(Mute::Mute(Some("Pagan".into()))));
        assert_eq!(Mute::parse("/unmute"), Some(Mute::Unmute(None)));
        assert_eq!(Mute::parse("/unmute Maren Ash"), Some(Mute::Unmute(Some("Maren Ash".into()))));
        assert_eq!(Mute::parse("/muted"), None);
        assert_eq!(Mute::parse("/mutex Bo"), None);
        assert_eq!(Mute::parse("mute Bo"), None);
        assert_eq!(Mute::parse("/unmuteBo"), None);
        let names = ["Maren", "Pagan", "Mar"];
        assert_eq!(find("mar", names.iter().copied()), Some("Mar"), "a whole name first");
        assert_eq!(find("PAG", names.iter().copied()), Some("Pagan"));
        assert_eq!(find("viktor", names.iter().copied()), None);
        assert_eq!(find(" ", names.iter().copied()), None);
        assert_eq!(talker_key(12, Via::Air), "air:12");
        assert_eq!(talker_key(301, Via::Radio), "radio:301");
    }
}
