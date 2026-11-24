//! Pilots on foot who fight (the owner's "open season" on foot, `docs/DECISIONS.md`;
//! `docs/OPEN_CITY.md`, "Heat and the Watch": people can be hurt): a pilot's health, the blow that
//! lands, being knocked down and getting up, being knocked out, and healing.
//!
//! A closed form of the tick, like the Watch's heat ([`super::watch::Heat`]): the server keeps one
//! [`Brawl`] for each pilot on foot (`bc-server`'s `brawl`), off the sector's tick for now and ready
//! to move into the interior sector's once people on foot are its entities (`OPEN_CITY.md`, S1).
//! [`lands`] is one function for the client's prompt and the server's judgement, so the prompt never
//! offers a blow the server refuses. Nothing here allocates or keeps a clock of its own, and its trig
//! is libm's (`crate::math`).
//!
//! - **Health:** [`MAX_HP`]. A blow takes [`BLOW`] (from behind, [`BEHIND`] times it). At none, the
//!   pilot is down for [`DOWN_TICKS`], then up with [`UP_HP`]; a friend lifts them sooner, holding
//!   beside them for [`HELP_TICKS`] within [`HELP_REACH`].
//! - **Out:** a lethal blow on a pilot already down, or within [`REWOUND_TICKS`] of getting up (no
//!   loops of down and up), knocks them out: carried up to their bay ([`OUT_HP`] when they wake).
//! - **Healing:** unhurt for [`HEAL_AFTER_TICKS`], a point every [`HEAL_TICKS`].
//! - **New arrivals** can't be hurt for [`SHIELD_TICKS`] (down a lift, out of an airlock, woken), unless
//!   they strike first; and a blow at whoever hurt you in the last [`PAYBACK_TICKS`] is self-defence
//!   ([`Brawl::provoked_by`]: the Watch doesn't mind it).

use bc_proto::presence::{BASE_DECK, PersonPose, RING_DECK};
use glam::Vec3;

use super::city::{self, Stage};
use super::deck::{self, BASE, RING};
use crate::TICK_HZ;
use crate::math::{cos, hypot, sin};

/// A pilot's health, whole.
pub const MAX_HP: u8 = 100;
/// What a blow takes (the owner's knob: a punch, 5 blows to put someone down from whole).
pub const BLOW: u8 = 20;
/// A blow from behind is worth this many: the one struck faces away from the one striking, within
/// [`BEHIND_COS`] of straight away.
pub const BEHIND: u8 = 2;
pub const BEHIND_COS: f32 = 0.5;
/// How far a blow reaches, m, middle to middle (oneshot's `MELEE_RANGE`).
pub const REACH: f32 = 2.0;
/// Inside this, m, a blow lands whichever way the striker faces (they're chest to chest).
pub const CLOSE: f32 = 0.35;
/// How far off the way the striker faces a blow still lands: its cosine (about 35°).
pub const CONE_COS: f32 = 0.82;
/// How far apart in height two pilots can be and still trade blows, m (not a storey apart).
pub const HEIGHT: f32 = 1.2;
/// A pilot strikes at most once in this, ticks (0.8 s, oneshot's `MELEE_COOL_MS`).
pub const COOL_TICKS: u32 = TICK_HZ * 4 / 5;
/// How long a pilot knocked down stays down, ticks (the owner's knob: 25 s, as the town's people do).
pub const DOWN_TICKS: u32 = 25 * TICK_HZ;
/// What a pilot gets up with.
pub const UP_HP: u8 = 30;
/// A lethal blow this soon after getting up knocks them out, ticks (Gates' `REWOUND`).
pub const REWOUND_TICKS: u32 = 60 * TICK_HZ;
/// What a pilot knocked out wakes in their bay with (the bay patches them up).
pub const OUT_HP: u8 = MAX_HP;
/// Newly on the floor, nobody can hurt them for this long, ticks, unless they strike first (the
/// owner's knob: 10 s).
pub const SHIELD_TICKS: u32 = 10 * TICK_HZ;
/// A blow at whoever hurt you this recently is self-defence, ticks.
pub const PAYBACK_TICKS: u32 = 15 * TICK_HZ;
/// Unhurt this long, a pilot starts to heal, ticks; then a point each [`HEAL_TICKS`].
pub const HEAL_AFTER_TICKS: u32 = 30 * TICK_HZ;
pub const HEAL_TICKS: u32 = TICK_HZ;
/// Helping someone up: held this long, ticks, beside them within this far, m (Gates' `assist`).
pub const HELP_TICKS: u32 = 6 * TICK_HZ;
pub const HELP_REACH: f32 = 3.0;

