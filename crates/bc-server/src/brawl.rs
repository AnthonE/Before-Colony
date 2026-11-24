//! Pilots on foot who fight, as the server keeps them (the owner's "open season" on foot,
//! `docs/DECISIONS.md`; `docs/OPEN_CITY.md`, "Heat and the Watch"): each one's health and fight
//! (`bc_sim::colony::brawl::Brawl`, on the sector's tick) by their session, off the tick like the
//! Watch, and what judges a blow and a car that runs a pilot down from the poses the plaza took
//! ([`claimed`], [`lands_on`], [`runs_down`]): never from what a client says alone.
//!
//! Who's down is also kept where the plaza reads it without a lock ([`Brawls::is_down`]): it writes
//! `RIDE_DOWN` into the poses it relays of them, and refuses their moves.
//!
//! - **A blow** (`Request::Strike`): the striker says where they stood and the way they faced; that
//!   has to be where the plaza's last poses of them could have gone by now ([`claimed`]), and the
//!   blow has to land on one of the other's poses of the last moment ([`lands_on`]: the striker saw
//!   them a little late).
//! - **A car** (`Request::RunDown`): the driver's poses have to be unbroken lately and fast
//!   ([`runs_down`]: the speed measured from them, not the speed a pose says), and one of them near
//!   one of the other's.
//! - **Out:** carried up to the bay by their own session (`net::session`), which takes the word
//!   here ([`Brawls::take_out`]).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bc_proto::presence::{BASE_DECK, PersonPose, RING_DECK};
use bc_sim::colony::brawl::{self, Brawl, Cannot, HELP_TICKS, Hurt};
use bc_sim::colony::deck::{self, BASE, RING};

/// How far back a pilot's poses are looked at for a blow or a car, s: everyone is drawn about 0.2 s
/// late, plus the relay's tenth and a pose's fifteenth, plus the way there.
pub const RECENT: Duration = Duration::from_millis(600);
/// The most a pilot on foot goes between a pose and what they say of where they stood, m/s (the
/// walker runs at 7), and what the wire and a pose in flight add, m.
const CLAIM_SPEED: f32 = 9.0;
const CLAIM_SLACK: f32 = 0.75;
/// A car knocks a pilot down at this speed or over, m/s (`bc_client_core::town::HIT_SPEED`).
pub const HIT_SPEED: f32 = 6.0;
/// Measured over at least this long between the driver's poses, s.
const SPEED_OVER: f32 = 0.2;
/// The driver's poses must come no further apart than this lately, s (a driver who holds their
/// poses back to lunge is refused), and the last no older.
const GAP: f32 = 0.3;
/// How near a car's middle a pilot it knocks down was, m: half a car's length and a person's
/// width, and a little.
pub const RUN_REACH: f32 = 3.5;

/// How `a` and `b` lie apart over their floor, m (round the decks the short way).
fn apart(a: &PersonPose, b: &PersonPose) -> f32 {
    let ds = match a.strip {
        RING_DECK => deck::round_from(RING, a.s, b.s),
        BASE_DECK => deck::round_from(BASE, a.s, b.s),
        _ => b.s - a.s,
    };
    (b.x - a.x).hypot(ds)
}

/// On foot: nothing ridden or driven, not sitting (as `bc_client_core::brawl` has it).
fn afoot(p: &PersonPose) -> bool {
    !p.driving() && p.riding().is_none() && !p.seated() && !p.down()
}

/// Where a striker stood for a blow, if what they say of it (`(x, s)` on their floor, the way they
/// faced) could be: near where the newest of their poses of the last moment (`recent`, newest
/// first, each as it was heard) had them on foot, as far as they could have gone since. That pose,
/// there and facing so.
pub fn claimed(claim: (f32, f32, f32), recent: &[(Instant, PersonPose)], now: Instant) -> Option<PersonPose> {
    let (x, s, yaw) = claim;
    // The newest: an older one allows more, and the newest has to allow it anyway.
    let (at, p) =
        recent.first().filter(|(at, p)| now.saturating_duration_since(*at) <= RECENT && afoot(p))?;
    let there = PersonPose { x, s, yaw, ..*p };
    let age = now.saturating_duration_since(*at).as_secs_f32();
    (apart(p, &there) <= CLAIM_SPEED * age + CLAIM_SLACK).then_some(there)
}

