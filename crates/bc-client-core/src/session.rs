//! The link's lifecycle, from the title screen's Play to the world and back: dialing, the
//! handshake, being in game, losing the link and redialing, and failing with a sentence a player
//! can act on.
//!
//! Pure (no clock, no I/O): the browser client calls it with the page's time and does what the
//! returned [`LinkAction`] says, so every transition is testable natively.

use bc_proto::control::RejectReason;

/// How long a dial (certificate discovery + WebTransport connect) may take, s.
pub const DIAL_TIMEOUT: f64 = 10.0;
/// How long the server may take to answer Hello, s.
pub const HANDSHAKE_TIMEOUT: f64 = 10.0;
/// Waits between automatic redials after a link that was in game drops, s. One entry per attempt.
pub const BACKOFF: [f64; 6] = [1.0, 2.0, 4.0, 8.0, 15.0, 15.0];

/// Why the link is down.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkError {
    /// The browser has no WebTransport.
    NoWebTransport,
    /// The dial failed, with the browser's words.
    Unreachable(String),
    /// The dial didn't finish in [`DIAL_TIMEOUT`].
    DialTimeout,
    /// The server didn't answer Hello in [`HANDSHAKE_TIMEOUT`].
    HandshakeTimeout,
    /// The server refused the session.
    Rejected(RejectReason),
    /// The server said goodbye.
    ServerBye(u8),
    /// The link dropped.
    Lost,
}

impl LinkError {
    /// Whether redialing could help (a server that's down may come back; a refusal won't change).
    pub fn retryable(&self) -> bool {
        match self {
            LinkError::NoWebTransport => false,
            LinkError::Rejected(r) => matches!(r, RejectReason::ServerFull),
            _ => true,
        }
    }

    /// Whether only reloading the page can help (the page is older than the server).
    pub fn needs_reload(&self) -> bool {
        matches!(self, LinkError::Rejected(RejectReason::VersionMismatch))
    }

    /// What to tell the player.
    pub fn text(&self) -> String {
        match self {
            LinkError::NoWebTransport => "This browser has no WebTransport. Play in Chrome or Edge.".into(),
            LinkError::Unreachable(why) => {
                format!("Can't reach the server: it may be down, or UDP port 4433 may be blocked. ({why})")
            }
            LinkError::DialTimeout => {
                "The server didn't answer. It may be down, or UDP port 4433 may be blocked.".into()
            }
            LinkError::HandshakeTimeout => "The server connected but never let us in.".into(),
            LinkError::Rejected(r) => match r {
                RejectReason::VersionMismatch => {
                    "This page is older than the server. Reload to update the game.".into()
                }
                RejectReason::ServerFull => "The sector is full. Try again in a moment.".into(),
                RejectReason::BadHello => "The server didn't understand this client.".into(),
                RejectReason::FrameNotAllowed => "That frame isn't flyable here. Pick another.".into(),
            },
            LinkError::ServerBye(_) => "The server closed the connection.".into(),
            LinkError::Lost => "The link to the server dropped.".into(),
        }
    }
}

/// Where the link is.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkState {
    /// Nothing asked for yet: the title screen.
    Idle,
    /// Finding the server and opening the transport.
    Dialing { attempt: u32, since: f64 },
    /// Transport up, waiting for the server's Welcome.
    Handshake { attempt: u32, since: f64 },
    /// In the world.
    InGame { since: f64 },
    /// Down; redialing at `at`.
    Retrying { attempt: u32, at: f64, why: LinkError },
    /// Down, and waiting for the player.
    Failed(LinkError),
}

/// What the driver must do now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkAction {
    /// Start a dial (and forget any previous session).
    Dial,
    /// Close the current transport, if any.
    Close,
}

/// The link's state machine.
#[derive(Clone, Debug)]
pub struct Link {
    pub state: LinkState,
    /// Automatic redials that got back in game.
    pub reconnects: u32,
    /// The last error, kept after a successful redial (for the dev hooks).
    pub last_error: Option<LinkError>,
}

impl Default for Link {
    fn default() -> Self {
        Self { state: LinkState::Idle, reconnects: 0, last_error: None }
    }
}

impl Link {
    /// The player pressed Play, Retry or Reconnect now: dial now. `None` while already dialing or
    /// in game. Reconnecting early keeps counting attempts, so a failure still redials.
    pub fn play(&mut self, now: f64) -> Option<LinkAction> {
        let attempt = match self.state {
            LinkState::Idle | LinkState::Failed(_) => 0,
            LinkState::Retrying { attempt, .. } => attempt,
            _ => return None,
        };
        self.state = LinkState::Dialing { attempt, since: now };
        Some(LinkAction::Dial)
    }

    /// The player gave up (Cancel, back to the title, disconnect).
    pub fn cancel(&mut self) -> LinkAction {
        self.state = LinkState::Idle;
        LinkAction::Close
    }