/// How `a` and `b` lie apart on their floor: across (or along, on a strip) and round it, m (round
/// the ring's floor the short way).
fn apart(a: &PersonPose, b: &PersonPose) -> (f32, f32) {
    let ds = match a.strip {
        RING_DECK => deck::round_from(RING, a.s, b.s),
        BASE_DECK => deck::round_from(BASE, a.s, b.s),
        _ => b.s - a.s,
    };
    (b.x - a.x, ds)
}

/// The way a pose faces over the ground, `(x, s)` (the walker's yaw: 0 faces −s, τ/4 faces +x).
pub fn facing(yaw: f32) -> (f32, f32) {
    (sin(yaw), -cos(yaw))
}

/// Whether something of the floor `p` is on stands between `p` and `(dx, ds)` from it, at chest
/// height: a wall, a building, a door.
fn walled(p: &PersonPose, dx: f32, ds: f32) -> bool {
    let far = hypot(dx, ds);
    let steps = (far / 0.4) as u32 + 1;
    let (lo, hi, e) = (p.h + 1.1, p.h + 1.4, 0.05);
    (1..steps).any(|k| {
        let u = k as f32 / steps as f32;
        let (x, s) = (p.x + dx * u, p.s + ds * u);
        let (min, max) = (Vec3::new(x - e, lo, -s - e), Vec3::new(x + e, hi, -s + e));
        match p.strip {
            RING_DECK => deck::solid(RING, min, max),
            BASE_DECK => deck::solid(BASE, min, max),
            // (As a person meets the city: a home's own walls inside it, and its furniture.)
            strip => city::solid(strip, min, max, Stage(0)),
        }
    })
}

/// Whether a blow thrown from `from` lands on whoever is at `on`, and what it takes: on the same
/// floor, within [`REACH`] (and [`HEIGHT`]), within [`CONE_COS`] of the way `from` faces (inside
/// [`CLOSE`], any way), with no wall between at chest height; from behind, [`BEHIND`] times
/// [`BLOW`]. What the client's prompt and the server's judgement both ask. Who may be struck at all
/// (on foot, not newly arrived) is [`Brawl`]'s and the server's.
pub fn lands(from: &PersonPose, on: &PersonPose) -> Option<u8> {
    if from.strip != on.strip || (on.h - from.h).abs() > HEIGHT {
        return None;
    }
    let (dx, ds) = apart(from, on);
    let far = hypot(dx, ds);
    if far > REACH {
        return None;
    }
    let (fx, fs) = facing(from.yaw);
    if far > CLOSE && (dx * fx + ds * fs) / far < CONE_COS {
        return None;
    }
    if walled(from, dx, ds) {
        return None;
    }
    // Their back to the one striking: they face along the line from them.
    let (vx, vs) = facing(on.yaw);
    let behind = far > CLOSE && (dx * vx + ds * vs) / far > BEHIND_COS;
    Some(if behind { BLOW * BEHIND } else { BLOW })
}

/// Whether a helper at `from` is near enough to lift someone at `on`: on the same floor, within
/// [`HELP_REACH`] (and [`HEIGHT`]).
pub fn helps(from: &PersonPose, on: &PersonPose) -> bool {
    let (dx, ds) = apart(from, on);
    from.strip == on.strip && (on.h - from.h).abs() <= HEIGHT && hypot(dx, ds) <= HELP_REACH
}

/// Why a pilot's own blow can't be thrown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cannot {
    /// They're down.
    Down,
    /// Too soon after their last ([`COOL_TICKS`]).
    Cooling,
}

