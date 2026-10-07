//! The Charter Board: the colony's notices, its contracts and its great works.
//!
//! - **Contracts.** Jobs posted with their rewards: a pilot's are held in escrow here from the
//!   moment they're posted, so a job is always good for its pay; the colony's are paid from its
//!   treasury. Two kinds:
//!   - *supply*: deliver `qty` of an item. Anyone but its issuer may deliver part of it from their
//!     stores and is paid pro rata on the spot; what's delivered goes to the issuer (to the
//!     colony's desk, or to the pilot's account here, collected next time they're in their bay).
//!     The colony posts its own as its stocks run low, at a premium over its desks' prices;
//!   - *patrol* (once the militia has a hangar): take it, and down Mobile Dolls for a given sum of
//!     bounties before the time's up; the militia pays its reward on top of the bounties. One
//!     pilot holds a patrol at a time.
//!
//!   Whatever isn't paid out when a contract expires or is withdrawn goes back to its issuer.
//! - **Great works** ([`Work`]): the colony's projects, each needing tonnes of materials. Pilots
//!   deliver them at the colony's value and a premium, and in standing. Finishing one changes the
//!   world (the second foundry halves its fees and doubles its speed; the militia's hangar opens
//!   patrols), and finishing the era's works opens the charter vote: once enough pilots of
//!   standing have signed it, the calendar begins (`STORY.md`, "The calendar and the eras").
//! - **Standing**: credits a pilot has earned from the colony's contracts and works. It is what
//!   signing the charter takes, and the board's lists of contributors are ranked by it.
//!
//! The ledger: credits come into the economy only from the colony (`colony_paid`); a pilot's
//! contract never makes or loses any (`tests/ledger.rs`).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::catalogue::{value, worth};
use crate::exchange::{Account, Exchange, Trader};
use crate::hangar::{Done, Hangar};
use crate::item::{Item, Material, Ore, thousands};

/// Most open contracts one pilot may have posted.
pub const MAX_POSTED: usize = 8;
/// The colony's open supply contracts, and (with the militia's hangar) its patrols.
pub const COLONY_SUPPLY: usize = 4;
pub const COLONY_PATROLS: usize = 2;
/// How long the colony's supply contracts stand, s.
pub const SUPPLY_SECS: u64 = 2 * 3_600;
/// What the colony's supply contracts pay over the value of what they ask for, percent.
pub const SUPPLY_PCT: u64 = 135;
/// What the great works pay over the value of what's delivered, percent.
pub const WORKS_PCT: u64 = 120;
/// A patrol: the bounties to earn, what the militia pays on top, how long a pilot has once
/// they've taken it, and how long it stands untaken, s.
pub const PATROL_BOUNTY: u64 = 1_000;
pub const PATROL_REWARD: u64 = 1_500;
pub const PATROL_SECS: u64 = 3_600;
pub const PATROL_STANDS_SECS: u64 = 6 * 3_600;
/// Signatures of pilots of standing the charter needs.
pub const SIGNATURES: usize = 3;
/// The longest a pilot's contract may stand, hours.
pub const MAX_HOURS: u64 = 72;
/// Notices kept for pilots who weren't there.
const NEWS_KEPT: usize = 16;
/// Most contracts a view lists (the pilot's own first).
const VIEW_CONTRACTS: usize = 40;

/// The colony's great works.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Work {
    /// A second zero-G foundry on the spire: gundanium at half the fee, twice as fast.
    SecondFoundry,
    /// A hangar for the colony's militia: it posts patrols over the field.
    MilitiaHangar,
    /// The charter vote: signed by pilots of standing, it begins the calendar.
    CharterVote,
}

const STEEL: Item = Item::Material(Material::Steel);
const ALLOY: Item = Item::Material(Material::TitaniumAlloy);
const ELECTRONICS: Item = Item::Material(Material::Electronics);
const COMPONENTS: Item = Item::Material(Material::Components);
const MUNITIONS: Item = Item::Material(Material::Munitions);
const PROPELLANT: Item = Item::Material(Material::Propellant);

impl Work {
    pub const ALL: [Work; 3] = [Work::SecondFoundry, Work::MilitiaHangar, Work::CharterVote];

    pub fn name(self) -> &'static str {
        match self {
            Work::SecondFoundry => "A second foundry",
            Work::MilitiaHangar => "The militia's hangar",
            Work::CharterVote => "The charter vote",
        }
    }

    /// What finishing it does, as the board says it.
    pub fn summary(self) -> &'static str {
        match self {
            Work::SecondFoundry => {
                "A second zero-G foundry on the spire. Gundanium at half the fee, made twice as fast."
            }
            Work::MilitiaHangar => {
                "A hangar for the colony's own militia. Pilots on contract patrol the field against the Dolls."
            }
            Work::CharterVote => {
                "The colony governs itself. Pilots of standing sign, and the calendar begins: AC 1."
            }
        }
    }

    /// What it needs delivered (none: the vote, which needs signatures).
    pub fn needs(self) -> &'static [(Item, u64)] {
        match self {
            Work::SecondFoundry => {
                &[(STEEL, 12_000), (ALLOY, 6_000), (ELECTRONICS, 800), (COMPONENTS, 1_500)]
            }
            Work::MilitiaHangar => &[
                (STEEL, 15_000),
                (ALLOY, 4_000),
                (COMPONENTS, 2_000),
                (MUNITIONS, 3_000),
                (PROPELLANT, 4_000),
            ],
            Work::CharterVote => &[],
        }
    }

    /// The era it belongs to.
    pub fn era(self) -> u8 {
        0
    }

    /// What every pilot hears when it's finished.
    fn finished(self) -> &'static str {
        match self {
            Work::SecondFoundry => {
                "THE SECOND FOUNDRY IS LIT · GUNDANIUM AT HALF THE FEE, TWICE AS FAST · THE CHARTER BOARD THANKS ITS PILOTS"
            }
            Work::MilitiaHangar => {
                "THE MILITIA'S HANGAR IS OPEN · PATROLS ON THE CHARTER BOARD · THE COLONY PAYS FOR EVERY DOLL DOWNED ON CONTRACT"
            }
            Work::CharterVote => {
                "THE CHARTER IS SIGNED · THE COLONY GOVERNS ITSELF · THE CALENDAR BEGINS: AC 1"
            }
        }
    }
}

