//! The colony's register of homes (`docs/OPEN_CITY.md`, H1): who holds which flat or house, and
//! what its door and its panel say. The homes themselves are closed forms of the city
//! (`bc_sim::colony::homes`); this holds only what pilots did to them.
//!
//! - **Leases and deeds.** A flat is rented: its rent is paid ahead into its panel, which spends it
//!   by the hour (Rust's tool cupboard and its upkeep; Gates' hearth). Rent unpaid a day
//!   ([`GRACE_S`]) and the lease lapses back to the colony. A house, or a flat on its building's top
//!   storey, is bought, and sold back to the colony for most of its price; one left unvisited a
//!   month ([`ABANDONED_S`]) is sold back for its owner. Whatever a lapsed or sold home held waits
//!   for its holder in their bay.
//! - **The lock** is Gates' code lock (`sim-core/src/lock.rs`), as it is: a main code and a guest
//!   code, four digits each. The right main code puts you on the home's **crew** (who may open its
//!   door, use its panel and what's inside); the guest code, its door alone. Setting the main code
//!   clears both lists, keeping whoever set it and the holder. Wrong codes shock (more for each in a
//!   row, forgiven after [`SHOCK_RESET_S`]), and [`LOCKOUT_TRIES`] in a row shut the keypad for
//!   [`LOCKOUT_S`]. A new lease's door has the colony's key lock: its holder alone, until they set a
//!   code. The lists are capped and refuse newcomers when full, never forgetting anyone (Gates'
//!   `roster.rs`), and no pilot's key ever goes on the wire to build them: you can only add
//!   yourself.
//! - **The door** is open or shut; whoever passes its lock opens and shuts it, so nobody is shut
//!   in. Unlocked, anyone passes. A knock is heard by the home's crew wherever they are.
//! - **The rent's group tax:** a crew of more than four makes the rent dearer (Gates' `group_pct`,
//!   `content::homes::hourly`), so a flat stays a home.
//!
//! The ledger: the colony takes the rent as it's spent and a home's price, and pays back what it
//! buys back; nothing else makes or loses a credit here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::exchange::{Account, Trader};
use crate::hangar::{Done, Hangar};
use crate::item::{Item, thousands};
use bc_sim::colony::homes::{Home, HomeId, home, plan_at};
use bc_sim::content::homes::{
    ABANDONED_S, GRACE_S, HOUR_S, MAX_HELD, bought, hourly, price, rent, sell_back,
};
use bc_sim::content::homes::{FURNITURE_BACK_PCT, Furniture, MAX_PIECES, fits, piece_box};

/// Wrong codes: the shock's step, hp (more for each in a row), how long until the streak's
/// forgiven, s; how many in a row shut the keypad, and for how long, s (Gates' numbers).
pub const SHOCK_STEP_HP: u16 = 5;
pub const SHOCK_RESET_S: u64 = 10;
pub const LOCKOUT_TRIES: u8 = 8;
pub const LOCKOUT_S: u64 = 15 * 60;
/// The most a home's crew lists (full rights), and its guests.
pub const CREW_CAP: usize = 10;
pub const GUEST_CAP: usize = 8;
/// The longest lease paid at once, weeks.
pub const MAX_WEEKS: u64 = 8;
/// Knocks kept for the crews to hear.
const KNOCKS: usize = 32;

/// A four-digit code, `0000` to `9999`.
pub const CODE_MAX: u16 = 9_999;

/// How a home is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tenure {
    /// Rented: what its panel holds to pay the rent with, cr; until when it's been charged (unix
    /// seconds); and since when it's been unpaid, if it has.
    Lease { bank: u64, charged_to: u64, unpaid_since: Option<u64> },
    /// Bought, for `paid` cr; its owner last seen in it at `seen`.
    Deed { paid: u64, seen: u64 },
}

/// A home's lock: Gates' code lock.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lock {
    pub code: Option<u16>,
    pub guest_code: Option<u16>,
    /// Whether the door answers to the lists at all: unlocked, anyone passes.
    pub locked: bool,
    /// The crew: full rights (the holder always among them).
    pub crew: Vec<Trader>,
    pub guests: Vec<Trader>,
    misses: u8,
    last_miss: u64,
    shut_until: u64,
}

impl Lock {
    /// The colony's key lock on a new home: locked, its holder alone.
    fn key(holder: &str) -> Self {
        Self { locked: true, crew: vec![holder.to_string()], ..Self::default() }
    }

    pub fn is_crew(&self, who: &str) -> bool {
        self.crew.iter().any(|c| c == who)
    }

    /// Whether `who` may work the door.
    pub fn passes(&self, who: &str) -> bool {
        !self.locked || self.is_crew(who) || self.guests.iter().any(|g| g == who)
    }
}

/// A home someone holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Held {
    pub holder: Trader,
    /// The name its holder went by (on its door).
    pub name: String,
    pub tenure: Tenure,
    pub lock: Lock,
    /// Its door: open, or shut.
    pub open: bool,
    /// Since when it's been held (unix seconds).
    pub since: u64,
    /// Its furniture (H2), and the id the next piece gets.
    #[serde(default)]
    pub pieces: Vec<Piece>,
    #[serde(default)]
    pub next_piece: u16,
    /// Who's asleep in its beds (H3): logged out at home, and drawn lying there until they're back.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asleep: Vec<Asleep>,
}

/// Someone asleep in a home's bed: which bed (its piece), whose record, and the name they went by.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asleep {
    pub piece: u16,
    pub who: Trader,
    pub name: String,
}

/// A piece of furniture in a home: which, where its middle is (dm from the home's floor's −s −x
/// corner), turned how many quarter turns, on which storey (a house's), and what it holds (a
/// chest's, a safe's).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Piece {
    pub id: u16,
    #[serde(with = "furniture_serde")]
    pub kind: Furniture,
    pub s: u16,
    pub x: u16,
    pub turn: u8,
    #[serde(default)]
    pub storey: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holds: Vec<(Item, u64)>,
}

impl Piece {
    /// Its middle, m from the home's floor's −s −x corner.
    pub fn at(&self) -> (f32, f32) {
        (f32::from(self.s) / 10.0, f32::from(self.x) / 10.0)
    }
}