    /// The browser can't do WebTransport at all.
    pub fn unsupported(&mut self) {
        self.fail(LinkError::NoWebTransport);
    }

    /// The transport opened: now the handshake.
    pub fn dialed(&mut self, now: f64) {
        if let LinkState::Dialing { attempt, .. } = self.state {
            self.state = LinkState::Handshake { attempt, since: now };
        }
    }

    /// The server's Welcome arrived.
    pub fn welcomed(&mut self, now: f64) {
        if let LinkState::Handshake { attempt, .. } = self.state {
            if attempt > 0 {
                self.reconnects += 1;
            }
            self.state = LinkState::InGame { since: now };
        }
    }

    /// The dial failed.
    pub fn dial_failed(&mut self, now: f64, why: String) -> Option<LinkAction> {
        match self.state {
            LinkState::Dialing { .. } | LinkState::Handshake { .. } => {
                Some(self.down(now, LinkError::Unreachable(why)))
            }
            _ => None,
        }
    }

    /// The server refused the session.
    pub fn rejected(&mut self, now: f64, reason: RejectReason) -> Option<LinkAction> {
        match self.state {
            LinkState::Dialing { .. } | LinkState::Handshake { .. } | LinkState::InGame { .. } => {
                Some(self.down(now, LinkError::Rejected(reason)))
            }
            _ => None,
        }
    }

    /// The server said goodbye.
    pub fn server_bye(&mut self, now: f64, reason: u8) -> Option<LinkAction> {
        match self.state {
            LinkState::Handshake { .. } | LinkState::InGame { .. } => {
                Some(self.down(now, LinkError::ServerBye(reason)))
            }
            _ => None,
        }
    }

    /// The transport closed under us.
    pub fn lost(&mut self, now: f64) -> Option<LinkAction> {
        match self.state {
            LinkState::Dialing { .. } | LinkState::Handshake { .. } | LinkState::InGame { .. } => {
                Some(self.down(now, LinkError::Lost))
            }
            _ => None,
        }
    }

    /// Timeouts and scheduled redials; call every frame.
    pub fn tick(&mut self, now: f64) -> Option<LinkAction> {
        match self.state.clone() {
            LinkState::Dialing { since, .. } if now - since > DIAL_TIMEOUT => {
                Some(self.down(now, LinkError::DialTimeout))
            }
            LinkState::Handshake { since, .. } if now - since > HANDSHAKE_TIMEOUT => {
                Some(self.down(now, LinkError::HandshakeTimeout))
            }
            LinkState::Retrying { attempt, at, .. } if now >= at => {
                self.state = LinkState::Dialing { attempt, since: now };
                Some(LinkAction::Dial)
            }
            _ => None,
        }
    }

    /// In the world.
    pub fn in_game(&self) -> bool {
        matches!(self.state, LinkState::InGame { .. })
    }