/// What a blow from `from` takes off whoever `victim`'s poses of the last moment (newest first) had
/// on foot, if it lands on any of them (`brawl::lands`): the most it would.
pub fn lands_on(from: &PersonPose, victim: &[(Instant, PersonPose)], now: Instant) -> Option<u8> {
    victim
        .iter()
        .filter(|(at, p)| now.saturating_duration_since(*at) <= RECENT && afoot(p))
        .filter_map(|(_, p)| brawl::lands(from, p))
        .max()
}

/// Whether a driver's car knocked down the pilot on foot whose poses of the last moment are
/// `victim`, by the driver's own (`driver`, newest first): at the wheel, their poses unbroken lately
/// (none more than [`GAP`] apart, the last no older), as fast as [`HIT_SPEED`] by them, and one of
/// them within [`RUN_REACH`] of one of the victim's. Why not, if not.
pub fn runs_down(
    driver: &[(Instant, PersonPose)],
    victim: &[(Instant, PersonPose)],
    now: Instant,
) -> Result<(), &'static str> {
    let fresh = |at: &Instant| now.saturating_duration_since(*at) <= RECENT;
    let mine: Vec<&(Instant, PersonPose)> = driver.iter().take_while(|(at, _)| fresh(at)).collect();
    let Some((last_at, last)) = mine.first().copied() else { return Err("you're not at a wheel") };
    if !last.driving() {
        return Err("you're not at a wheel");
    }
    if now.saturating_duration_since(*last_at).as_secs_f32() > GAP
        || mine.windows(2).any(|w| w[0].0.saturating_duration_since(w[1].0).as_secs_f32() > GAP)
    {
        return Err("too far between");
    }
    // The speed from the poses themselves, over a fifth of a second or more.
    let Some((then_at, then)) = mine
        .iter()
        .find(|(at, p)| p.driving() && last_at.saturating_duration_since(*at).as_secs_f32() >= SPEED_OVER)
    else {
        return Err("too slow");
    };
    let speed = apart(then, last) / last_at.saturating_duration_since(*then_at).as_secs_f32();
    if speed < HIT_SPEED {
        return Err("too slow");
    }
    let near = mine.iter().filter(|(_, d)| d.driving()).any(|(_, d)| {
        victim
            .iter()
            .filter(|(at, v)| fresh(at) && afoot(v))
            .any(|(_, v)| d.strip == v.strip && apart(d, v) <= RUN_REACH)
    });
    if near { Ok(()) } else { Err("nobody there") }
}

/// One pilot's fight, as kept.
struct Entry {
    brawl: Brawl,
    /// Knocked out: their session carries them up ([`Brawls::take_out`]).
    out: bool,
    /// Who's lifting them up, and since what tick.
    help: Option<(u16, u32)>,
    /// What whoever last hurt them is called.
    by_name: Option<String>,
    /// Moves whenever anything here does, for their session's word to them.
    version: u64,
}

impl Entry {
    fn new(brawl: Brawl) -> Self {
        Self { brawl, out: false, help: None, by_name: None, version: 1 }
    }
}

/// What a pilot's session tells them of their fight ([`Brawls::view`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub hp: u8,
    /// The tick they get up at, if they're down.
    pub up_at: Option<u32>,
    pub by: Option<String>,
    /// Who's lifting them up, by session.
    pub helper: Option<u16>,
    pub version: u64,
}

