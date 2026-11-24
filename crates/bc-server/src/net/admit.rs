//! Who gets a connection (`docs/PEERS.md`, "Staying up"; `docs/ARCHITECTURE.md`, "Under abuse").
//!
//! Titanfall's servers were DDoSed out of the game for years. A flood of packets is for the
//! network upstream of the server to stop (a host's scrubbing, a firewall's rate limits); what the
//! server does is keep a flood of *connections* from crowding the pilots out:
//! - **Retry under load.** With more than [`Limits::retry_above`] handshakes under way, an address
//!   that hasn't proved it hears back (QUIC's address validation) is sent a Retry first: a flood
//!   from spoofed addresses never gets as far as a handshake.
//! - **A share per address.** One address holds at most [`Limits::per_address`] connections,
//!   handshaking or open, and opens no more than that many each [`HANDSHAKE`] (a bucket that
//!   refills at that rate). Loopback is excepted: the server's own agents connect from it.
//! - **A ceiling.** At most [`Limits::connections`] in all, counted by the passes here; and at most
//!   [`Limits::quic_connections`] that QUIC really holds (the accept loop, `endpoint.rs`), so a
//!   connection that outlives its pass can't pile up unseen. QUIC keeps a closed connection for
//!   three probe timeouts after its pass is given back, which a peer can stretch to most of a
//!   minute (its `max_ack_delay`): the opening rate is what keeps one address from filling that
//!   count with connections it opens and closes, at most about
//!   `per_address × (1 + 49 s / HANDSHAKE)` of them, and the ceiling has room over the passes'
//!   for those that close in the ordinary way.
//! - **A deadline.** A handshake, QUIC's and WebTransport's, finishes within [`HANDSHAKE`].
//!
//! What a session may do once it's in is limited where it's read (`session.rs`: inputs, the
//! radio, and the hangar's requests).

use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a connection's handshakes may take, QUIC's and then WebTransport's.
pub const HANDSHAKE: Duration = Duration::from_secs(5);

/// How many times the passes' ceiling QUIC may hold ([`Limits::quic_connections`]): room for the
/// connections closing (a few per address that comes and goes, and at most
/// `per_address × (1 + 49 s / HANDSHAKE)` from one that opens and closes as fast as it's let).
pub const QUIC_HEADROOM: u32 = 4;

/// How many connections the server takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Connections in all, handshaking or open.
    pub connections: u32,
    /// Connections QUIC holds in all, closing ones included: past this a new one is refused.
    pub quic_connections: u32,
    /// Connections from one address, and how many it may open each [`HANDSHAKE`]...
    pub per_address: u32,
    /// ...but loopback's, where the server's own agents connect from.
    pub loopback_excepted: bool,
    /// Handshakes under way past which an unvalidated address is sent a Retry first.
    pub retry_above: u32,
}

impl Default for Limits {
    fn default() -> Self {
        // A sector takes 64 pilots; a household or a campus behind one address brings a few.
        let connections = 512;
        Self {
            connections,
            quic_connections: connections * QUIC_HEADROOM,
            per_address: 8,
            loopback_excepted: true,
            retry_above: 32,
        }
    }
}

/// What to do with a connection attempt.
#[derive(Debug)]
pub enum Verdict {
    /// In, for as long as the [`Pass`] is kept.
    Admit(Pass),
    /// Ask it to prove its address first (a QUIC Retry).
    Retry,
    Refuse(Refusal),
}

/// Why a connection attempt was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The server has all the connections it takes.
    Full,
    /// Its address has its share, or has opened its share this [`HANDSHAKE`].
    Address,
}

/// What one address holds, and how many more it may open now.
#[derive(Debug)]
struct Holder {
    held: u32,
    /// Connections it may open now (a bucket of `per_address` refilling at `per_address` a
    /// [`HANDSHAKE`]), as of `at`.
    opens: f64,
    at: Instant,
}

impl Holder {
    /// Its bucket refilled to `now`.
    fn refill(&mut self, now: Instant, per_address: u32) {
        let rate = f64::from(per_address) / HANDSHAKE.as_secs_f64();
        let gap = now.saturating_duration_since(self.at).as_secs_f64();
        self.opens = (self.opens + gap * rate).min(f64::from(per_address));
        self.at = self.at.max(now);
    }