    /// Short name for the dev hooks and the page.
    pub fn name(&self) -> &'static str {
        match self.state {
            LinkState::Idle => "idle",
            LinkState::Dialing { .. } => "dialing",
            LinkState::Handshake { .. } => "handshake",
            LinkState::InGame { .. } => "ingame",
            LinkState::Retrying { .. } => "retrying",
            LinkState::Failed(_) => "failed",
        }
    }

    /// Which redial this is (0: the first dial).
    pub fn attempt(&self) -> u32 {
        match self.state {
            LinkState::Dialing { attempt, .. }
            | LinkState::Handshake { attempt, .. }
            | LinkState::Retrying { attempt, .. } => attempt,
            _ => 0,
        }
    }

    fn fail(&mut self, why: LinkError) {
        self.last_error = Some(why.clone());
        self.state = LinkState::Failed(why);
    }

    /// The link went down: redial if it had been in the world and the error allows, else fail.
    fn down(&mut self, now: f64, why: LinkError) -> LinkAction {
        self.last_error = Some(why.clone());
        // Only a link that had made it into the world redials by itself: a first connection that
        // fails waits for the player (who may be on the wrong server, or offline).
        let next = match self.state {
            LinkState::InGame { .. } => Some(0),
            LinkState::Dialing { attempt, .. } | LinkState::Handshake { attempt, .. } if attempt > 0 => {
                Some(attempt)
            }
            _ => None,
        };
        self.state = match next {
            Some(n) if why.retryable() && (n as usize) < BACKOFF.len() => {
                LinkState::Retrying { attempt: n + 1, at: now + BACKOFF[n as usize], why }
            }
            _ => LinkState::Failed(why),
        };
        LinkAction::Close
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_game(link: &mut Link, now: f64) {
        assert_eq!(link.play(now), Some(LinkAction::Dial));
        link.dialed(now + 0.1);
        link.welcomed(now + 0.2);
        assert!(link.in_game());
    }

    #[test]
    fn a_first_dial_that_fails_waits_for_the_player() {
        let mut link = Link::default();
        link.play(0.0);
        assert_eq!(link.dial_failed(1.0, "refused".into()), Some(LinkAction::Close));
        assert!(matches!(link.state, LinkState::Failed(LinkError::Unreachable(_))));
        // Nothing happens by itself.
        assert_eq!(link.tick(100.0), None);
        // Retry is the player's.
        assert_eq!(link.play(101.0), Some(LinkAction::Dial));
    }

    #[test]
    fn a_dial_that_hangs_times_out() {
        let mut link = Link::default();
        link.play(0.0);
        assert_eq!(link.tick(DIAL_TIMEOUT - 0.1), None);
        assert_eq!(link.tick(DIAL_TIMEOUT + 0.1), Some(LinkAction::Close));
        assert_eq!(link.state, LinkState::Failed(LinkError::DialTimeout));
    }

    #[test]
    fn a_handshake_that_hangs_times_out() {
        let mut link = Link::default();
        link.play(0.0);
        link.dialed(1.0);
        assert_eq!(link.tick(1.0 + HANDSHAKE_TIMEOUT + 0.1), Some(LinkAction::Close));
        assert_eq!(link.state, LinkState::Failed(LinkError::HandshakeTimeout));
    }

    #[test]
    fn a_lost_link_redials_with_backoff_and_counts_the_reconnect() {
        let mut link = Link::default();
        in_game(&mut link, 0.0);
        assert_eq!(link.lost(10.0), Some(LinkAction::Close));
        assert!(matches!(link.state, LinkState::Retrying { attempt: 1, .. }));
        assert_eq!(link.tick(10.0 + BACKOFF[0] - 0.01), None);
        assert_eq!(link.tick(10.0 + BACKOFF[0]), Some(LinkAction::Dial));
        link.dialed(11.2);
        link.welcomed(11.3);
        assert!(link.in_game());
        assert_eq!(link.reconnects, 1);
        assert_eq!(link.last_error, Some(LinkError::Lost));
    }

    #[test]
    fn redials_back_off_then_give_up() {
        let mut link = Link::default();
        in_game(&mut link, 0.0);
        let mut now = 5.0;
        link.lost(now);
        let mut waits = Vec::new();
        while let LinkState::Retrying { at, .. } = link.state {
            waits.push(at - now);
            now = at;
            assert_eq!(link.tick(now), Some(LinkAction::Dial));
            link.dial_failed(now + 0.5, "down".into());
            now += 0.5;
        }
        assert_eq!(waits, BACKOFF.to_vec());
        assert!(matches!(link.state, LinkState::Failed(LinkError::Unreachable(_))));
    }

    #[test]
    fn a_refusal_that_cannot_change_does_not_redial() {
        let mut link = Link::default();
        in_game(&mut link, 0.0);
        link.lost(1.0);
        link.tick(1.0 + BACKOFF[0]);
        link.dialed(2.1);
        link.rejected(2.2, RejectReason::VersionMismatch);
        let LinkState::Failed(e) = &link.state else { panic!("{:?}", link.state) };
        assert!(e.needs_reload());
        assert!(!e.retryable());
        // A full sector may empty.
        assert!(LinkError::Rejected(RejectReason::ServerFull).retryable());
    }

    #[test]
    fn cancel_goes_home_from_anywhere() {
        let mut link = Link::default();
        in_game(&mut link, 0.0);
        link.lost(1.0);
        assert_eq!(link.cancel(), LinkAction::Close);
        assert_eq!(link.state, LinkState::Idle);
        assert_eq!(link.tick(1_000.0), None);
    }

    #[test]
    fn late_news_from_a_dead_dial_is_ignored() {
        let mut link = Link::default();
        link.play(0.0);
        link.cancel();
        // The dial finished after all, or failed: the title screen doesn't care.
        link.dialed(1.0);
        assert_eq!(link.state, LinkState::Idle);
        assert_eq!(link.dial_failed(1.0, "x".into()), None);
        assert_eq!(link.lost(1.0), None);
    }

    #[test]
    fn every_error_has_words() {
        for e in [
            LinkError::NoWebTransport,
            LinkError::Unreachable("x".into()),
            LinkError::DialTimeout,
            LinkError::HandshakeTimeout,
            LinkError::Rejected(RejectReason::VersionMismatch),
            LinkError::Rejected(RejectReason::ServerFull),
            LinkError::Rejected(RejectReason::BadHello),
            LinkError::Rejected(RejectReason::FrameNotAllowed),
            LinkError::ServerBye(0),
            LinkError::Lost,
        ] {
            assert!(e.text().len() > 10, "{e:?}");
        }
    }
}