/// An era's name, as the board heads its notices.
pub fn era_name(era: u8) -> String {
    match era {
        0 => "BEFORE COLONY".into(),
        n => format!("AC {n} · THE CHARTER"),
    }
}

/// What a contract asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Task {
    /// Deliver `qty` of `item`; `delivered` so far.
    Supply { item: Item, qty: u64, delivered: u64 },
    /// Down Mobile Dolls for `bounty` credits of bounties while holding it; `earned` so far.
    Patrol { bounty: u64, earned: u64 },
}

/// A contract on the board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub id: u64,
    /// Who posted it (`None`: the colony), and the name they posted it under.
    pub issuer: Option<Trader>,
    pub issuer_name: String,
    pub task: Task,
    /// What it pays, all of it, and what it has paid so far.
    pub reward: u64,
    pub paid: u64,
    /// Unix seconds: when it was posted, and when it expires.
    pub posted: u64,
    pub expires: u64,
    /// A patrol: who's flying it.
    pub holder: Option<Trader>,
}

impl Contract {
    /// What delivering up to `delivered` of a supply contract's `qty` has paid in all.
    fn pay_for(&self, delivered: u64, qty: u64) -> u64 {
        let v = u128::from(self.reward) * u128::from(delivered) / u128::from(qty.max(1));
        u64::try_from(v).unwrap_or(u64::MAX)
    }

    /// What's left to pay (a pilot's: still in escrow).
    pub fn unpaid(&self) -> u64 {
        self.reward - self.paid
    }

    fn label(&self) -> String {
        format!("CONTRACT #{}", self.id)
    }
}

/// A great work's progress.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub delivered: BTreeMap<Item, u64>,
    /// Unix seconds it was finished.
    pub done: Option<u64>,
    /// Standing each pilot earned on it.
    pub contributors: BTreeMap<Trader, u64>,
}

/// What the finished works do to the colony's services.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effects {
    /// The foundry's fee, percent of the recipe's, and how much faster it works.
    pub foundry_fee_pct: u64,
    pub foundry_speed: f64,
    /// The militia posts patrols.
    pub patrols: bool,
    pub era: u8,
}

/// A notice for every pilot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub seq: u64,
    pub text: String,
}

/// The board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    contracts: BTreeMap<u64, Contract>,
    works: BTreeMap<Work, Progress>,
    /// Who has signed the charter.
    signatures: Vec<Trader>,
    era: u8,
    standing: BTreeMap<Trader, u64>,
    /// The names pilots last went by here.
    names: BTreeMap<Trader, String>,
    /// What pilots are owed: escrow handed back, goods delivered to them, rewards.
    accounts: BTreeMap<Trader, Account>,
    news: Vec<Notice>,
    next_id: u64,
    news_seq: u64,
    /// Credits the colony has paid out on contracts and works (the ledger's source).
    pub colony_paid: u64,
    /// The Most Wanted (`bc_sim::content::aces`): who downed each of Zodiac's aces last...
    #[serde(default)]
    wanted: BTreeMap<u8, Downed>,
    /// ...how many each pilot has downed (the ladder)...
    #[serde(default)]
    aces: BTreeMap<Trader, u32>,
    /// ...and the pilots who take an ace's bounty as salvage rights to its wreck (the rest, pay).
    #[serde(default)]
    salvage_terms: BTreeSet<Trader>,
}

/// An ace downed: by whom (the name they went by), and when (unix seconds).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Downed {
    pub by: String,
    pub at: u64,
}

impl Default for Board {
    fn default() -> Self {
        Self {
            contracts: BTreeMap::new(),
            works: Work::ALL.into_iter().map(|w| (w, Progress::default())).collect(),
            signatures: Vec::new(),
            era: 0,
            standing: BTreeMap::new(),
            names: BTreeMap::new(),
            accounts: BTreeMap::new(),
            news: Vec::new(),
            next_id: 1,
            news_seq: 0,
            colony_paid: 0,
            wanted: BTreeMap::new(),
            aces: BTreeMap::new(),
            salvage_terms: BTreeSet::new(),
        }
    }
}

fn refuse<T>(why: impl Into<String>) -> Result<T, String> {
    Err(why.into())
}

/// The colony's supply jobs: what it asks for, and how much at a time.
const SUPPLY: [(Item, u64); 9] = [
    (Item::Ore(Ore::NickelIron), 4_000),
    (Item::Ore(Ore::Titanium), 2_000),
    (Item::Ore(Ore::Volatiles), 2_000),
    (Item::Ore(Ore::Exotics), 300),
    (STEEL, 1_500),
    (ALLOY, 1_000),
    (ELECTRONICS, 200),
    (MUNITIONS, 1_000),
    (COMPONENTS, 400),
];

/// What `qty` of `item` is worth at the colony's value, times `pct` percent.
fn priced(item: Item, qty: u64, pct: u64) -> u64 {
    worth(item, value(item), qty).saturating_mul(pct) / 100
}

impl Board {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn era(&self) -> u8 {
        self.era
    }

    pub fn contracts(&self) -> impl Iterator<Item = &Contract> {
        self.contracts.values()
    }

    pub fn contract(&self, id: u64) -> Option<&Contract> {
        self.contracts.get(&id)
    }

    pub fn progress(&self, work: Work) -> &Progress {
        self.works.get(&work).expect("every work has its progress")
    }

    pub fn standing(&self, trader: &str) -> u64 {
        self.standing.get(trader).copied().unwrap_or(0)
    }

    pub fn done(&self, work: Work) -> bool {
        self.progress(work).done.is_some()
    }

    /// The charter vote is open: the era's other works are finished.
    pub fn vote_open(&self) -> bool {
        Work::ALL.iter().filter(|w| **w != Work::CharterVote && w.era() == self.era).all(|w| self.done(*w))
            && !self.done(Work::CharterVote)
    }