/// The counts `/status` shows: blows landed, pilots knocked down, knocked out, helped up, and
/// blows and cars refused.
#[derive(Default)]
struct Counts {
    blows: AtomicU64,
    downs: AtomicU64,
    outs: AtomicU64,
    helped: AtomicU64,
    refused: AtomicU64,
}

/// Every pilot's fight on foot, by session.
pub struct Brawls {
    /// By session id: the tick each is down till (0: up), and the tick they last got up or get up
    /// at (0: never down), for the plaza.
    downs: Box<[AtomicU32]>,
    rises: Box<[AtomicU32]>,
    inner: Mutex<HashMap<u16, Entry>>,
    counts: Counts,
}

impl Default for Brawls {
    fn default() -> Self {
        Self {
            downs: (0..=u16::MAX).map(|_| AtomicU32::new(0)).collect(),
            rises: (0..=u16::MAX).map(|_| AtomicU32::new(0)).collect(),
            inner: Mutex::new(HashMap::new()),
            counts: Counts::default(),
        }
    }
}

impl Brawls {
    /// Whether `id` is down at tick `tick` (no lock: the plaza asks for every pose).
    pub fn is_down(&self, id: u16, tick: u32) -> bool {
        tick < self.downs[usize::from(id)].load(Ordering::Relaxed)
    }

    /// The tick `id` last got up (or gets up at, if they're down); none if never down (no lock).
    pub fn rose_at(&self, id: u16) -> Option<u32> {
        Some(self.rises[usize::from(id)].load(Ordering::Relaxed)).filter(|t| *t > 0)
    }

    /// Keeps `id`'s down and up ticks where the plaza reads them.
    fn publish(&self, id: u16, b: &Brawl) {
        self.downs[usize::from(id)].store(b.up_at(0).unwrap_or(0), Ordering::Relaxed);
        self.rises[usize::from(id)].store(b.rises_at().unwrap_or(0), Ordering::Relaxed);
    }

    /// Runs `f` on `id`'s entry (none if their session hasn't begun one, [`Brawls::enter`]), then
    /// keeps when they're down till where the plaza reads it, and moves their version if anything
    /// changed.
    fn with<T>(&self, id: u16, f: impl FnOnce(&mut Entry) -> T) -> Option<T> {
        let mut inner = self.inner.lock().ok()?;
        let e = inner.get_mut(&id)?;
        let before = (e.brawl, e.out, e.help, e.by_name.clone());
        let out = f(e);
        if (e.brawl, e.out, e.help, e.by_name.clone()) != before {
            e.version += 1;
        }
        self.publish(id, &e.brawl);
        Some(out)
    }

    /// `id`'s session began with `hp` kept from when they left, `away` ticks ago, at tick `tick`.
    pub fn enter(&self, id: u16, hp: u8, away: u32, tick: u32) {
        let b = Brawl::resumed(hp, away, tick);
        if let Ok(mut inner) = self.inner.lock() {
            inner.insert(id, Entry::new(b));
        }
        self.publish(id, &b);
    }

    /// `id` came onto a floor at tick `tick` (down a lift, out of an airlock, woken): nobody can hurt
    /// them for a while, unless they strike first.
    pub fn arrive(&self, id: u16, tick: u32) {
        self.with(id, |e| e.brawl.arrive(tick));
    }