/// Serde for a piece of furniture, as its slug (`bed`, `sofa`…).
pub mod furniture_serde {
    use bc_sim::content::homes::Furniture;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(f: &Furniture, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(f.spec().slug)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Furniture, D::Error> {
        let slug = String::deserialize(d)?;
        Furniture::from_slug(&slug).ok_or_else(|| D::Error::custom(format!("no furniture {slug}")))
    }
}

/// What a code entered at a door did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entered {
    /// The main code: on the crew now.
    Crew,
    /// The guest code: the door works for them.
    Guest,
    /// Wrong: a shock of `shock` hp; `shut` if it shut the keypad.
    Wrong { shock: u16, shut: bool },
    /// The keypad is shut: not even tried.
    Shut,
}

/// A knock at a home's door: which, by whom (the name they go by), and its place in the knocks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Knock {
    pub seq: u64,
    pub home: u32,
    pub by: String,
}

/// The register.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Homes {
    held: BTreeMap<u32, Held>,
    /// What's waiting for pilots in their bays: a lapsed home's rent handed back, a home's price
    /// paid back, and the news of it.
    accounts: BTreeMap<Trader, Account>,
    #[serde(skip)]
    knocks: Vec<Knock>,
    #[serde(skip)]
    knock_seq: u64,
    /// The ledger: rent the colony has taken, what it's sold homes for, and paid back for them.
    pub rent_taken: u64,
    pub sold_for: u64,
    pub paid_back: u64,
    /// What furniture has sold for, and what the colony's paid back for it.
    #[serde(default)]
    pub furniture_sold: u64,
    #[serde(default)]
    pub furniture_back: u64,
}

fn refuse<T>(why: impl Into<String>) -> Result<T, String> {
    Err(why.into())
}

/// A home's address as its door has it: "FLAT 6B, ARRIVAL HEIGHTS" or "14 OLD TOWN".
pub fn address(h: &Home) -> String {
    let district = bc_sim::colony::city::district_of(h.id.strip(), h.id.bx(), bc_sim::colony::city::Stage(0))
        .map_or("THE CITY", |(d, _)| {
            bc_sim::content::city::DISTRICT_NAMES[h.id.strip() as usize % 3][d as usize]
        });
    // A building's number on its row, from its block and lot.
    let number = (h.id.bx() - bc_sim::colony::city::CITY.0).max(0) * 9 + i32::from(h.id.lot()) + 1;
    if h.id.house() {
        format!("{} {} ROW · HOUSE {}", number, district, h.id.unit() + 1)
    } else {
        let floor = if h.id.storey() == 0 { "G".to_string() } else { h.id.storey().to_string() };
        let letter = (b'A' + h.id.unit() as u8) as char;
        format!("FLAT {floor}{letter} · {number} {district}")
    }
}