    pub fn effects(&self) -> Effects {
        let foundry = self.done(Work::SecondFoundry);
        Effects {
            foundry_fee_pct: if foundry { 50 } else { 100 },
            foundry_speed: if foundry { 2.0 } else { 1.0 },
            patrols: self.done(Work::MilitiaHangar),
            era: self.era,
        }
    }

    /// The newest notice's number (0: none yet).
    pub fn news_seq(&self) -> u64 {
        self.news_seq
    }

    /// Notices after `seq`, oldest first.
    pub fn news_since(&self, seq: u64) -> impl Iterator<Item = &Notice> {
        self.news.iter().filter(move |n| n.seq > seq)
    }

    fn announce(&mut self, text: impl Into<String>) {
        self.news_seq += 1;
        self.news.push(Notice { seq: self.news_seq, text: text.into() });
        if self.news.len() > NEWS_KEPT {
            self.news.remove(0);
        }
    }

    fn name(&mut self, trader: &str, name: &str) {
        if !name.is_empty() && self.names.get(trader).is_none_or(|n| n != name) {
            self.names.insert(trader.to_string(), name.to_string());
        }
    }

    /// Whether `trader` takes an ace's bounty as salvage rights to its wreck, rather than pay.
    pub fn salvage_terms(&self, trader: &str) -> bool {
        self.salvage_terms.contains(trader)
    }

    /// How `trader` takes an ace's bounty from now on (MechWarrior's contract terms: pay, or
    /// salvage).
    pub fn set_terms(&mut self, trader: &str, salvage: bool) -> Done {
        if salvage {
            self.salvage_terms.insert(trader.to_string());
            Ok("TERMS: AN ACE'S WRECK FOR ITS BOUNTY · THE TUGS BRING IT HOME".into())
        } else {
            self.salvage_terms.remove(trader);
            Ok("TERMS: AN ACE'S BOUNTY, PAID".into())
        }
    }

    /// Zodiac's ace `ace` is out among the Dolls: the news.
    pub fn ace_out(&mut self, ace: u8) {
        let a = bc_sim::content::aces::ace(ace);
        self.announce(format!(
            "ZODIAC'S {} IS OUT AMONG THE DOLLS · {} CR ON IT",
            a.name,
            thousands(u64::from(a.bounty))
        ));
    }

    /// Ace `ace` was downed by `trader`, going by `name`: on the Most Wanted, the ladder and the
    /// news. Whether they take its bounty as the rights to its wreck (else as pay, [`Self::pay_ace`]).
    pub fn ace_downed(&mut self, ace: u8, trader: &str, name: &str, now: u64) -> bool {
        self.name(trader, name);
        self.wanted.insert(ace, Downed { by: name.to_string(), at: now });
        *self.aces.entry(trader.to_string()).or_default() += 1;
        let a = bc_sim::content::aces::ace(ace);
        self.announce(format!("ZODIAC'S {} DOWNED BY {}", a.name, name.to_uppercase()));
        self.salvage_terms(trader)
    }

    /// The colony pays `trader` the bounty on ace `ace` (a source, with the standing they earn).
    /// Its note.
    pub fn pay_ace(&mut self, hangar: &mut Hangar, ace: u8, trader: &str) -> String {
        let a = bc_sim::content::aces::ace(ace);
        let bounty = u64::from(a.bounty);
        self.colony_pays(trader, bounty);
        hangar.credits += bounty;
        format!("{} DOWNED · THE CHARTER BOARD PAYS {} CR", a.name, thousands(bounty))
    }

    fn name_of(&self, trader: &str) -> String {
        self.names.get(trader).cloned().unwrap_or_else(|| "A PILOT".into())
    }

    fn owe(&mut self, trader: &str) -> &mut Account {
        self.accounts.entry(trader.to_string()).or_default()
    }

    /// Credits the colony pays a pilot (a source), with the standing they earn.
    fn colony_pays(&mut self, trader: &str, credits: u64) {
        self.colony_paid += credits;
        *self.standing.entry(trader.to_string()).or_default() += credits;
    }

    /// Whether anything waits for `trader` to collect.
    pub fn owed(&self, trader: &str) -> bool {
        self.accounts.get(trader).is_some_and(|a| !a.is_empty())
    }

    /// Hands `trader` what they're owed. Its notes.
    pub fn collect(&mut self, hangar: &mut Hangar, trader: &str) -> Vec<String> {
        let Some(a) = self.accounts.remove(trader) else { return Vec::new() };
        hangar.credits += a.credits;
        for (item, qty) in a.goods {
            hangar.stores.add(item, qty);
        }
        a.notes
    }

    /// Credits and goods held here (escrow and accounts), for the ledger.
    pub fn held(&self) -> (u64, BTreeMap<Item, u64>) {
        let mut credits: u64 =
            self.contracts.values().filter(|c| c.issuer.is_some()).map(Contract::unpaid).sum();
        let mut goods: BTreeMap<Item, u64> = BTreeMap::new();
        for a in self.accounts.values() {
            credits += a.credits;
            for (item, q) in &a.goods {
                *goods.entry(*item).or_default() += q;
            }
        }
        (credits, goods)
    }

    fn post_contract(
        &mut self,
        issuer: Option<&str>,
        issuer_name: &str,
        task: Task,
        reward: u64,
        now: u64,
        secs: u64,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.contracts.insert(
            id,
            Contract {
                id,
                issuer: issuer.map(str::to_string),
                issuer_name: issuer_name.to_string(),
                task,
                reward,
                paid: 0,
                posted: now,
                expires: now + secs,
                holder: None,
            },
        );
        id
    }

    /// Closes a contract: what it hasn't paid goes back to a pilot issuer, with `why`.
    fn close(&mut self, id: u64, why: &str) {
        let Some(c) = self.contracts.remove(&id) else { return };
        if let Some(issuer) = &c.issuer {
            let back = c.unpaid();
            let a = self.owe(issuer);
            a.credits += back;
            a.notes.push(if back > 0 {
                format!("{} {why} · {} CR BACK FROM ESCROW", c.label(), thousands(back))
            } else {
                format!("{} {why}", c.label())
            });
        }
        if let Some(holder) = c.holder.as_deref().filter(|_| why == "EXPIRED") {
            let text = format!("{} (PATROL) RAN OUT OF TIME", c.label());
            self.owe(holder).notes.push(text);
        }
    }