/// What a blow (or a car) did to the one it reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hurt {
    /// Hurt, still up, with this much left.
    Hit(u8),
    /// Knocked down (for [`DOWN_TICKS`]).
    Down,
    /// Knocked out: carried up to their bay.
    Out,
    /// Newly arrived: untouched ([`SHIELD_TICKS`]).
    Shielded,
}

/// One pilot's health and their fight's state, on the tick's clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Brawl {
    hp: u8,
    /// The tick their healing is counted to.
    at: u32,
    /// The tick they were last hurt (or got up): healing waits [`HEAL_AFTER_TICKS`] from it.
    hurt_at: Option<u32>,
    /// Down till this tick.
    down_until: Option<u32>,
    /// The tick they last got up.
    up_at: Option<u32>,
    /// The tick of their last blow.
    struck_at: Option<u32>,
    /// Untouchable till this tick (newly arrived), unless they strike first.
    shield_until: u32,
    /// Who last hurt them (the keeper's id for them: the server's session), and when.
    by: Option<(u16, u32)>,
}

impl Default for Brawl {
    /// Whole.
    fn default() -> Self {
        Self {
            hp: MAX_HP,
            at: 0,
            hurt_at: None,
            down_until: None,
            up_at: None,
            struck_at: None,
            shield_until: 0,
            by: None,
        }
    }
}

impl Brawl {
    /// With `hp` kept from when they left, `away` ticks before tick `tick`: healed as that time
    /// unhurt would have (a reconnect a moment later heals nothing).
    pub fn resumed(hp: u8, away: u32, tick: u32) -> Self {
        let then = tick.saturating_sub(away);
        let mut b = Self { hp: hp.clamp(1, MAX_HP), at: then, hurt_at: Some(then), ..Self::default() };
        b.settle(tick);
        b
    }

    /// Knocked out at tick `tick`: what they wake in their bay as.
    pub fn woke_out() -> Self {
        Self { hp: OUT_HP, ..Self::default() }
    }

    /// Runs the clock to `tick`: up again once their time down is over, and healed for the time
    /// unhurt since it was last counted.
    fn settle(&mut self, tick: u32) {
        if let Some(u) = self.down_until {
            if tick < u {
                return;
            }
            self.down_until = None;
            self.up_at = Some(u);
            self.hurt_at = Some(u);
            self.hp = UP_HP;
            self.at = u;
        }
        let from = self.hurt_at.map_or(self.at, |h| self.at.max(h.saturating_add(HEAL_AFTER_TICKS)));
        if tick <= from || self.hp >= MAX_HP {
            self.at = self.at.max(tick);
            return;
        }
        let gained = (tick - from) / HEAL_TICKS;
        let hp = u32::from(self.hp) + gained;
        if hp >= u32::from(MAX_HP) {
            self.hp = MAX_HP;
            self.at = tick;
        } else {
            self.hp = hp as u8;
            self.at = from + gained * HEAL_TICKS;
        }
    }

    /// As at tick `tick`.
    fn at_tick(&self, tick: u32) -> Self {
        let mut b = *self;
        b.settle(tick);
        b
    }

    /// Their health at tick `tick` (0 while down).
    pub fn hp(&self, tick: u32) -> u8 {
        let b = self.at_tick(tick);
        if b.down_until.is_some() { 0 } else { b.hp }
    }

    /// Whether they're down at tick `tick`.
    pub fn is_down(&self, tick: u32) -> bool {
        self.down_until.is_some_and(|u| tick < u)
    }

    /// The tick they get up at, if they're down at tick `tick`.
    pub fn up_at(&self, tick: u32) -> Option<u32> {
        self.down_until.filter(|u| tick < *u)
    }

    /// The tick they last got up, or get up at if they're down (none: never down).
    pub fn rises_at(&self) -> Option<u32> {
        self.down_until.or(self.up_at)
    }

    /// Whether nobody can hurt them at tick `tick` (newly arrived).
    pub fn shielded(&self, tick: u32) -> bool {
        tick < self.shield_until
    }