impl Homes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, id: HomeId) -> Option<&Held> {
        self.held.get(&id.0)
    }

    pub fn all(&self) -> impl Iterator<Item = (HomeId, &Held)> {
        self.held.iter().map(|(k, v)| (HomeId(*k), v))
    }

    /// The homes `who` holds.
    pub fn of(&self, who: &str) -> Vec<HomeId> {
        self.held.iter().filter(|(_, h)| h.holder == who).map(|(k, _)| HomeId(*k)).collect()
    }

    /// Whether `who` may go into home `id`: one nobody holds is open to see round; a held one, if
    /// its door's open or its lock lets them by.
    pub fn may_enter(&self, id: HomeId, who: &str) -> bool {
        self.held.get(&id.0).is_none_or(|h| h.open || h.lock.passes(who))
    }

    fn held_by(&mut self, id: HomeId, who: &str) -> Result<&mut Held, String> {
        match self.held.get_mut(&id.0) {
            Some(h) if h.holder == who => Ok(h),
            Some(_) => refuse("that home isn't yours"),
            None => refuse("nobody holds that home"),
        }
    }

    fn crew_of(&mut self, id: HomeId, who: &str) -> Result<&mut Held, String> {
        match self.held.get_mut(&id.0) {
            Some(h) if h.lock.is_crew(who) => Ok(h),
            Some(_) => refuse("you're not on its panel's crew"),
            None => refuse("nobody holds that home"),
        }
    }

    fn can_take(&self, home: &Home, who: &str) -> Result<(), String> {
        if who.is_empty() || who.starts_with("guest-") {
            return refuse("a home needs a pilot's record: sign in with a wallet");
        }
        if self.held.contains_key(&home.id.0) {
            return refuse("someone holds that home already");
        }
        if self.of(who).len() >= MAX_HELD {
            return refuse(format!("you hold {MAX_HELD} homes already"));
        }
        Ok(())
    }

    /// `who` (going by `name`) rents `home` for `weeks`, paid from their hangar ahead into its panel.
    pub fn rent(
        &mut self,
        home: &Home,
        who: &str,
        name: &str,
        weeks: u64,
        hangar: &mut Hangar,
        now: u64,
    ) -> Done {
        if bought(home) {
            return refuse("that home is bought, not rented");
        }
        self.can_take(home, who)?;
        let weeks = weeks.clamp(1, MAX_WEEKS);
        let cost = rent(home) * weeks;
        if hangar.credits < cost {
            return refuse(format!("{weeks} weeks' rent is {} cr", thousands(cost)));
        }
        hangar.credits -= cost;
        self.held.insert(
            home.id.0,
            Held {
                holder: who.to_string(),
                name: name.to_string(),
                tenure: Tenure::Lease { bank: cost, charged_to: now, unpaid_since: None },
                lock: Lock::key(who),
                open: false,
                since: now,
                pieces: Vec::new(),
                next_piece: 0,
                asleep: Vec::new(),
            },
        );
        Ok(format!("{} IS YOURS · {weeks} WEEKS' RENT ON ITS PANEL · {} CR", address(home), thousands(cost)))
    }

    /// `who` buys `home`.
    pub fn buy(&mut self, home: &Home, who: &str, name: &str, hangar: &mut Hangar, now: u64) -> Done {
        if !bought(home) {
            return refuse("that home is rented, not bought");
        }
        self.can_take(home, who)?;
        let cost = price(home);
        if hangar.credits < cost {
            return refuse(format!("it's {} cr", thousands(cost)));
        }
        hangar.credits -= cost;
        self.sold_for += cost;
        self.held.insert(
            home.id.0,
            Held {
                holder: who.to_string(),
                name: name.to_string(),
                tenure: Tenure::Deed { paid: cost, seen: now },
                lock: Lock::key(who),
                open: false,
                since: now,
                pieces: Vec::new(),
                next_piece: 0,
                asleep: Vec::new(),
            },
        );
        Ok(format!("{} IS YOURS · BOUGHT FOR {} CR", address(home), thousands(cost)))
    }

    /// `who` gives up home `id`: a lease ends, its panel's rent handed back; a deed is sold back to
    /// the colony.
    pub fn give_up(&mut self, id: HomeId, who: &str, hangar: &mut Hangar) -> Done {
        let held = self.held_by(id, who)?.clone();
        let place = home(id).map_or_else(|| "IT".to_string(), |h| address(&h));
        self.held.remove(&id.0);
        let (cr, goods) = self.cleared_out(&held);
        hangar.credits += cr;
        for (item, qty) in goods {
            hangar.stores.add(item, qty);
        }
        match held.tenure {
            Tenure::Lease { bank, .. } => {
                hangar.credits += bank;
                Ok(format!("{place} GIVEN UP · {} CR OF RENT BACK", thousands(bank)))
            }
            Tenure::Deed { .. } => {
                let back = home(id).map_or(0, |h| sell_back(&h));
                hangar.credits += back;
                self.paid_back += back;
                Ok(format!("{place} SOLD BACK TO THE COLONY · {} CR", thousands(back)))
            }
        }
    }

    /// A code `code` entered at home `id`'s door by `who`, `now`.
    pub fn enter_code(&mut self, id: HomeId, who: &str, code: u16, now: u64) -> Result<Entered, String> {
        if code > CODE_MAX {
            return refuse("a code is four digits");
        }
        let Some(h) = self.held.get_mut(&id.0) else { return refuse("nobody holds that home") };
        let l = &mut h.lock;
        if now < l.shut_until {
            return Ok(Entered::Shut);
        }
        if l.shut_until != 0 {
            // The shutter ran out: a fresh ladder of misses.
            l.shut_until = 0;
            l.misses = 0;
        }
        let want = if Some(code) == l.code {
            Entered::Crew
        } else if Some(code) == l.guest_code {
            Entered::Guest
        } else {
            let step =
                if now.saturating_sub(l.last_miss) > SHOCK_RESET_S { 1 } else { u16::from(l.misses) + 1 };
            l.misses = l.misses.saturating_add(1);
            l.last_miss = now;
            let shut = l.misses >= LOCKOUT_TRIES;
            if shut {
                l.shut_until = now + LOCKOUT_S;
            }
            return Ok(Entered::Wrong { shock: SHOCK_STEP_HP.saturating_mul(step), shut });
        };
        let remembered = match want {
            Entered::Crew => remember(&mut l.crew, who, CREW_CAP),
            // Crew who enter the guest code stay crew.
            _ if l.is_crew(who) => true,
            _ => remember(&mut l.guests, who, GUEST_CAP),
        };
        if !remembered {
            return refuse("its lock remembers as many as it can");
        }
        l.misses = 0;
        Ok(want)
    }

    /// The crew's `who` sets home `id`'s main code (`guest`: its guest code; `None` clears that).
    /// A new main code clears both lists, keeping its setter and the holder.
    pub fn set_code(&mut self, id: HomeId, who: &str, guest: bool, code: Option<u16>) -> Done {
        if code.is_some_and(|c| c > CODE_MAX) {
            return refuse("a code is four digits");
        }
        let h = self.crew_of(id, who)?;
        if guest {
            h.lock.guest_code = code;
            h.lock.guests.clear();
            return Ok(match code {
                Some(_) => "THE GUEST CODE IS SET · ITS GUESTS FORGOTTEN".into(),
                None => "NO GUEST CODE NOW · ITS GUESTS FORGOTTEN".into(),
            });
        }
        let Some(code) = code else { return refuse("the main code can't be cleared, only changed") };
        h.lock.code = Some(code);
        h.lock.guest_code = None;
        h.lock.locked = true;
        h.lock.guests.clear();
        h.lock.crew.clear();
        h.lock.crew.push(h.holder.clone());
        remember(&mut h.lock.crew, who, CREW_CAP);
        h.lock.misses = 0;
        h.lock.shut_until = 0;
        let lock = &h.lock;
        h.asleep.retain(|a| lock.is_crew(&a.who));
        Ok("THE CODE IS SET · ITS LISTS CLEARED · LOCKED".into())
    }

    /// The crew's `who` locks home `id`'s door (or unlocks it: anyone passes).
    pub fn set_locked(&mut self, id: HomeId, who: &str, locked: bool) -> Done {
        let h = self.crew_of(id, who)?;
        h.lock.locked = locked;
        Ok(if locked { "LOCKED".into() } else { "UNLOCKED · ANYONE CAN COME IN".into() })
    }

    /// `who` opens or shuts home `id`'s door, if its lock lets them.
    pub fn door(&mut self, id: HomeId, who: &str, open: bool) -> Done {
        let Some(h) = self.held.get_mut(&id.0) else { return refuse("nobody holds that home") };
        if !h.lock.passes(who) {
            return refuse("it's locked");
        }
        h.open = open;
        Ok(if open { "OPEN".into() } else { "SHUT".into() })
    }

    /// `who` leaves home `id`'s crew (never its holder).
    pub fn leave_crew(&mut self, id: HomeId, who: &str) -> Done {
        let Some(h) = self.held.get_mut(&id.0) else { return refuse("nobody holds that home") };
        if h.holder == who {
            return refuse("its holder is always on its crew");
        }
        h.lock.crew.retain(|c| c != who);
        h.lock.guests.retain(|c| c != who);
        h.asleep.retain(|a| a.who != who);
        Ok("OFF ITS CREW".into())
    }

    /// The crew's `who` clears home `id`'s crew and guests, keeping themselves and the holder.
    pub fn clear_crew(&mut self, id: HomeId, who: &str) -> Done {
        let h = self.crew_of(id, who)?;
        let holder = h.holder.clone();
        h.lock.crew.retain(|c| *c == holder || c == who);
        h.lock.guests.clear();
        let lock = &h.lock;
        h.asleep.retain(|a| lock.is_crew(&a.who));
        Ok("ITS CREW IS CLEARED".into())
    }

    /// Anyone pays `credits` of rent into home `id`'s panel.
    pub fn feed(&mut self, id: HomeId, credits: u64, hangar: &mut Hangar) -> Done {
        let Some(h) = self.held.get_mut(&id.0) else { return refuse("nobody holds that home") };
        let Tenure::Lease { bank, .. } = &mut h.tenure else { return refuse("a home bought has no rent") };
        if credits == 0 || hangar.credits < credits {
            return refuse("you haven't the credits");
        }
        hangar.credits -= credits;
        *bank += credits;
        Ok(format!("{} CR ON ITS PANEL · {} CR IN ALL", thousands(credits), thousands(*bank)))
    }

    /// The crew's `who` takes `credits` back off home `id`'s panel.
    pub fn withdraw(&mut self, id: HomeId, who: &str, credits: u64, hangar: &mut Hangar) -> Done {
        let h = self.crew_of(id, who)?;
        let Tenure::Lease { bank, .. } = &mut h.tenure else { return refuse("a home bought has no rent") };
        if credits == 0 || credits > *bank {
            return refuse(format!("its panel holds {} cr", thousands(*bank)));
        }
        *bank -= credits;
        hangar.credits += credits;
        Ok(format!("{} CR OFF ITS PANEL", thousands(credits)))
    }

    /// `by` knocks at home `id`'s door: its crew hear it.
    pub fn knock(&mut self, id: HomeId, by: &str) -> Done {
        if !self.held.contains_key(&id.0) {
            return refuse("nobody's home: nobody holds it");
        }
        self.knock_seq += 1;
        self.knocks.push(Knock { seq: self.knock_seq, home: id.0, by: by.to_string() });
        if self.knocks.len() > KNOCKS {
            self.knocks.remove(0);
        }
        Ok("YOU KNOCK".into())
    }

    /// The knocks after `since` at the doors of homes `who` is on the crew of; and the last knock's
    /// place.
    pub fn knocks_for(&self, who: &str, since: u64) -> (Vec<String>, u64) {
        let heard = self
            .knocks
            .iter()
            .filter(|k| k.seq > since)
            .filter(|k| self.held.get(&k.home).is_some_and(|h| h.lock.is_crew(who)))
            .map(|k| {
                let place = home(HomeId(k.home)).map_or_else(|| "YOUR DOOR".to_string(), |h| address(&h));
                format!("{} KNOCKS AT {place}", k.by.to_uppercase())
            })
            .collect();
        (heard, self.knock_seq)
    }

    /// The holder of home `id` is in it, `now`: a bought home isn't abandoned.
    pub fn visited(&mut self, id: HomeId, who: &str, now: u64) {
        if let Some(h) = self.held.get_mut(&id.0)
            && h.holder == who
            && let Tenure::Deed { seen, .. } = &mut h.tenure
        {
            *seen = now;
        }
    }

    /// The register `now`: each lease charged its rent by the hour from its panel (lapsing a day
    /// after it can't be paid), each bought home left a month sold back. Whether anything changed.
    pub fn tick(&mut self, now: u64) -> bool {
        let mut changed = false;
        let mut gone: Vec<(u32, String)> = Vec::new();
        for (&id, h) in self.held.iter_mut() {
            let Some(home) = home(HomeId(id)) else { continue };
            match &mut h.tenure {
                Tenure::Lease { bank, charged_to, unpaid_since } => {
                    let hours = now.saturating_sub(*charged_to) / HOUR_S;
                    if hours == 0 {
                        continue;
                    }
                    let per = ((hourly(&home, h.lock.crew.len()) + 0.5) as u64).max(1);
                    let due = per * hours;
                    let paid = due.min(*bank);
                    *bank -= paid;
                    self.rent_taken += paid;
                    let from = *charged_to;
                    *charged_to += hours * HOUR_S;
                    changed = true;
                    if paid < due {
                        // Unpaid from the hour its panel ran dry.
                        let since = *unpaid_since.get_or_insert(from + paid / per * HOUR_S);
                        if now.saturating_sub(since) >= GRACE_S {
                            gone.push((id, format!("{} LAPSED: ITS RENT WENT UNPAID", address(&home))));
                        }
                    } else {
                        *unpaid_since = None;
                    }
                }
                Tenure::Deed { seen, .. } => {
                    if now.saturating_sub(*seen) >= ABANDONED_S {
                        gone.push((
                            id,
                            format!("{} SOLD BACK: NOBODY HAD BEEN IN IT FOR A MONTH", address(&home)),
                        ));
                    }
                }
            }
        }
        for (id, why) in gone {
            let Some(h) = self.held.remove(&id) else { continue };
            let back = match h.tenure {
                Tenure::Lease { bank, .. } => bank,
                Tenure::Deed { .. } => {
                    let back = home(HomeId(id)).map_or(0, |home| sell_back(&home));
                    self.paid_back += back;
                    back
                }
            };
            let (cr, goods) = self.cleared_out(&h);
            let a = self.accounts.entry(h.holder).or_default();
            a.credits += back + cr;
            for (item, qty) in goods {
                *a.goods.entry(item).or_default() += qty;
            }
            a.notes.push(why);
            changed = true;
        }
        changed
    }

    /// Whether anything's waiting for `who` in their bay.
    pub fn owed(&self, who: &str) -> bool {
        self.accounts.contains_key(who)
    }

    /// A home's furniture sold back as it's given up or lapses (the colony's half), and what its
    /// chests and safes held: credits, and goods.
    fn cleared_out(&mut self, h: &Held) -> (u64, Vec<(Item, u64)>) {
        let back: u64 = h.pieces.iter().map(|p| p.kind.spec().price * FURNITURE_BACK_PCT / 100).sum();
        self.furniture_back += back;
        (back, h.pieces.iter().flat_map(|p| p.holds.iter().copied()).collect())
    }

    /// The crew's `who` buys a piece of furniture `kind` for home `id` and puts it down where it fits
    /// (`content::homes::fits`): its middle `(s, x)` dm from the home's floor's −s −x corner, `turn`
    /// quarter turns, on `storey`.
    #[allow(clippy::too_many_arguments)]
    pub fn furnish(
        &mut self,
        id: HomeId,
        who: &str,
        kind: Furniture,
        (s, x): (u16, u16),
        turn: u8,
        storey: u8,
        hangar: &mut Hangar,
    ) -> Done {
        let h = self.crew_of(id, who)?;
        if h.pieces.len() >= MAX_PIECES {
            return refuse(format!("a home holds {MAX_PIECES} pieces at most"));
        }
        let piece = Piece { id: h.next_piece, kind, s, x, turn: turn % 4, storey, holds: Vec::new() };
        placed(id, h, &piece, None)?;
        let spec = kind.spec();
        if hangar.credits < spec.price {
            return refuse(format!("{} is {} cr", spec.name, thousands(spec.price)));
        }
        hangar.credits -= spec.price;
        h.next_piece = h.next_piece.wrapping_add(1);
        h.pieces.push(piece);
        self.furniture_sold += spec.price;
        Ok(format!("{} DELIVERED AND PUT DOWN · {} CR", spec.name.to_uppercase(), thousands(spec.price)))
    }

    /// The crew's `who` moves piece `piece` of home `id`.
    pub fn move_piece(
        &mut self,
        id: HomeId,
        who: &str,
        piece: u16,
        (s, x): (u16, u16),
        turn: u8,
        storey: u8,
    ) -> Done {
        let h = self.crew_of(id, who)?;
        let Some(i) = h.pieces.iter().position(|p| p.id == piece) else { return refuse("no such piece") };
        let moved = Piece { s, x, turn: turn % 4, storey, ..h.pieces[i].clone() };
        placed(id, h, &moved, Some(piece))?;
        h.pieces[i] = moved;
        Ok("MOVED".into())
    }

    /// `who` (going by `name`) logs out at home: in home `id`, on its crew, they lie down in the free
    /// bed nearest `(s, x)` on `storey` (m from the home's floor's −s −x corner; a house's storey)
    /// and are asleep there until they're back (one bed at a time). The bed, if there's one.
    pub fn sleep(
        &mut self,
        id: HomeId,
        who: &str,
        name: &str,
        (s, x): (f32, f32),
        storey: u8,
    ) -> Option<u16> {
        self.wake(who);
        let h = self.held.get_mut(&id.0)?;
        if !h.lock.is_crew(who) {
            return None;
        }
        let far = |p: &Piece| {
            let (ps, px) = p.at();
            (ps - s).hypot(px - x) + if p.storey == storey { 0.0 } else { 100.0 }
        };
        let bed = h
            .pieces
            .iter()
            .filter(|p| p.kind == Furniture::Bed && !h.asleep.iter().any(|a| a.piece == p.id))
            .min_by(|a, b| far(a).total_cmp(&far(b)))?
            .id;
        h.asleep.push(Asleep { piece: bed, who: who.into(), name: name.into() });
        Some(bed)
    }

    /// `who` is back: they're no longer asleep anywhere. The home and the bed they wake in, if
    /// it's still theirs to (still on its crew, the bed still there).
    pub fn wake(&mut self, who: &str) -> Option<(HomeId, Piece)> {
        let (id, h) = self.held.iter_mut().find(|(_, h)| h.asleep.iter().any(|a| a.who == who))?;
        let i = h.asleep.iter().position(|a| a.who == who)?;
        let piece = h.asleep.remove(i).piece;
        if !h.lock.is_crew(who) {
            return None;
        }
        let bed = h.pieces.iter().find(|p| p.id == piece)?;
        Some((HomeId(*id), bed.clone()))
    }

    /// Who's asleep in home `id`'s beds: each bed, and the name on them.
    pub fn asleep_in(&self, id: HomeId) -> Vec<(u16, String)> {
        self.held
            .get(&id.0)
            .map_or_else(Vec::new, |h| h.asleep.iter().map(|a| (a.piece, a.name.clone())).collect())
    }

    /// The crew's `who` sells piece `piece` of home `id` back to the colony (half its price): one
    /// that holds nothing.
    pub fn sell_piece(&mut self, id: HomeId, who: &str, piece: u16, hangar: &mut Hangar) -> Done {
        let h = self.crew_of(id, who)?;
        let Some(i) = h.pieces.iter().position(|p| p.id == piece) else { return refuse("no such piece") };
        if !h.pieces[i].holds.is_empty() {
            return refuse("empty it first");
        }
        if h.asleep.iter().any(|a| a.piece == piece) {
            return refuse("someone's asleep in it");
        }
        let kind = h.pieces.remove(i).kind;
        let back = kind.spec().price * FURNITURE_BACK_PCT / 100;
        hangar.credits += back;
        self.furniture_back += back;
        Ok(format!("{} SOLD BACK · {} CR", kind.spec().name.to_uppercase(), thousands(back)))
    }

    /// The crew's `who` puts `qty` of `item` from their bay's stores into piece `piece` of home `id`
    /// (a chest or a safe: so many kinds of thing at most; a suit's parts stay in the bay).
    pub fn stash(
        &mut self,
        id: HomeId,
        who: &str,
        piece: u16,
        item: Item,
        qty: u64,
        hangar: &mut Hangar,
    ) -> Done {
        if matches!(item, Item::Part(..)) || !item.valid() || qty == 0 {
            return refuse("that stays in the bay");
        }
        let h = self.crew_of(id, who)?;
        let Some(p) = h.pieces.iter_mut().find(|p| p.id == piece) else { return refuse("no such piece") };
        let holds = usize::from(p.kind.spec().holds);
        if holds == 0 {
            return refuse("that holds nothing");
        }
        if !p.holds.iter().any(|(i, _)| *i == item) && p.holds.len() >= holds {
            return refuse(format!("it holds {holds} kinds of thing at most"));
        }
        if !hangar.stores.take(item, qty) {
            return refuse("you haven't that much in your bay");
        }
        match p.holds.iter_mut().find(|(i, _)| *i == item) {
            Some((_, q)) => *q += qty,
            None => p.holds.push((item, qty)),
        }
        Ok(format!("{} PUT AWAY", item.amount(qty)))
    }

    /// The crew's `who` takes `qty` of `item` out of piece `piece` of home `id`, back to their bay.
    pub fn unstash(
        &mut self,
        id: HomeId,
        who: &str,
        piece: u16,
        item: Item,
        qty: u64,
        hangar: &mut Hangar,
    ) -> Done {
        let h = self.crew_of(id, who)?;
        let Some(p) = h.pieces.iter_mut().find(|p| p.id == piece) else { return refuse("no such piece") };
        let Some(i) = p.holds.iter().position(|(i, q)| *i == item && *q >= qty && qty > 0) else {
            return refuse("it doesn't hold that much");
        };
        p.holds[i].1 -= qty;
        if p.holds[i].1 == 0 {
            p.holds.remove(i);
        }
        hangar.stores.add(item, qty);
        Ok(format!("{} TAKEN BACK TO YOUR BAY", item.amount(qty)))
    }

    /// What's waiting for `who` in their bay, into their hangar: what a lapsed or sold home handed
    /// back, and the news of it.
    pub fn collect(&mut self, hangar: &mut Hangar, who: &str) -> Vec<String> {
        let Some(a) = self.accounts.remove(who) else { return Vec::new() };
        hangar.credits += a.credits;
        for (item, qty) in a.goods {
            hangar.stores.add(item, qty);
        }
        let mut notes = a.notes;
        if a.credits > 0 {
            notes.push(format!("{} CR FROM THE COLONY'S REGISTER OF HOMES", thousands(a.credits)));
        }
        notes
    }

    /// Credits the register holds: the rent on every panel, and what's waiting in accounts.
    pub fn holding(&self) -> u64 {
        let banks: u64 = self
            .held
            .values()
            .map(|h| match h.tenure {
                Tenure::Lease { bank, .. } => bank,
                Tenure::Deed { .. } => 0,
            })
            .sum();
        banks + self.accounts.values().map(|a| a.credits).sum::<u64>()
    }
}