    /// The clock at `now`: contracts expire, and the colony posts what it needs (its supply jobs,
    /// for what its desks run short of first; the militia's patrols). Whether anything changed.
    pub fn tick(&mut self, now: u64, ex: &Exchange) -> bool {
        let expired: Vec<u64> = self.contracts.values().filter(|c| c.expires <= now).map(|c| c.id).collect();
        let mut changed = !expired.is_empty();
        for id in expired {
            self.close(id, "EXPIRED");
        }
        let open = |b: &Board, patrol: bool| {
            b.contracts
                .values()
                .filter(|c| c.issuer.is_none() && matches!(c.task, Task::Patrol { .. }) == patrol)
                .count()
        };
        while open(self, false) < COLONY_SUPPLY {
            // What it's shortest of, among what it isn't already asking for.
            let asked: Vec<Item> = self
                .contracts
                .values()
                .filter_map(|c| match c.task {
                    Task::Supply { item, .. } if c.issuer.is_none() => Some(item),
                    _ => None,
                })
                .collect();
            let short = |item: Item| -> f64 {
                let target = crate::catalogue::desk(item).map_or(1.0, |d| d.target as f64);
                ex.colony_stock(item).unwrap_or(target) / target.max(1.0)
            };
            let Some(&(item, qty)) = SUPPLY
                .iter()
                .filter(|(i, _)| !asked.contains(i))
                .min_by(|a, b| short(a.0).total_cmp(&short(b.0)).then(a.0.cmp(&b.0)))
            else {
                break;
            };
            let task = Task::Supply { item, qty, delivered: 0 };
            self.post_contract(None, "THE COLONY", task, priced(item, qty, SUPPLY_PCT), now, SUPPLY_SECS);
            changed = true;
        }
        if self.effects().patrols {
            while open(self, true) < COLONY_PATROLS {
                let task = Task::Patrol { bounty: PATROL_BOUNTY, earned: 0 };
                self.post_contract(None, "THE MILITIA", task, PATROL_REWARD, now, PATROL_STANDS_SECS);
                changed = true;
            }
        }
        changed
    }

    /// A pilot posts a supply contract: `reward` credits for `qty` of `item`, standing `hours`.
    /// The reward goes into escrow from their hangar.
    #[allow(clippy::too_many_arguments)]
    pub fn post(
        &mut self,
        hangar: &mut Hangar,
        trader: &str,
        name: &str,
        item: Item,
        qty: u64,
        reward: u64,
        hours: u64,
        now: u64,
    ) -> Done {
        if !item.valid() {
            return refuse("no such item");
        }
        if qty == 0 || qty > crate::exchange::MAX_QTY {
            return refuse("that's not a quantity");
        }
        if reward == 0 {
            return refuse("a contract has to pay something");
        }
        if hours == 0 || hours > MAX_HOURS {
            return refuse(format!("a contract stands from 1 to {MAX_HOURS} hours"));
        }
        if self.contracts.values().filter(|c| c.issuer.as_deref() == Some(trader)).count() >= MAX_POSTED {
            return refuse(format!("you have {MAX_POSTED} contracts posted already"));
        }
        if hangar.credits < reward {
            return refuse(format!(
                "that needs {} cr, and you have {}",
                thousands(reward),
                thousands(hangar.credits)
            ));
        }
        hangar.credits -= reward;
        self.name(trader, name);
        let task = Task::Supply { item, qty, delivered: 0 };
        let id = self.post_contract(Some(trader), name, task, reward, now, hours * 3_600);
        Ok(format!(
            "CONTRACT #{id} POSTED: {} {} FOR {} CR (IN ESCROW)",
            item.amount(qty),
            item.name().to_uppercase(),
            thousands(reward)
        ))
    }

    /// The issuer takes their contract down: what it hasn't paid comes back.
    pub fn withdraw(&mut self, hangar: &mut Hangar, trader: &str, id: u64) -> Done {
        match self.contracts.get(&id) {
            Some(c) if c.issuer.as_deref() == Some(trader) => {}
            Some(_) => return refuse("that isn't your contract"),
            None => return refuse("no such contract"),
        }
        self.close(id, "WITHDRAWN");
        let notes = self.collect(hangar, trader);
        Ok(notes.join(" · "))
    }

    /// Delivers up to `qty` toward supply contract `id` from the hangar's stores, paid pro rata.
    #[allow(clippy::too_many_arguments)]
    pub fn deliver(
        &mut self,
        hangar: &mut Hangar,
        ex: &mut Exchange,
        trader: &str,
        name: &str,
        id: u64,
        qty: u64,
        now: u64,
    ) -> Done {
        let Some(c) = self.contracts.get(&id) else { return refuse("no such contract") };
        let Task::Supply { item, qty: wanted, delivered } = c.task else {
            return refuse("that's a patrol: take it, and fly it");
        };
        if c.issuer.as_deref() == Some(trader) {
            return refuse("that's your own contract");
        }
        if c.expires <= now {
            return refuse("that contract has expired");
        }
        let n = qty.min(wanted - delivered).min(hangar.stores.get(item));
        if n == 0 {
            return refuse(match item {
                Item::Part(..) => format!("you have no new {} to deliver", item.name()),
                _ => format!("you have no {} to deliver", item.name()),
            });
        }
        if !hangar.stores.take(item, n) {
            return refuse("the stores couldn't release that");
        }
        let pay = c.pay_for(delivered + n, wanted) - c.pay_for(delivered, wanted);
        let issuer = c.issuer.clone();
        let c = self.contracts.get_mut(&id).expect("looked up above");
        c.paid += pay;
        c.task = Task::Supply { item, qty: wanted, delivered: delivered + n };
        let filled = delivered + n == wanted;
        hangar.credits += pay;
        self.name(trader, name);
        match &issuer {
            None => {
                ex.colony_receive(item, n);
                self.colony_pays(trader, pay);
            }
            Some(issuer) => {
                let issuer = issuer.clone();
                let a = self.owe(&issuer);
                *a.goods.entry(item).or_default() += n;
                a.notes.push(format!(
                    "CONTRACT #{id}: {} DELIVERED {} {}",
                    name.to_uppercase(),
                    item.amount(n),
                    item.name().to_uppercase()
                ));
            }
        }
        if filled {
            if let Some(issuer) = issuer {
                self.owe(&issuer).notes.push(format!("CONTRACT #{id} FILLED"));
            }
            self.contracts.remove(&id);
        }
        Ok(format!(
            "DELIVERED {} {} · PAID {} CR{}",
            item.amount(n),
            item.name().to_uppercase(),
            thousands(pay),
            if filled { format!(" · CONTRACT #{id} FILLED") } else { String::new() }
        ))
    }