    /// Holds nothing and could open its whole share: no need to remember it.
    fn idle(&self, now: Instant, per_address: u32) -> bool {
        let rate = f64::from(per_address) / HANDSHAKE.as_secs_f64();
        let gap = now.saturating_duration_since(self.at).as_secs_f64();
        self.held == 0 && self.opens + gap * rate >= f64::from(per_address)
    }
}

/// The addresses heard from, and when those holding nothing were last forgotten.
#[derive(Debug)]
struct Addresses {
    by_ip: HashMap<IpAddr, Holder>,
    swept: Instant,
}

/// The connections the server holds, in all and by address.
#[derive(Debug)]
pub struct Admission {
    limits: Limits,
    by_address: Mutex<Addresses>,
    open: AtomicU32,
    handshaking: AtomicU32,
}

impl Admission {
    pub fn new(limits: Limits) -> Arc<Self> {
        Arc::new(Self {
            limits,
            by_address: Mutex::new(Addresses { by_ip: HashMap::new(), swept: Instant::now() }),
            open: AtomicU32::new(0),
            handshaking: AtomicU32::new(0),
        })
    }

    /// A connection attempt from `ip`; `validated`: it has proved it hears back at that address.
    pub fn admit(self: &Arc<Self>, ip: IpAddr, validated: bool) -> Verdict {
        self.admit_at(ip, validated, Instant::now())
    }

    fn admit_at(self: &Arc<Self>, ip: IpAddr, validated: bool, now: Instant) -> Verdict {
        let ip = ip.to_canonical();
        let local = ip.is_loopback() && self.limits.loopback_excepted;
        let ip = whose(ip);
        let per_address = self.limits.per_address;
        let mut addresses = self.by_address.lock().unwrap_or_else(|e| e.into_inner());
        // Those holding nothing are forgotten once their buckets are full again.
        if now.saturating_duration_since(addresses.swept) >= HANDSHAKE {
            addresses.swept = now;
            addresses.by_ip.retain(|_, h| !h.idle(now, per_address));
        }
        if self.open.load(Ordering::Relaxed) >= self.limits.connections {
            return Verdict::Refuse(Refusal::Full);
        }
        let mut holder =
            addresses.by_ip.remove(&ip).unwrap_or(Holder { held: 0, opens: f64::from(per_address), at: now });
        holder.refill(now, per_address);
        let verdict = if !local && (holder.held >= per_address || holder.opens < 1.0) {
            Verdict::Refuse(Refusal::Address)
        } else if !validated && self.handshaking.load(Ordering::Relaxed) >= self.limits.retry_above {
            Verdict::Retry
        } else {
            holder.held += 1;
            if !local {
                holder.opens -= 1.0;
            }
            self.open.fetch_add(1, Ordering::Relaxed);
            self.handshaking.fetch_add(1, Ordering::Relaxed);
            Verdict::Admit(Pass { admission: self.clone(), ip, handshaking: true })
        };
        // An address heard from but holding nothing is kept only while its bucket isn't full.
        if !holder.idle(now, per_address) {
            addresses.by_ip.insert(ip, holder);
        }
        verdict
    }

    /// What it takes.
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Connections held, handshaking or open.
    pub fn open(&self) -> u32 {
        self.open.load(Ordering::Relaxed)
    }

    /// Handshakes under way.
    pub fn handshaking(&self) -> u32 {
        self.handshaking.load(Ordering::Relaxed)
    }

    /// Distinct addresses holding connections.
    pub fn addresses(&self) -> usize {
        self.by_address.lock().map_or(0, |a| a.by_ip.values().filter(|h| h.held > 0).count())
    }
}

/// Whom a connection from `ip` (canonical) counts against: an IPv4 address, or an IPv6 address's
/// /64, which is what one subscriber is given (a host can take any address in it).
fn whose(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => IpAddr::V6(Ipv6Addr::from(u128::from(v6) & !(u128::from(u64::MAX)))),
        v4 => v4,
    }
}

/// A connection's place, given back when it's dropped.
#[derive(Debug)]
pub struct Pass {
    admission: Arc<Admission>,
    ip: IpAddr,
    handshaking: bool,
}