    /// Newly on the floor at tick `tick`: shielded for [`SHIELD_TICKS`].
    pub fn arrive(&mut self, tick: u32) {
        self.shield_until = tick.saturating_add(SHIELD_TICKS);
    }

    /// Whether `id` hurt them within [`PAYBACK_TICKS`] of tick `tick`: hitting back at them is
    /// self-defence.
    pub fn provoked_by(&self, id: u16, tick: u32) -> bool {
        self.by.is_some_and(|(by, t)| by == id && tick.saturating_sub(t) <= PAYBACK_TICKS)
    }

    /// Who last hurt them, and when.
    pub fn hurt_by(&self) -> Option<(u16, u32)> {
        self.by
    }

    /// Whether they could throw a blow at tick `tick`.
    pub fn can_strike(&self, tick: u32) -> Result<(), Cannot> {
        if self.is_down(tick) {
            return Err(Cannot::Down);
        }
        if self.struck_at.is_some_and(|t| tick.saturating_sub(t) < COOL_TICKS) {
            return Err(Cannot::Cooling);
        }
        Ok(())
    }

    /// They throw a blow at tick `tick` (it lands or not): the cooldown starts, and striking first
    /// gives up a new arrival's shield.
    pub fn strike(&mut self, tick: u32) -> Result<(), Cannot> {
        self.can_strike(tick)?;
        self.settle(tick);
        self.struck_at = Some(tick);
        self.shield_until = 0;
        Ok(())
    }

    /// A lethal blow at tick `tick`: down, or out if they're down already or got up only lately.
    fn lethal(&mut self, by: u16, tick: u32) -> Hurt {
        self.by = Some((by, tick));
        let rewound = self.up_at.is_some_and(|u| tick.saturating_sub(u) < REWOUND_TICKS);
        if self.is_down(tick) || rewound {
            *self = Self::woke_out();
            return Hurt::Out;
        }
        self.hp = 0;
        self.hurt_at = Some(tick);
        self.at = tick;
        self.down_until = Some(tick.saturating_add(DOWN_TICKS));
        Hurt::Down
    }

    /// A blow from `by` taking `damage` at tick `tick`.
    pub fn hurt(&mut self, damage: u8, by: u16, tick: u32) -> Hurt {
        self.settle(tick);
        if self.shielded(tick) {
            return Hurt::Shielded;
        }
        if self.is_down(tick) || damage >= self.hp {
            return self.lethal(by, tick);
        }
        self.hp -= damage;
        self.hurt_at = Some(tick);
        self.at = tick;
        self.by = Some((by, tick));
        Hurt::Hit(self.hp)
    }

    /// Run down by `by`'s car at tick `tick`: knocked down whatever their health (out if down
    /// already, or up only lately).
    pub fn run_down(&mut self, by: u16, tick: u32) -> Hurt {
        self.settle(tick);
        if self.shielded(tick) {
            return Hurt::Shielded;
        }
        self.lethal(by, tick)
    }