    /// Takes patrol `id`: the time starts now.
    pub fn take(&mut self, trader: &str, name: &str, id: u64, now: u64) -> Done {
        if self.contracts.values().any(|c| c.holder.as_deref() == Some(trader)) {
            return refuse("you're flying a patrol already");
        }
        let Some(c) = self.contracts.get_mut(&id) else { return refuse("no such contract") };
        if !matches!(c.task, Task::Patrol { .. }) {
            return refuse("there's nothing to take: deliver it");
        }
        if c.holder.is_some() {
            return refuse("another pilot is flying that patrol");
        }
        c.holder = Some(trader.to_string());
        c.expires = now + PATROL_SECS;
        let Task::Patrol { bounty, .. } = c.task else { unreachable!() };
        self.name(trader, name);
        Ok(format!(
            "PATROL #{id} TAKEN · DOWN DOLLS FOR {} CR OF BOUNTIES WITHIN THE HOUR",
            thousands(bounty)
        ))
    }

    /// Gives patrol `id` up: it goes back on the board.
    pub fn drop_patrol(&mut self, trader: &str, id: u64, now: u64) -> Done {
        let Some(c) = self.contracts.get_mut(&id) else { return refuse("no such contract") };
        if c.holder.as_deref() != Some(trader) {
            return refuse("that isn't your patrol");
        }
        c.holder = None;
        c.task = Task::Patrol { bounty: PATROL_BOUNTY, earned: 0 };
        c.expires = now + PATROL_STANDS_SECS;
        Ok(format!("PATROL #{id} GIVEN UP"))
    }

    /// A sortie of `trader`'s ended with `bounty` credits of bounties: it counts toward the patrol
    /// they hold, which pays (to their account) once it's flown. What to tell them.
    pub fn bounties(&mut self, trader: &str, bounty: u64, now: u64) -> Vec<String> {
        if bounty == 0 {
            return Vec::new();
        }
        let Some(c) =
            self.contracts.values_mut().find(|c| c.holder.as_deref() == Some(trader) && c.expires > now)
        else {
            return Vec::new();
        };
        let Task::Patrol { bounty: target, earned } = c.task else { return Vec::new() };
        let earned = (earned + bounty).min(target);
        c.task = Task::Patrol { bounty: target, earned };
        let id = c.id;
        if earned < target {
            return vec![format!("PATROL #{id}: {} OF {} CR", thousands(earned), thousands(target))];
        }
        let reward = c.reward;
        self.contracts.remove(&id);
        self.colony_pays(trader, reward);
        self.owe(trader).credits += reward;
        vec![format!("PATROL #{id} FLOWN · THE MILITIA PAYS {} CR", thousands(reward))]
    }

    /// Delivers up to `qty` of `item` toward great work `work`, paid by the colony.
    #[allow(clippy::too_many_arguments)]
    pub fn contribute(
        &mut self,
        hangar: &mut Hangar,
        trader: &str,
        name: &str,
        work: Work,
        item: Item,
        qty: u64,
        now: u64,
    ) -> Done {
        if work.era() != self.era || self.done(work) {
            return refuse(format!("{} is finished", work.name()));
        }
        let Some(&(_, need)) = work.needs().iter().find(|(i, _)| *i == item) else {
            return refuse(format!("{} doesn't need {}", work.name(), item.name()));
        };
        let p = self.progress(work);
        let have = p.delivered.get(&item).copied().unwrap_or(0);
        let n = qty.min(need.saturating_sub(have)).min(hangar.stores.get(item));
        if n == 0 {
            return refuse(if have >= need {
                format!("{} has all the {} it needs", work.name(), item.name())
            } else {
                format!("you have no {} to deliver", item.name())
            });
        }
        if !hangar.stores.take(item, n) {
            return refuse("the stores couldn't release that");
        }
        let pay = priced(item, n, WORKS_PCT);
        hangar.credits += pay;
        self.name(trader, name);
        self.colony_pays(trader, pay);
        let p = self.works.get_mut(&work).expect("every work has its progress");
        *p.delivered.entry(item).or_default() += n;
        *p.contributors.entry(trader.to_string()).or_default() += pay;
        let complete = work.needs().iter().all(|(i, q)| p.delivered.get(i).copied().unwrap_or(0) >= *q);
        let mut text = format!(
            "{}: DELIVERED {} {} · PAID {} CR",
            work.name().to_uppercase(),
            item.amount(n),
            item.name().to_uppercase(),
            thousands(pay)
        );
        if complete {
            p.done = Some(now);
            self.announce(work.finished());
            if self.vote_open() {
                self.announce("THE CHARTER VOTE IS OPEN · PILOTS OF STANDING, SIGN AT THE CHARTER BOARD");
            }
            text.push_str(" · FINISHED");
        }
        Ok(text)
    }