/// Whether `piece` fits where it's put in home `id` (held as `h`), among its other pieces but
/// `moving` (the one being moved, if it's a move).
fn placed(id: HomeId, h: &Held, piece: &Piece, moving: Option<u16>) -> Result<(), String> {
    let Some(home) = home(id) else { return refuse("there's no home there") };
    let (ms, mx) = home.rect.middle();
    let Some(plan) = plan_at(home.strip, ms, mx) else { return refuse("there's no home there") };
    let b = piece_box(&home, piece.kind, piece.at(), piece.turn, u32::from(piece.storey));
    let others = h
        .pieces
        .iter()
        .filter(|p| Some(p.id) != moving)
        .map(|p| (piece_box(&home, p.kind, p.at(), p.turn, u32::from(p.storey)), p.kind.spec().solid));
    fits(&home, &plan, piece.kind, &b, u32::from(piece.storey), others).map_err(String::from)
}

/// Puts `who` on `list`, unless it's full (a list refuses newcomers, never forgets anyone).
fn remember(list: &mut Vec<Trader>, who: &str, cap: usize) -> bool {
    if list.iter().any(|m| m == who) {
        return true;
    }
    if list.len() >= cap {
        return false;
    }
    list.push(who.to_string());
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use bc_sim::colony::city::{CITY, ROWS, Stage, block, lots};
    use bc_sim::colony::homes::plan;
    use bc_sim::content::homes::WEEK_S;

    /// A flat that's rented, and a home that's bought, on strip 0.
    fn homes() -> (Home, Home) {
        let (mut rented, mut owned) = (None, None);
        'out: for bx in CITY.0..CITY.1 {
            for row in 1..=ROWS {
                let Some(b) = block(0, bx, row, Stage(0)) else { continue };
                for bd in lots(&b).as_slice() {
                    let Some(p) = plan(&b, bd) else { continue };
                    for k in 0..p.storeys {
                        for u in 0..p.homes_per_storey() {
                            let h = p.home(k, u).unwrap();
                            if bought(&h) {
                                owned.get_or_insert(h);
                            } else {
                                rented.get_or_insert(h);
                            }
                        }
                    }
                    if rented.is_some() && owned.is_some() {
                        break 'out;
                    }
                }
            }
        }
        (rented.unwrap(), owned.unwrap())
    }

    fn rich() -> Hangar {
        Hangar { credits: 1_000_000, ..Hangar::starter() }
    }

    #[test]
    fn a_flat_is_rented_its_rent_spent_by_the_hour_and_it_lapses_unpaid() {
        let (flat, _) = homes();
        let mut reg = Homes::new();
        let mut h = rich();
        let start = h.credits;
        reg.rent(&flat, "0xa", "ADA", 1, &mut h, 1_000).unwrap();
        assert_eq!(h.credits, start - rent(&flat));
        assert!(reg.rent(&flat, "0xb", "BO", 1, &mut rich(), 1_000).is_err(), "taken");
        // A week on, the week's rent is spent (give or take the hours' rounding).
        reg.tick(1_000 + WEEK_S);
        let spent = reg.rent_taken;
        assert!(spent + 200 >= rent(&flat) && spent <= rent(&flat) + 200, "{spent} of {}", rent(&flat));
        // Unpaid a day and more, it lapses; what was left comes back in the bay.
        reg.tick(1_000 + WEEK_S + 2 * GRACE_S);
        assert!(reg.get(flat.id).is_none(), "lapsed");
        let notes = reg.collect(&mut h, "0xa");
        assert!(notes.iter().any(|n| n.contains("LAPSED")), "{notes:?}");
        // Every credit accounted for.
        assert_eq!(h.credits + reg.rent_taken + reg.holding(), start);
    }

    #[test]
    fn a_house_is_bought_and_sold_back() {
        let (_, house) = homes();
        let mut reg = Homes::new();
        let mut h = rich();
        let start = h.credits;
        reg.buy(&house, "0xa", "ADA", &mut h, 0).unwrap();
        assert_eq!(h.credits, start - price(&house));
        assert!(reg.give_up(house.id, "0xb", &mut rich()).is_err(), "not theirs");
        reg.give_up(house.id, "0xa", &mut h).unwrap();
        assert_eq!(h.credits, start - price(&house) + sell_back(&house));
        assert_eq!(h.credits + reg.sold_for - reg.paid_back, start);
    }

    #[test]
    fn the_code_lock_lets_in_who_knows_its_code_and_shuts_out_a_guesser() {
        let (flat, _) = homes();
        let mut reg = Homes::new();
        reg.rent(&flat, "0xa", "ADA", 1, &mut rich(), 0).unwrap();
        // The key lock: its holder alone.
        assert!(!reg.may_enter(flat.id, "0xb"));
        assert!(reg.may_enter(flat.id, "0xa"));
        assert!(reg.set_code(flat.id, "0xb", false, Some(1234)).is_err(), "not crew");
        reg.set_code(flat.id, "0xa", false, Some(1234)).unwrap();
        reg.set_code(flat.id, "0xa", true, Some(5555)).unwrap();
        // The guest code opens the door, nothing more.
        assert_eq!(reg.enter_code(flat.id, "0xg", 5555, 10), Ok(Entered::Guest));
        assert!(reg.may_enter(flat.id, "0xg"));
        assert!(reg.set_locked(flat.id, "0xg", false).is_err());
        // The main code: on the crew.
        assert_eq!(reg.enter_code(flat.id, "0xb", 1234, 10), Ok(Entered::Crew));
        assert!(reg.get(flat.id).unwrap().lock.is_crew("0xb"));
        // A guesser: shocks that climb, and the keypad shut after eight.
        let mut shocks = Vec::new();
        for i in 0..LOCKOUT_TRIES {
            match reg.enter_code(flat.id, "0xc", 1000 + u16::from(i), 20 + u64::from(i)) {
                Ok(Entered::Wrong { shock, shut }) => {
                    shocks.push(shock);
                    assert_eq!(shut, i + 1 == LOCKOUT_TRIES);
                }
                other => panic!("{other:?}"),
            }
        }
        assert!(shocks.windows(2).all(|w| w[1] > w[0]), "{shocks:?}");
        assert_eq!(reg.enter_code(flat.id, "0xc", 1234, 40), Ok(Entered::Shut), "even the right code");
        assert_eq!(reg.enter_code(flat.id, "0xc", 1234, 40 + LOCKOUT_S), Ok(Entered::Crew));
        // A new code: the lists cleared, its setter and holder kept.
        reg.set_code(flat.id, "0xb", false, Some(9876)).unwrap();
        let l = &reg.get(flat.id).unwrap().lock;
        assert!(l.is_crew("0xa") && l.is_crew("0xb") && !l.is_crew("0xc") && l.guests.is_empty());
    }

    #[test]
    fn a_door_is_worked_by_whoever_passes_its_lock_and_an_open_door_by_anyone() {
        let (flat, _) = homes();
        let mut reg = Homes::new();
        reg.rent(&flat, "0xa", "ADA", 1, &mut rich(), 0).unwrap();
        assert!(reg.door(flat.id, "0xb", true).is_err());
        reg.door(flat.id, "0xa", true).unwrap();
        assert!(reg.may_enter(flat.id, "0xb"), "an open door");
        reg.door(flat.id, "0xa", false).unwrap();
        reg.set_locked(flat.id, "0xa", false).unwrap();
        assert!(reg.may_enter(flat.id, "0xb"), "unlocked");
        reg.door(flat.id, "0xb", true).unwrap();
    }

    #[test]
    fn a_crew_of_more_than_four_pays_more_and_a_full_list_refuses() {
        let (flat, _) = homes();
        let mut reg = Homes::new();
        reg.rent(&flat, "0xa", "ADA", 1, &mut rich(), 0).unwrap();
        reg.set_code(flat.id, "0xa", false, Some(1)).unwrap();
        for i in 1..CREW_CAP {
            assert_eq!(reg.enter_code(flat.id, &format!("0x{i}"), 1, 0), Ok(Entered::Crew));
        }
        assert!(reg.enter_code(flat.id, "0xlate", 1, 0).is_err(), "the list is full");
        assert!(hourly(&flat, CREW_CAP) > hourly(&flat, 1));
        reg.clear_crew(flat.id, "0x3").unwrap();
        let l = &reg.get(flat.id).unwrap().lock;
        assert_eq!(l.crew.len(), 2, "{:?}", l.crew);
    }

    #[test]
    fn guests_hold_no_homes_and_nobody_holds_more_than_the_most() {
        let (flat, house) = homes();
        let mut reg = Homes::new();
        assert!(reg.rent(&flat, "guest-3-ab", "G", 1, &mut rich(), 0).is_err());
        reg.rent(&flat, "0xa", "ADA", 1, &mut rich(), 0).unwrap();
        reg.buy(&house, "0xa", "ADA", &mut rich(), 0).unwrap();
        let third = third(&flat, &house);
        assert!(reg.rent(&third, "0xa", "ADA", 1, &mut rich(), 0).is_err(), "{MAX_HELD} at most");
    }

    /// A rented flat that's neither `a` nor `b`.
    fn third(a: &Home, b: &Home) -> Home {
        for bx in CITY.0..CITY.1 {
            for row in 1..=ROWS {
                let Some(bl) = block(0, bx, row, Stage(0)) else { continue };
                for bd in lots(&bl).as_slice() {
                    let Some(p) = plan(&bl, bd) else { continue };
                    let h = p.home(0, 0).unwrap();
                    if h.id != a.id && h.id != b.id && !bought(&h) {
                        return h;
                    }
                }
            }
        }
        panic!("no third home")
    }

    #[test]
    fn a_home_is_furnished_its_chest_holds_what_the_bay_puts_in_and_it_all_comes_back() {
        let (flat, _) = homes();
        let mut reg = Homes::new();
        let mut h = rich();
        let start = h.credits;
        reg.rent(&flat, "0xa", "ADA", 1, &mut h, 0).unwrap();
        let mid = ((flat.rect.width() * 5.0) as u16, (flat.rect.length() * 5.0) as u16);
        assert!(reg.furnish(flat.id, "0xb", Furniture::Sofa, mid, 0, 0, &mut rich()).is_err(), "not crew");
        reg.furnish(flat.id, "0xa", Furniture::Sofa, mid, 0, 0, &mut h).unwrap();
        assert!(reg.furnish(flat.id, "0xa", Furniture::Chest, mid, 0, 0, &mut h).is_err(), "on the sofa");
        let beside = (mid.0, mid.1 + 15);
        reg.furnish(flat.id, "0xa", Furniture::Chest, beside, 0, 0, &mut h).unwrap();
        let chest = reg.get(flat.id).unwrap().pieces[1].id;
        let steel = Item::Material(crate::item::Material::Steel);
        let have = h.stores.get(steel);
        reg.stash(flat.id, "0xa", chest, steel, 50, &mut h).unwrap();
        assert_eq!(h.stores.get(steel), have - 50);
        assert!(reg.sell_piece(flat.id, "0xa", chest, &mut h).is_err(), "it holds something");
        // Given up, the furniture's sold back for half and what the chest held goes to the bay.
        reg.give_up(flat.id, "0xa", &mut h).unwrap();
        assert_eq!(h.stores.get(steel), have);
        let spent = Furniture::Sofa.spec().price + Furniture::Chest.spec().price;
        assert_eq!(h.credits, start - spent + spent / 2);
        assert_eq!(
            h.credits + reg.rent_taken + reg.furniture_sold - reg.furniture_back + reg.holding(),
            start
        );
    }

    #[test]
    fn everyone_near_sees_the_furniture_but_only_its_crew_what_the_chest_holds() {
        let (flat, _) = homes();
        let mut reg = Homes::new();
        let mut h = rich();
        reg.rent(&flat, "0xa", "ADA", 1, &mut h, 0).unwrap();
        let mid = ((flat.rect.width() * 5.0) as u16, (flat.rect.length() * 5.0) as u16);
        reg.furnish(flat.id, "0xa", Furniture::Chest, mid, 0, 0, &mut h).unwrap();
        let chest = reg.get(flat.id).unwrap().pieces[0].id;
        let steel = Item::Material(crate::item::Material::Steel);
        reg.stash(flat.id, "0xa", chest, steel, 50, &mut h).unwrap();
        let seen = |reg: &Homes, who: &str| {
            crate::wire::HomesView::of(reg, [flat.id.0].into_iter(), who).homes.remove(0)
        };
        let (mine, theirs) = (seen(&reg, "0xa"), seen(&reg, "0xb"));
        assert_eq!(mine.pieces[0].holds, vec![(steel, 50)]);
        assert_eq!(theirs.pieces.len(), 1, "the chest stands there for all to see");
        assert!(theirs.pieces[0].holds.is_empty() && theirs.panel.is_none());
        // A stranger can't take from it, or put in it.
        assert!(reg.unstash(flat.id, "0xb", chest, steel, 1, &mut rich()).is_err());
        assert!(reg.stash(flat.id, "0xb", chest, steel, 1, &mut rich()).is_err());
        // Its code shared, they're on its crew and may.
        reg.set_code(flat.id, "0xa", false, Some(1234)).unwrap();
        reg.enter_code(flat.id, "0xb", 1234, 0).unwrap();
        let mut theirs = rich();
        reg.unstash(flat.id, "0xb", chest, steel, 10, &mut theirs).unwrap();
        assert_eq!(seen(&reg, "0xb").pieces[0].holds, vec![(steel, 40)]);
    }

    #[test]
    fn logged_out_at_home_they_sleep_in_its_nearest_bed_and_wake_in_it() {
        let (flat, _) = homes();
        let mut reg = Homes::new();
        let mut h = rich();
        reg.rent(&flat, "0xa", "ADA", 1, &mut h, 0).unwrap();
        // No bed, no sleep.
        assert_eq!(reg.sleep(flat.id, "0xa", "ADA", (1.0, 1.0), 0), None);
        let mid = ((flat.rect.width() * 5.0) as u16, (flat.rect.length() * 5.0) as u16);
        reg.furnish(flat.id, "0xa", Furniture::Bed, (mid.0, mid.1 - 12), 0, 0, &mut h).unwrap();
        reg.furnish(flat.id, "0xa", Furniture::Bed, (mid.0, mid.1 + 12), 0, 0, &mut h).unwrap();
        let far = reg.get(flat.id).unwrap().pieces[1].id;
        // A stranger can't.
        assert_eq!(reg.sleep(flat.id, "0xb", "BO", (1.0, 1.0), 0), None);
        let near_far = (f32::from(mid.0) / 10.0, f32::from(mid.1) / 10.0 + 2.0);
        assert_eq!(reg.sleep(flat.id, "0xa", "ADA", near_far, 0), Some(far));
        assert_eq!(reg.asleep_in(flat.id), vec![(far, "ADA".to_string())]);
        assert!(reg.sell_piece(flat.id, "0xa", far, &mut h).is_err(), "someone's asleep in it");
        // Its bed went round the register's save and back.
        let back: Homes = serde_json::from_str(&serde_json::to_string(&reg).unwrap()).unwrap();
        assert_eq!(back.asleep_in(flat.id).len(), 1);
        let (home, bed) = reg.wake("0xa").unwrap();
        assert_eq!((home, bed.id), (flat.id, far));
        assert!(reg.asleep_in(flat.id).is_empty() && reg.wake("0xa").is_none());
        // Off its crew while asleep (a new code), they wake at Hub Gate.
        reg.set_code(flat.id, "0xa", false, Some(4321)).unwrap();
        reg.enter_code(flat.id, "0xb", 4321, 0).unwrap();
        reg.sleep(flat.id, "0xb", "BO", (1.0, 1.0), 0).unwrap();
        reg.set_code(flat.id, "0xa", false, Some(1111)).unwrap();
        assert!(reg.asleep_in(flat.id).is_empty() && reg.wake("0xb").is_none());
    }

    #[test]
    fn a_knock_is_heard_by_the_crew_alone() {
        let (flat, _) = homes();
        let mut reg = Homes::new();
        reg.rent(&flat, "0xa", "ADA", 1, &mut rich(), 0).unwrap();
        reg.knock(flat.id, "Bo").unwrap();
        let (heard, seq) = reg.knocks_for("0xa", 0);
        assert_eq!(heard.len(), 1, "{heard:?}");
        assert!(reg.knocks_for("0xb", 0).0.is_empty());
        assert!(reg.knocks_for("0xa", seq).0.is_empty());
    }
}