impl Pass {
    /// Its handshakes are done.
    pub fn established(&mut self) {
        if std::mem::take(&mut self.handshaking) {
            self.admission.handshaking.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

impl Drop for Pass {
    fn drop(&mut self) {
        self.established();
        let a = &self.admission;
        a.open.fetch_sub(1, Ordering::Relaxed);
        let mut addresses = a.by_address.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(h) = addresses.by_ip.get_mut(&self.ip) {
            h.held = h.held.saturating_sub(1);
            // Its bucket stays until it's full again: closing doesn't give back what opening took.
            if h.idle(Instant::now(), a.limits.per_address) {
                addresses.by_ip.remove(&self.ip);
            }
        }
    }
}

/// How fast a session may ask things of its hangar on the control stream: a bucket of
/// [`Requests::BURST`] that refills at [`Requests::PER_SEC`]. A session that keeps on past an
/// empty bucket is ended ([`Requests::ENDS_AFTER`] refused in a row).
#[derive(Debug)]
pub struct Requests {
    tokens: f64,
    last: std::time::Instant,
    refused: u32,
}

/// What a session's request comes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    Take,
    /// Too fast: refused (say so).
    Refuse,
    /// Far too fast, for too long: end the session.
    End,
}

impl Requests {
    pub const PER_SEC: f64 = 10.0;
    pub const BURST: f64 = 40.0;
    pub const ENDS_AFTER: u32 = 200;

    pub fn new(now: std::time::Instant) -> Self {
        Self { tokens: Self::BURST, last: now, refused: 0 }
    }

    pub fn ask(&mut self, now: std::time::Instant) -> Asked {
        let gap = now.saturating_duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + gap * Self::PER_SEC).min(Self::BURST);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            self.refused = 0;
            Asked::Take
        } else {
            self.refused += 1;
            if self.refused >= Self::ENDS_AFTER { Asked::End } else { Asked::Refuse }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn ip(last: u8) -> IpAddr {
        IpAddr::from([203, 0, 113, last])
    }

    fn admit(a: &Arc<Admission>, from: IpAddr) -> Option<Pass> {
        match a.admit(from, true) {
            Verdict::Admit(p) => Some(p),
            _ => None,
        }
    }

    #[test]
    fn an_address_gets_its_share_and_no_more() {
        let a = Admission::new(Limits {
            connections: 100,
            per_address: 3,
            retry_above: 100,
            ..Limits::default()
        });
        let mut held: Vec<Pass> = (0..3).map(|_| admit(&a, ip(1)).unwrap()).collect();
        assert!(matches!(a.admit(ip(1), true), Verdict::Refuse(Refusal::Address)));
        // Another address is let in.
        let other = admit(&a, ip(2)).unwrap();
        assert_eq!((a.open(), a.addresses()), (4, 2));
        // A connection closing makes room for its address again, once it may open another.
        held.pop();
        let later = Instant::now() + HANDSHAKE;
        assert!(matches!(a.admit_at(ip(1), true, later), Verdict::Admit(_)));
        drop(other);
        assert_eq!(a.addresses(), 1);
    }

    #[test]
    fn an_address_opens_its_share_a_handshake_and_no_faster() {
        let a = Admission::new(Limits {
            connections: 100,
            per_address: 2,
            loopback_excepted: true,
            retry_above: 100,
            ..Limits::default()
        });
        let t0 = Instant::now();
        let opens = |at: Duration| match a.admit_at(ip(1), true, t0 + at) {
            Verdict::Admit(p) => drop(p),
            Verdict::Refuse(why) => panic!("refused {why:?} at {at:?}"),
            Verdict::Retry => panic!("retry"),
        };
        let refused =
            |at: Duration| matches!(a.admit_at(ip(1), true, t0 + at), Verdict::Refuse(Refusal::Address));
        // Opening and closing at once, as fast as it can: its share, then no more...
        opens(Duration::ZERO);
        opens(Duration::ZERO);
        assert!(refused(Duration::ZERO), "a third went through the same instant");
        // ...but one more each half of a handshake (two a handshake).
        assert!(refused(HANDSHAKE / 4));
        opens(HANDSHAKE / 2);
        assert!(refused(HANDSHAKE / 2));
        opens(HANDSHAKE);
        // Over a minute flat out, at most its share each handshake: what one address can leave
        // in QUIC's count while its closed connections drain.
        let mut n = 0;
        for ms in (5_000..65_000).step_by(10) {
            if let Verdict::Admit(p) = a.admit_at(ip(1), true, t0 + Duration::from_millis(ms)) {
                n += 1;
                drop(p);
            }
        }
        assert!(n <= 2 * 60 / HANDSHAKE.as_secs() as u32 + 2, "{n} opened in a minute");
        // Loopback, where the server's own agents connect from, has no rate.
        for _ in 0..10 {
            assert!(matches!(a.admit_at(IpAddr::from([127, 0, 0, 1]), true, t0), Verdict::Admit(_)));
        }
        // An address that went quiet is forgotten once its share is whole again.
        let quiet = t0 + Duration::from_secs(80);
        assert!(matches!(a.admit_at(ip(2), true, quiet), Verdict::Admit(_)));
        let remembered: Vec<IpAddr> = a.by_address.lock().unwrap().by_ip.keys().copied().collect();
        assert_eq!(remembered, [ip(2)], "addresses holding nothing, their share whole, are still remembered");
    }

    #[test]
    fn an_address_is_counted_as_its_subscriber() {
        let a = Admission::new(Limits { per_address: 2, ..Limits::default() });
        // IPv4 as itself, mapped into IPv6 or not.
        let v4 = admit(&a, ip(1)).unwrap();
        let mapped = admit(&a, IpAddr::V6(Ipv6Addr::from([0, 0, 0, 0, 0, 0xffff, 0xcb00, 0x7101]))).unwrap();
        assert!(matches!(a.admit(ip(1), true), Verdict::Refuse(Refusal::Address)));
        // IPv6 by its /64: a host hopping addresses within it is one subscriber.
        let net = |host: u16| IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 1, 2, 0, 0, 0, host));
        let held: Vec<Pass> = (1..=2).map(|h| admit(&a, net(h)).unwrap()).collect();
        assert!(matches!(a.admit(net(3), true), Verdict::Refuse(Refusal::Address)));
        assert!(admit(&a, IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 1, 3, 0, 0, 0, 1))).is_some());
        // Loopback mapped into IPv6 is loopback.
        let local: Vec<Pass> = (0..4)
            .map(|_| admit(&a, IpAddr::V6(Ipv6Addr::from([0, 0, 0, 0, 0, 0xffff, 0x7f00, 1]))).unwrap())
            .collect();
        drop((v4, mapped, held, local));
    }

    #[test]
    fn loopback_is_excepted_but_not_from_the_ceiling() {
        let a =
            Admission::new(Limits { connections: 5, per_address: 2, retry_above: 100, ..Limits::default() });
        let local: Vec<Pass> = (0..5).map(|_| admit(&a, IpAddr::from([127, 0, 0, 1])).unwrap()).collect();
        assert!(matches!(a.admit(IpAddr::from([127, 0, 0, 1]), true), Verdict::Refuse(Refusal::Full)));
        assert!(matches!(a.admit(ip(9), true), Verdict::Refuse(Refusal::Full)));
        drop(local);
        assert_eq!((a.open(), a.handshaking(), a.addresses()), (0, 0, 0));
    }

    #[test]
    fn under_load_an_unproved_address_is_asked_to_retry() {
        let a = Admission::new(Limits {
            connections: 100,
            per_address: 100,
            retry_above: 2,
            ..Limits::default()
        });
        let mut first: Vec<Pass> = (0..2).map(|k| admit(&a, ip(k)).unwrap()).collect();
        assert_eq!(a.handshaking(), 2);
        // Two handshakes under way: a new address must prove itself; one that has comes in.
        assert!(matches!(a.admit(ip(7), false), Verdict::Retry));
        let proved = admit(&a, ip(7)).unwrap();
        // Handshakes done, the load is off: no Retry.
        for p in &mut first {
            p.established();
        }
        drop(proved);
        assert_eq!(a.handshaking(), 0);
        assert!(matches!(a.admit(ip(8), false), Verdict::Admit(_)));
    }

    #[test]
    fn a_request_flood_is_refused_then_ended() {
        let t0 = Instant::now();
        let mut r = Requests::new(t0);
        // A burst is fine...
        for _ in 0..Requests::BURST as usize {
            assert_eq!(r.ask(t0), Asked::Take);
        }
        // ...then it's refused until the bucket refills...
        assert_eq!(r.ask(t0), Asked::Refuse);
        assert_eq!(r.ask(t0 + Duration::from_millis(150)), Asked::Take);
        // ...and a session that never stops is ended.
        let mut last = Asked::Take;
        for _ in 0..Requests::ENDS_AFTER {
            last = r.ask(t0 + Duration::from_millis(150));
        }
        assert_eq!(last, Asked::End);
    }
}