    /// Signs the charter.
    pub fn sign(&mut self, trader: &str, name: &str, now: u64) -> Done {
        if self.done(Work::CharterVote) {
            return refuse("the charter is signed");
        }
        if !self.vote_open() {
            return refuse("the vote opens once the colony's great works are finished");
        }
        if self.standing(trader) == 0 {
            return refuse("only pilots of standing sign: deliver to the colony's contracts or works first");
        }
        if self.signatures.iter().any(|t| t == trader) {
            return refuse("you've signed already");
        }
        self.name(trader, name);
        self.signatures.push(trader.to_string());
        let n = self.signatures.len();
        if n < SIGNATURES {
            return Ok(format!("YOU SIGNED THE CHARTER · {n} OF {SIGNATURES}"));
        }
        let p = self.works.get_mut(&Work::CharterVote).expect("every work has its progress");
        p.done = Some(now);
        self.era = 1;
        self.announce(Work::CharterVote.finished());
        Ok("YOU SIGNED THE CHARTER · IT IS ADOPTED".into())
    }

    /// The board as `trader` sees it at `now`.
    pub fn view(&self, trader: &str, now: u64) -> CharterView {
        let mut list: Vec<&Contract> = self.contracts.values().collect();
        // Their own and the one they hold first, then the colony's, then the newest.
        list.sort_by_key(|c| {
            let mine = c.issuer.as_deref() == Some(trader) || c.holder.as_deref() == Some(trader);
            (!mine, c.issuer.is_some(), std::cmp::Reverse(c.id))
        });
        let contracts = list
            .into_iter()
            .take(VIEW_CONTRACTS)
            .map(|c| ContractView {
                id: c.id,
                issuer: c.issuer_name.clone(),
                colony: c.issuer.is_none(),
                mine: c.issuer.as_deref() == Some(trader),
                task: c.task,
                reward: c.reward,
                paid: c.paid,
                secs_left: c.expires.saturating_sub(now),
                holder: c.holder.as_deref().map(|h| self.name_of(h)),
                held: c.holder.as_deref() == Some(trader),
            })
            .collect();
        let works = Work::ALL
            .into_iter()
            .filter(|w| w.era() == self.era || self.done(*w))
            .map(|w| {
                let p = self.progress(w);
                let mut top: Vec<(String, u64)> =
                    p.contributors.iter().map(|(t, s)| (self.name_of(t), *s)).collect();
                top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                top.truncate(5);
                WorkView {
                    work: w,
                    name: w.name().into(),
                    summary: w.summary().into(),
                    needs: w
                        .needs()
                        .iter()
                        .map(|(i, q)| (*i, *q, p.delivered.get(i).copied().unwrap_or(0)))
                        .collect(),
                    open: if w == Work::CharterVote { self.vote_open() } else { !self.done(w) },
                    done: self.done(w),
                    top,
                    mine: p.contributors.get(trader).copied().unwrap_or(0),
                }
            })
            .collect();
        CharterView {
            era: self.era,
            era_name: era_name(self.era),
            contracts,
            works,
            standing: self.standing(trader),
            signatures: self.signatures.len(),
            signatures_needed: SIGNATURES,
            signed: self.signatures.iter().any(|t| t == trader),
            max_posted: MAX_POSTED,
            supply_pct: SUPPLY_PCT,
            works_pct: WORKS_PCT,
            wanted: bc_sim::content::aces::ACES
                .iter()
                .enumerate()
                .map(|(k, a)| WantedView {
                    ace: k as u8,
                    name: a.name.into(),
                    bounty: a.bounty,
                    out: false,
                    last: self.wanted.get(&(k as u8)).cloned(),
                })
                .collect(),
            ladder: {
                let mut rows: Vec<(String, u32)> =
                    self.aces.iter().map(|(t, n)| (self.name_of(t), *n)).collect();
                rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                rows.truncate(LADDER_KEPT);
                rows
            },
            mine_aces: self.aces.get(trader).copied().unwrap_or(0),
            salvage_terms: self.salvage_terms(trader),
        }
    }
}

/// The ladder's length, as the board shows it.
pub const LADDER_KEPT: usize = 10;

/// One of Zodiac's aces on the Most Wanted, as a pilot sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WantedView {
    pub ace: u8,
    pub name: String,
    pub bounty: u32,
    /// Out among the Dolls now (the server's to say).
    #[serde(default)]
    pub out: bool,
    /// Who downed it last, and when.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<Downed>,
}

/// A contract, as a pilot sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractView {
    pub id: u64,
    pub issuer: String,
    pub colony: bool,
    /// They posted it.
    pub mine: bool,
    pub task: Task,
    pub reward: u64,
    pub paid: u64,
    pub secs_left: u64,
    /// A patrol: the name of who's flying it, and whether it's them.
    pub holder: Option<String>,
    pub held: bool,
}

/// A great work, as a pilot sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkView {
    pub work: Work,
    pub name: String,
    pub summary: String,
    /// Each item it needs: how much, and how much is delivered.
    pub needs: Vec<(Item, u64, u64)>,
    pub open: bool,
    pub done: bool,
    /// Its most generous contributors (their names, and the standing they earned on it), and the
    /// pilot's own.
    pub top: Vec<(String, u64)>,
    pub mine: u64,
}