    /// `id`'s session is over: forgotten.
    pub fn leave(&self, id: u16) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.remove(&id);
        }
        self.publish(id, &Brawl::default());
    }

    /// `id`'s health at tick `tick` (whole if they're not known).
    pub fn hp(&self, id: u16, tick: u32) -> u8 {
        let Ok(inner) = self.inner.lock() else { return brawl::MAX_HP };
        inner.get(&id).map_or(brawl::MAX_HP, |e| e.brawl.hp(tick))
    }

    /// What `id` is told of their fight at tick `tick`.
    pub fn view(&self, id: u16, tick: u32) -> Option<View> {
        let inner = self.inner.lock().ok()?;
        let e = inner.get(&id)?;
        Some(View {
            hp: e.brawl.hp(tick),
            up_at: e.brawl.up_at(tick),
            by: e.by_name.clone(),
            helper: e.help.map(|(h, _)| h),
            version: e.version,
        })
    }

    /// Whether `id` could throw a blow at tick `tick`.
    pub fn can_strike(&self, id: u16, tick: u32) -> Result<(), Cannot> {
        self.with(id, |e| e.brawl.can_strike(tick)).unwrap_or(Ok(()))
    }

    /// `id` throws a blow at tick `tick` (the cooldown, and a new arrival's shield given up).
    pub fn strike(&self, id: u16, tick: u32) -> Result<(), Cannot> {
        self.with(id, |e| e.brawl.strike(tick)).unwrap_or(Ok(()))
    }

    /// Whether `victim` hurt `id` lately (hitting back at them is self-defence).
    pub fn provoked(&self, id: u16, victim: u16, tick: u32) -> bool {
        let Ok(inner) = self.inner.lock() else { return false };
        inner.get(&id).is_some_and(|e| e.brawl.provoked_by(victim, tick))
    }

    /// Whether `id` can't be hurt at tick `tick` (newly arrived).
    pub fn shielded(&self, id: u16, tick: u32) -> bool {
        let Ok(inner) = self.inner.lock() else { return false };
        inner.get(&id).is_some_and(|e| e.brawl.shielded(tick))
    }

    fn counted(&self, hurt: Hurt) -> Hurt {
        let c = &self.counts;
        match hurt {
            Hurt::Hit(_) => &c.blows,
            Hurt::Down => &c.downs,
            Hurt::Out => &c.outs,
            Hurt::Shielded => &c.refused,
        }
        .fetch_add(1, Ordering::Relaxed);
        hurt
    }

    /// A blow from `by` (called `by_name`) taking `damage` off `id` at tick `tick`.
    pub fn hurt(&self, id: u16, damage: u8, by: u16, by_name: &str, tick: u32) -> Hurt {
        let hurt = self
            .with(id, |e| {
                let hurt = e.brawl.hurt(damage, by, tick);
                Self::took(e, hurt, by_name)
            })
            .unwrap_or(Hurt::Shielded);
        self.counted(hurt)
    }

    /// `id` run down by `by`'s car (called `by_name`) at tick `tick`.
    pub fn run_down(&self, id: u16, by: u16, by_name: &str, tick: u32) -> Hurt {
        let hurt = self
            .with(id, |e| {
                let hurt = e.brawl.run_down(by, tick);
                Self::took(e, hurt, by_name)
            })
            .unwrap_or(Hurt::Shielded);
        self.counted(hurt)
    }

    fn took(e: &mut Entry, hurt: Hurt, by_name: &str) -> Hurt {
        if hurt != Hurt::Shielded {
            e.by_name = Some(by_name.to_string());
            e.help = None;
        }
        if hurt == Hurt::Out {
            e.out = true;
        }
        hurt
    }

    /// A refused blow or car, for `/status`.
    pub fn refused(&self) {
        self.counts.refused.fetch_add(1, Ordering::Relaxed);
    }

    /// `helper` starts lifting `id` up at tick `tick` (`on`), or stops. Whether `id` is down, to be.
    pub fn help(&self, id: u16, helper: u16, on: bool, tick: u32) -> bool {
        self.with(id, |e| {
            if !e.brawl.is_down(tick) {
                e.help = None;
                return false;
            }
            match (on, e.help) {
                (true, Some((h, _))) if h == helper => {}
                (true, _) => e.help = Some((helper, tick)),
                (false, Some((h, _))) if h == helper => e.help = None,
                (false, _) => {}
            }
            true
        })
        .unwrap_or(false)
    }

    /// Who's lifting `id` up, if anyone is.
    pub fn helper_of(&self, id: u16) -> Option<u16> {
        self.inner.lock().ok()?.get(&id)?.help.map(|(h, _)| h)
    }

    /// `id`'s helper `helper`, at tick `tick`, if they're still at it: `near`, whether they're still
    /// beside them. Lifted up once they've been at it long enough: true then.
    pub fn help_tick(&self, id: u16, helper: u16, near: bool, tick: u32) -> bool {
        let helped = self
            .with(id, |e| {
                let (h, since) = e.help?;
                if h != helper {
                    return None;
                }
                if !e.brawl.is_down(tick) || !near {
                    e.help = None;
                    return None;
                }
                if tick.saturating_sub(since) < HELP_TICKS {
                    return None;
                }
                e.help = None;
                e.brawl.help_up(tick).then_some(())
            })
            .flatten()
            .is_some();
        if helped {
            self.counts.helped.fetch_add(1, Ordering::Relaxed);
        }
        helped
    }

    /// Whether `id` was knocked out since last asked (their session carries them up).
    pub fn take_out(&self, id: u16) -> bool {
        self.with(id, |e| std::mem::take(&mut e.out)).unwrap_or(false)
    }

    /// For `/status`: blows landed, pilots knocked down, knocked out and helped up, and blows and
    /// cars refused (no names, no places).
    pub fn counts(&self) -> [u64; 5] {
        let c = &self.counts;
        [&c.blows, &c.downs, &c.outs, &c.helped, &c.refused].map(|n| n.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_proto::presence::RIDE_CAR;
    use bc_sim::colony::brawl::{BLOW, DOWN_TICKS, HELP_TICKS, UP_HP};
    use bc_sim::colony::town;
    use core::f32::consts::FRAC_PI_2;

    /// On the ring's floor `dx` m across and `ds` m round from the step out of Hub Gate's lift,
    /// facing +x.
    fn at(dx: f32, ds: f32) -> PersonPose {
        let (x, t) = town::out_of_the_lift(0);
        let s = town::deck_s(t) + ds;
        PersonPose { strip: RING_DECK, x: x + dx, s, yaw: FRAC_PI_2, grounded: true, ..PersonPose::default() }
    }

    /// Poses heard every fifteenth of a second up to `now`, newest first: `f(k)` for the `k`-th back.
    fn heard(now: Instant, n: usize, f: impl Fn(usize) -> PersonPose) -> Vec<(Instant, PersonPose)> {
        (0..n).map(|k| (now - Duration::from_secs_f32(k as f32 / 15.0), f(k))).collect()
    }

    #[test]
    fn a_striker_stood_where_their_poses_could_have_had_them() {
        let now = Instant::now();
        let mine = heard(now, 6, |k| at(-0.4 * k as f32, 0.0));
        let (x, s) = (mine[0].1.x, mine[0].1.s);
        // Where the last pose has them, or a step on: taken, facing as they say.
        let from = claimed((x + 0.3, s, 1.0), &mine, now).expect("where they were");
        assert_eq!((from.x, from.yaw), (x + 0.3, 1.0));
        // Metres off: no.
        assert_eq!(claimed((x + 4.0, s, 0.0), &mine, now), None);
        // Nothing lately, or only at a wheel: no.
        let old = heard(now - Duration::from_secs(2), 6, |_| at(0.0, 0.0));
        assert_eq!(claimed((x, s, 0.0), &old, now), None);
        let driving = heard(now, 6, |_| PersonPose { ride: RIDE_CAR, ..at(0.0, 0.0) });
        assert_eq!(claimed((driving[0].1.x, s, 0.0), &driving, now), None);
    }

    #[test]
    fn a_blow_lands_on_where_the_other_was_drawn_a_moment_ago() {
        let now = Instant::now();
        // They've run off +x: 1.5 m ahead of the striker a moment ago, 3 m now; their back to them.
        let them = heard(now, 8, |k| at(3.0 - 0.25 * k as f32, 0.0));
        let me = at(0.0, 0.0);
        assert_eq!(lands_on(&me, &them, now), Some(BLOW * 2), "from behind, where they were");
        // Further off all along: missed.
        let gone = heard(now, 8, |_| at(4.0, 0.0));
        assert_eq!(lands_on(&me, &gone, now), None);
    }

    #[test]
    fn a_car_runs_a_pilot_down_only_by_its_own_poses() {
        let now = Instant::now();
        let car = |dx: f32| PersonPose { ride: RIDE_CAR, speed: 2.0, ..at(dx, 0.0) };
        // At 15 m/s along +x, its middle 1.5 m short of them now.
        let driver = heard(now, 9, |k| car(-1.5 - k as f32));
        let victim = heard(now, 9, |_| at(0.0, 0.0));
        assert_eq!(runs_down(&driver, &victim, now), Ok(()), "the pose says 2 m/s; its poses say 15");
        // Too slow by its poses (whatever they say).
        let crawl = heard(now, 9, |k| PersonPose { speed: 12.0, ..car(-1.5 - 0.1 * k as f32) });
        assert_eq!(runs_down(&crawl, &victim, now), Err("too slow"));
        // Poses held back to lunge: refused.
        let gappy: Vec<_> =
            driver.iter().copied().enumerate().filter(|(k, _)| *k == 0 || *k > 5).map(|(_, p)| p).collect();
        assert_eq!(runs_down(&gappy, &victim, now), Err("too far between"));
        // Nobody near, or not at a wheel.
        let far = heard(now, 9, |_| at(0.0, 12.0));
        assert_eq!(runs_down(&driver, &far, now), Err("nobody there"));
        let walking = heard(now, 9, |k| at(-1.5 - k as f32, 0.0));
        assert_eq!(runs_down(&walking, &victim, now), Err("you're not at a wheel"));
    }

    #[test]
    fn the_registry_downs_helps_up_knocks_out_and_says_so_without_a_lock() {
        let b = Brawls::default();
        b.enter(3, 100, 0, 1_000);
        b.enter(4, 100, 0, 1_000);
        assert_eq!(b.hurt(3, BLOW, 4, "Ines", 1_010), Hurt::Hit(100 - BLOW));
        assert!(b.provoked(3, 4, 1_020) && !b.provoked(4, 3, 1_020));
        assert_eq!(b.run_down(3, 4, "Ines", 1_020), Hurt::Down);
        assert!(b.is_down(3, 1_021) && !b.is_down(3, 1_020 + DOWN_TICKS));
        assert_eq!(b.rose_at(3), Some(1_020 + DOWN_TICKS));
        let v = b.view(3, 1_030).unwrap();
        assert_eq!((v.hp, v.up_at, v.by.as_deref()), (0, Some(1_020 + DOWN_TICKS), Some("Ines")));
        // A friend at their side: up after a while, not before; gone off, it stops.
        assert!(b.help(3, 5, true, 1_100));
        assert_eq!(b.helper_of(3), Some(5));
        assert!(!b.help_tick(3, 5, true, 1_100 + HELP_TICKS - 1));
        assert!(!b.help_tick(3, 5, false, 1_101 + HELP_TICKS - 1));
        assert_eq!(b.helper_of(3), None);
        assert!(b.help(3, 5, true, 1_200));
        assert!(b.help_tick(3, 5, true, 1_200 + HELP_TICKS));
        assert!(!b.is_down(3, 1_200 + HELP_TICKS) && b.hp(3, 1_200 + HELP_TICKS) == UP_HP);
        assert_eq!(b.rose_at(3), Some(1_200 + HELP_TICKS));
        // Up again lately, a lethal blow knocks them out: their session's to carry them up, once.
        assert_eq!(b.hurt(3, BLOW * 2, 4, "Ines", 1_300 + HELP_TICKS), Hurt::Out);
        assert!(b.take_out(3) && !b.take_out(3));
        assert_eq!(b.counts(), [1, 1, 1, 1, 0]);
        b.leave(3);
        assert!(!b.is_down(3, 0) && b.rose_at(3).is_none());
    }
}