    /// Lifted up by a friend at tick `tick`, if they were down: up with [`UP_HP`].
    pub fn help_up(&mut self, tick: u32) -> bool {
        if !self.is_down(tick) {
            return false;
        }
        self.down_until = None;
        self.up_at = Some(tick);
        self.hurt_at = Some(tick);
        self.hp = UP_HP;
        self.at = tick;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f32::consts::{FRAC_PI_2, PI};

    /// On the ring's floor at `(x, s)`, facing `yaw`.
    fn at(x: f32, s: f32, yaw: f32) -> PersonPose {
        PersonPose { strip: RING_DECK, x, s, yaw, grounded: true, ..PersonPose::default() }
    }

    /// Somewhere open on the ring's floor: in Hub Gate's square, a spot near its lift's door with
    /// nothing within 4 m of it at chest height.
    fn square() -> (f32, f32) {
        let (x0, t) = super::super::town::out_of_the_lift(0);
        let s0 = super::super::town::deck_s(t);
        let open = |x: f32, s: f32| {
            !deck::solid(RING, Vec3::new(x - 4.0, 0.5, -s - 4.0), Vec3::new(x + 4.0, 1.8, -s + 4.0))
        };
        (0..60)
            .flat_map(|i| (0..60).map(move |j| (x0 - 60.0 + 2.0 * i as f32, s0 - 60.0 + 2.0 * j as f32)))
            .find(|&(x, s)| open(x, s))
            .expect("open ground in Hub Gate's square")
    }

    #[test]
    fn a_blow_lands_in_front_within_reach_and_doubles_from_behind() {
        let (x, s) = square();
        // Facing +x (τ/4), the other 1.5 m ahead and facing back at them: a blow.
        let me = at(x, s, FRAC_PI_2);
        assert_eq!(lands(&me, &at(x + 1.5, s, -FRAC_PI_2)), Some(BLOW));
        // Their back to the one striking: twice it.
        assert_eq!(lands(&me, &at(x + 1.5, s, FRAC_PI_2)), Some(BLOW * BEHIND));
        // Out of reach, out of the cone, behind the one striking, a storey up, on another floor.
        assert_eq!(lands(&me, &at(x + REACH + 0.2, s, 0.0)), None);
        assert_eq!(lands(&me, &at(x + 1.0, s + 1.2, 0.0)), None, "off to the side");
        assert_eq!(lands(&me, &at(x - 1.5, s, 0.0)), None, "behind them");
        assert_eq!(lands(&me, &PersonPose { h: 3.0, ..at(x + 1.5, s, 0.0) }), None);
        assert_eq!(lands(&me, &PersonPose { strip: BASE_DECK, ..at(x + 1.5, s, 0.0) }), None);
        // Chest to chest, any way.
        assert!(lands(&me, &at(x - 0.2, s, 0.0)).is_some());
        // Round the ring's seam, the short way.
        let round = deck::DECKS[RING as usize].round();
        let (dx, ds) = apart(&at(x, 0.5, 0.0), &at(x + 0.3, round - 0.7, PI));
        assert!((dx - 0.3).abs() < 1e-3 && (ds + 1.2).abs() < 1e-2, "{dx} {ds}");
    }

    #[test]
    fn no_blow_lands_through_a_wall() {
        // Across the ring's floor from Hub Gate's square: a clear spot with a building's wall a
        // pace ahead at chest height.
        let (x0, s) = square();
        let clear =
            |x: f32| !deck::solid(RING, Vec3::new(x - 0.3, 0.1, -s - 0.3), Vec3::new(x + 0.3, 1.8, -s + 0.3));
        let x = (0..8_000)
            .map(|k| x0 + k as f32 * 0.25)
            .find(|x| clear(*x) && clear(*x - 1.5) && walled(&at(*x, s, 0.0), 1.5, 0.0))
            .expect("a wall across the town from Hub Gate");
        assert_eq!(lands(&at(x, s, FRAC_PI_2), &at(x + 1.5, s, -FRAC_PI_2)), None);
        // The same blow the other way, into the open, lands.
        assert!(!walled(&at(x, s, 0.0), -1.5, 0.0));
        assert_eq!(lands(&at(x, s, -FRAC_PI_2), &at(x - 1.5, s, FRAC_PI_2)), Some(BLOW));
    }

    #[test]
    fn five_blows_put_someone_down_and_they_get_up_in_time() {
        let mut b = Brawl::default();
        let mut t = 100;
        for k in 1..BLOW_COUNT {
            assert_eq!(b.hurt(BLOW, 7, t), Hurt::Hit(MAX_HP - BLOW * k as u8));
            t += 10;
        }
        assert_eq!(b.hurt(BLOW, 7, t), Hurt::Down);
        assert!(b.is_down(t) && b.hp(t) == 0);
        assert_eq!(b.can_strike(t), Err(Cannot::Down));
        assert_eq!(b.up_at(t), Some(t + DOWN_TICKS));
        assert!(b.provoked_by(7, t + PAYBACK_TICKS) && !b.provoked_by(7, t + PAYBACK_TICKS + 1));
        assert!(!b.provoked_by(8, t));
        // Up when their time's over, with a little.
        assert!(b.is_down(t + DOWN_TICKS - 1));
        assert!(!b.is_down(t + DOWN_TICKS));
        assert_eq!(b.hp(t + DOWN_TICKS), UP_HP);
        // A lethal blow soon after: out.
        let up = t + DOWN_TICKS;
        assert_eq!(b.hurt(BLOW * 2, 7, up + 30), Hurt::Out);
        assert_eq!(b, Brawl::woke_out());
    }

    /// How many plain blows put someone whole down.
    const BLOW_COUNT: u32 = (MAX_HP / BLOW) as u32;

    #[test]
    fn a_blow_on_someone_down_or_a_car_knocks_them_out() {
        let mut b = Brawl::default();
        assert_eq!(b.run_down(3, 10), Hurt::Down);
        assert_eq!(b.hurt(BLOW, 4, 20), Hurt::Out);
        assert_eq!(b.hp(20), OUT_HP);
        let mut c = Brawl::default();
        assert_eq!(c.run_down(3, 10), Hurt::Down);
        assert_eq!(c.run_down(3, 20), Hurt::Out);
        // Down and lifted by a friend: up, and a lethal blow a while later is down again, not out.
        let mut d = Brawl::default();
        d.run_down(3, 10);
        assert!(d.help_up(10 + HELP_TICKS));
        assert!(!d.help_up(10 + HELP_TICKS), "only once");
        assert_eq!(d.hp(10 + HELP_TICKS), UP_HP);
        let later = 10 + HELP_TICKS + REWOUND_TICKS;
        assert_eq!(d.run_down(3, later), Hurt::Down);
    }

    #[test]
    fn they_heal_unhurt_and_a_reconnect_heals_nothing() {
        let mut b = Brawl::default();
        b.hurt(BLOW * 3, 2, 0);
        assert_eq!(b.hp(HEAL_AFTER_TICKS), MAX_HP - BLOW * 3);
        assert_eq!(b.hp(HEAL_AFTER_TICKS + 5 * HEAL_TICKS), MAX_HP - BLOW * 3 + 5);
        // Hurt again: the wait starts over.
        b.hurt(1, 2, HEAL_AFTER_TICKS + 5 * HEAL_TICKS);
        let t = HEAL_AFTER_TICKS + 5 * HEAL_TICKS;
        assert_eq!(b.hp(t + HEAL_AFTER_TICKS - 1), MAX_HP - BLOW * 3 + 4);
        assert_eq!(b.hp(u32::MAX / 2), MAX_HP);
        // Kept over a logout: a moment away heals nothing; a long one, everything.
        let r = Brawl::resumed(40, 2 * TICK_HZ, 10_000);
        assert_eq!(r.hp(10_000), 40);
        assert_eq!(Brawl::resumed(40, 3_600 * TICK_HZ, 200_000).hp(200_000), MAX_HP);
        assert_eq!(Brawl::resumed(0, 0, 5).hp(5), 1, "never kept at nothing");
    }

    #[test]
    fn a_new_arrival_is_shielded_until_they_strike_and_blows_cool() {
        let mut b = Brawl::default();
        b.arrive(100);
        assert_eq!(b.hurt(BLOW, 1, 101), Hurt::Shielded);
        assert_eq!(b.run_down(1, 101), Hurt::Shielded);
        assert_eq!(b.hp(101), MAX_HP);
        assert!(!b.shielded(100 + SHIELD_TICKS));
        // Striking first gives it up.
        let mut c = Brawl::default();
        c.arrive(100);
        assert_eq!(c.strike(110), Ok(()));
        assert!(!c.shielded(111));
        assert_eq!(c.hurt(BLOW, 1, 111), Hurt::Hit(MAX_HP - BLOW));
        // And a pilot strikes at most once in a while.
        assert_eq!(c.strike(110 + COOL_TICKS - 1), Err(Cannot::Cooling));
        assert_eq!(c.strike(110 + COOL_TICKS), Ok(()));
    }

    #[test]
    fn a_friend_helps_from_near_enough() {
        let (x, s) = square();
        assert!(helps(&at(x, s, 0.0), &at(x + 2.5, s, 0.0)));
        assert!(!helps(&at(x, s, 0.0), &at(x + HELP_REACH + 0.1, s, 0.0)));
    }
}