/// The board, as a pilot sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharterView {
    pub era: u8,
    pub era_name: String,
    pub contracts: Vec<ContractView>,
    pub works: Vec<WorkView>,
    pub standing: u64,
    pub signatures: usize,
    pub signatures_needed: usize,
    pub signed: bool,
    pub max_posted: usize,
    pub supply_pct: u64,
    pub works_pct: u64,
    /// The Most Wanted: Zodiac's aces...
    #[serde(default)]
    pub wanted: Vec<WantedView>,
    /// ...the ladder, by aces downed (names and counts)...
    #[serde(default)]
    pub ladder: Vec<(String, u32)>,
    /// ...the pilot's own count, and their terms (salvage rights, or pay).
    #[serde(default)]
    pub mine_aces: u32,
    #[serde(default)]
    pub salvage_terms: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rich() -> Hangar {
        let mut h = Hangar { credits: 1_000_000, ..Hangar::default() };
        for (item, _) in SUPPLY {
            h.stores.add(item, 100_000);
        }
        for w in Work::ALL {
            for (item, q) in w.needs() {
                h.stores.add(*item, q * 2);
            }
        }
        h
    }

    #[test]
    fn the_colony_posts_what_it_is_shortest_of() {
        let mut b = Board::new();
        let mut ex = Exchange::new();
        // Its exotics run short.
        let _ = ex.colony_take(Item::Ore(Ore::Exotics), 4_000);
        assert!(b.tick(0, &ex));
        let supply: Vec<Item> = b
            .contracts()
            .filter_map(|c| match c.task {
                Task::Supply { item, .. } => Some(item),
                _ => None,
            })
            .collect();
        assert_eq!(supply.len(), COLONY_SUPPLY);
        assert_eq!(supply[0], Item::Ore(Ore::Exotics));
        // No patrols before the militia has a hangar.
        assert!(b.contracts().all(|c| matches!(c.task, Task::Supply { .. })));
        assert!(!b.tick(10, &ex), "nothing to post twice");
        // They expire, and are posted again.
        assert!(b.tick(SUPPLY_SECS, &ex));
        assert_eq!(b.contracts().count(), COLONY_SUPPLY);
        assert!(b.contracts().all(|c| c.posted == SUPPLY_SECS));
    }

    #[test]
    fn supply_is_paid_pro_rata_and_goes_to_its_issuer() {
        let mut b = Board::new();
        let mut ex = Exchange::new();
        let (mut alice, mut bob) = (rich(), rich());
        let steel = STEEL;
        let before = alice.credits;
        b.post(&mut alice, "a", "Alice", steel, 1_000, 999, 2, 0).unwrap();
        assert_eq!(alice.credits, before - 999);
        let id = b.contracts().find(|c| c.issuer.is_some()).unwrap().id;
        assert!(b.deliver(&mut alice, &mut ex, "a", "Alice", id, 10, 1).is_err(), "not her own");
        let b0 = bob.credits;
        b.deliver(&mut bob, &mut ex, "b", "Bob", id, 333, 1).unwrap();
        assert_eq!(bob.credits - b0, 332);
        b.deliver(&mut bob, &mut ex, "b", "Bob", id, 5_000, 2).unwrap();
        // All of it paid, none of it made.
        assert_eq!(bob.credits - b0, 999);
        assert!(b.contract(id).is_none());
        assert_eq!(b.standing("b"), 0, "a pilot's contract earns no standing with the colony");
        let s0 = alice.stores.get(steel);
        let notes = b.collect(&mut alice, "a");
        assert_eq!(alice.stores.get(steel) - s0, 1_000);
        assert!(notes.iter().any(|n| n.contains("FILLED")), "{notes:?}");
    }

    #[test]
    fn what_a_contract_has_not_paid_goes_back() {
        let mut b = Board::new();
        let mut ex = Exchange::new();
        let (mut alice, mut bob) = (rich(), rich());
        let c0 = alice.credits;
        b.post(&mut alice, "a", "Alice", STEEL, 100, 1_000, 1, 0).unwrap();
        let id = b.contracts().next().unwrap().id;
        b.deliver(&mut bob, &mut ex, "b", "Bob", id, 25, 1).unwrap();
        b.tick(3_600, &ex);
        assert!(b.contract(id).is_none());
        b.collect(&mut alice, "a");
        assert_eq!(c0 - alice.credits, 250);
        assert!(b.withdraw(&mut alice, "a", id).is_err());
        b.post(&mut alice, "a", "Alice", STEEL, 100, 1_000, 1, 0).unwrap();
        let id = b.contracts().find(|c| c.issuer.is_some()).unwrap().id;
        assert!(b.withdraw(&mut bob, "b", id).is_err());
        b.withdraw(&mut alice, "a", id).unwrap();
        assert_eq!(c0 - alice.credits, 250);
        assert_eq!(b.held(), (0, BTreeMap::new()));
    }

    #[test]
    fn the_great_works_change_the_colony_and_the_vote_begins_the_calendar() {
        let mut b = Board::new();
        let mut ex = Exchange::new();
        let pilots = ["a", "b", "c", "d"];
        let mut h: Vec<Hangar> = pilots.iter().map(|_| rich()).collect();
        assert!(b.sign("a", "A", 0).is_err(), "not before the works");
        assert_eq!(b.effects().foundry_fee_pct, 100);
        for (k, w) in [Work::SecondFoundry, Work::MilitiaHangar].into_iter().enumerate() {
            for (item, q) in w.needs() {
                // In two goes, by two pilots.
                b.contribute(&mut h[k], pilots[k], "P", w, *item, q / 2, 5).unwrap();
                b.contribute(&mut h[k + 1], pilots[k + 1], "Q", w, *item, *q, 6).unwrap();
                assert!(b.contribute(&mut h[k], pilots[k], "P", w, *item, 1, 7).is_err() || !b.done(w));
            }
            assert!(b.done(w), "{w:?}");
        }
        let e = b.effects();
        assert_eq!((e.foundry_fee_pct, e.foundry_speed, e.patrols), (50, 2.0, true));
        assert!(b.vote_open());
        assert!(b.news_since(0).any(|n| n.text.contains("VOTE IS OPEN")));
        // The militia posts patrols now.
        b.tick(10, &ex);
        assert_eq!(b.contracts().filter(|c| matches!(c.task, Task::Patrol { .. })).count(), COLONY_PATROLS);
        // Only pilots of standing sign, each once.
        assert!(b.sign("d", "D", 20).is_err());
        b.sign("a", "A", 20).unwrap();
        assert!(b.sign("a", "A", 20).is_err());
        b.sign("b", "B", 21).unwrap();
        // c earns standing on a colony contract.
        let id = b.contracts().find(|c| matches!(c.task, Task::Supply { .. })).unwrap().id;
        b.deliver(&mut h[2], &mut ex, "c", "C", id, 100, 22).unwrap();
        assert!(b.standing("c") > 0);
        assert_eq!(b.era(), 0);
        b.sign("c", "C", 23).unwrap();
        assert_eq!(b.era(), 1);
        assert!(b.news_since(0).last().unwrap().text.contains("AC 1"));
        assert!(b.sign("d", "D", 24).is_err());
        assert_eq!(b.view("a", 30).era_name, "AC 1 · THE CHARTER");
    }

    #[test]
    fn a_patrol_pays_once_its_bounties_are_earned_in_time() {
        let mut b = Board::new();
        let ex = Exchange::new();
        b.works.get_mut(&Work::MilitiaHangar).unwrap().done = Some(0);
        b.tick(0, &ex);
        let ids: Vec<u64> =
            b.contracts().filter(|c| matches!(c.task, Task::Patrol { .. })).map(|c| c.id).collect();
        b.take("a", "A", ids[0], 100).unwrap();
        assert!(b.take("a", "A", ids[1], 100).is_err(), "one at a time");
        assert!(b.take("b", "B", ids[0], 100).is_err(), "taken");
        assert_eq!(b.bounties("a", 250, 200).len(), 1);
        assert!(b.bounties("b", 5_000, 200).is_empty());
        let notes = b.bounties("a", 800, 300);
        assert!(notes[0].contains("FLOWN"), "{notes:?}");
        let mut h = Hangar::default();
        b.collect(&mut h, "a");
        assert_eq!(h.credits, PATROL_REWARD);
        assert_eq!(b.colony_paid, PATROL_REWARD);
        // Another runs out of time: it goes back on the board.
        b.take("b", "B", ids[1], 1_000).unwrap();
        b.tick(1_000 + PATROL_SECS, &ex);
        assert!(b.contract(ids[1]).is_none());
        assert_eq!(b.contracts().filter(|c| matches!(c.task, Task::Patrol { .. })).count(), COLONY_PATROLS);
        assert!(b.owed("b"));
    }

    /// The Most Wanted (`docs/DESIGN.md`, "Aces"): an ace downed goes on the list, the ladder and
    /// the news; its bounty is the colony's to pay (a source, with standing) unless the pilot's terms
    /// take its wreck instead.
    #[test]
    fn the_most_wanted_pays_or_leaves_the_wreck_and_keeps_the_ladder() {
        use bc_sim::content::aces::ACES;
        let mut b = Board::new();
        let mut h = Hangar::default();
        // Paid, unless the pilot says otherwise.
        assert!(!b.salvage_terms("a"));
        assert!(!b.ace_downed(0, "a", "Ann", 100));
        let note = b.pay_ace(&mut h, 0, "a");
        let bounty = u64::from(ACES[0].bounty);
        assert_eq!(h.credits, bounty);
        assert_eq!(b.colony_paid, bounty, "the colony pays it");
        assert_eq!(b.standing("a"), bounty);
        assert_eq!(note, "ARIES DOWNED · THE CHARTER BOARD PAYS 1,500 CR");
        assert!(b.news_since(0).any(|n| n.text == "ZODIAC'S ARIES DOWNED BY ANN"));
        // Its wreck instead, then pay again.
        assert!(b.set_terms("b", true).unwrap().contains("WRECK"));
        assert!(b.salvage_terms("b"));
        assert!(b.ace_downed(1, "b", "Bo", 200));
        assert!(b.ace_downed(2, "b", "Bo", 300));
        b.set_terms("b", false).unwrap();
        assert!(!b.ace_downed(3, "b", "Bo", 400));
        // The view: the list, who downed each last, the ladder by aces downed, theirs and their terms.
        b.set_terms("b", true).unwrap();
        let v = b.view("b", 500);
        assert_eq!(v.wanted.len(), ACES.len());
        assert_eq!((v.wanted[0].name.as_str(), v.wanted[0].bounty), ("ARIES", ACES[0].bounty));
        assert_eq!(v.wanted[0].last, Some(Downed { by: "Ann".into(), at: 100 }));
        assert_eq!(v.wanted[4].last, None);
        assert!(v.wanted.iter().all(|w| !w.out), "which is out is the server's to say");
        assert_eq!(v.ladder, [("Bo".to_string(), 3), ("Ann".to_string(), 1)]);
        assert_eq!((v.mine_aces, v.salvage_terms), (3, true));
        assert_eq!((b.view("a", 500).mine_aces, b.view("a", 500).salvage_terms), (1, false));
        // Downed again: by whoever downed it last.
        b.ace_downed(0, "b", "Bo", 600);
        assert_eq!(b.view("a", 700).wanted[0].last, Some(Downed { by: "Bo".into(), at: 600 }));
        // Kept with the board.
        let saved = serde_json::to_vec(&b).unwrap();
        assert_eq!(serde_json::from_slice::<Board>(&saved).unwrap(), b);
        // The ladder shows its top.
        for k in 0..20 {
            b.ace_downed(5, &format!("p{k}"), &format!("P{k}"), 800);
        }
        let ladder = b.view("a", 900).ladder;
        assert_eq!(ladder.len(), LADDER_KEPT);
        assert_eq!(ladder[0], ("Bo".to_string(), 4));
    }

    #[test]
    fn a_board_and_its_view_fit_in_a_frame_and_read_back() {
        let mut b = Board::new();
        let mut ex = Exchange::new();
        b.tick(0, &ex);
        for k in 0..60 {
            let mut h = rich();
            let t = format!("pilot-{k}");
            let _ = b.post(&mut h, &t, "Someone With A Long Name", STEEL, 1_000 + k, 5_000, 72, 0);
            let _ = b.contribute(&mut h, &t, "Someone", Work::SecondFoundry, ALLOY, 10, 0);
            b.ace_downed((k % 9) as u8, &t, "Someone With A Long Name", 0);
        }
        let id = b.contracts().next().unwrap().id;
        let mut h = rich();
        b.deliver(&mut h, &mut ex, "x", "X", id, 1, 1).unwrap();
        let view = crate::wire::encode(&crate::wire::Update::Charter(b.view("pilot-3", 5)));
        assert!(view.len() < 16_000, "{}", view.len());
        let back: crate::wire::Update = crate::wire::decode(&view).unwrap();
        let crate::wire::Update::Charter(v) = back else { panic!() };
        assert_eq!(v.contracts.len(), VIEW_CONTRACTS);
        assert!(v.contracts[0].mine);
        let saved = serde_json::to_vec(&b).unwrap();
        assert_eq!(serde_json::from_slice::<Board>(&saved).unwrap(), b);
    }
}
