//! The Colony 03 Exchange: an order book per item, where pilots trade with each other and with the
//! colony.
//!
//! - **Pilots** place limit orders. An order that crosses the book trades at once, at the resting
//!   order's price, best price first and oldest first at a price; what's left rests (or, asked
//!   for, is handed back). A sell order holds its goods and a buy order its credits in escrow
//!   while it rests, so nobody can sell what they no longer have or buy what they can't pay for.
//! - **The colony** is a trader too, with a desk for what it deals in (`catalogue::desk`). It
//!   quotes a bid and an ask off its stock: buying drives its stock up and its prices down,
//!   selling the other way, a step at a time. Its stock settles back to what it wants over hours:
//!   what it consumes when it has too much (a sink), what it imports when it has too little (a
//!   source). So prices move with what pilots bring in and take out, and drift back.
//! - The colony won't touch Gundam technology: gundanium, and the Gundams' parts and weapons trade
//!   only between pilots.
//! - **Fees.** A seller pays the colony 2% of every sale: credits leave the economy.
//! - What a trader is owed (goods bought, sale proceeds, escrow handed back) waits in their
//!   account here until they collect it, so orders fill while their owners are away.
//!
//! Prices are credits per tonne for bulk goods (ores, materials), per piece otherwise;
//! quantities are kilograms or pieces (`catalogue::worth`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::catalogue::{Desk, desk, worth};
use crate::item::Item;

/// Who trades: a pilot's key (their wallet address, or a guest's session).
pub type Trader = String;

/// The colony's fee on every sale, in basis points of its value.
pub const FEE_BP: u64 = 200;
/// Most open orders a trader may have.
pub const MAX_ORDERS: usize = 32;
/// Samples of each item's price kept for its chart (one a minute).
pub const HISTORY: usize = 60;
/// Seconds between price samples.
pub const SAMPLE_SECS: f64 = 60.0;
/// The largest price and quantity an order may name.
pub const MAX_PRICE: u64 = 1_000_000_000_000;
pub const MAX_QTY: u64 = 1_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
}

/// An order resting in a book.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub id: u64,
    pub trader: Trader,
    pub item: Item,
    pub side: Side,
    pub price: u64,
    /// Still wanted, or still on offer.
    pub qty: u64,
    /// A buy order's credits held against it.
    pub escrow: u64,
    /// For time priority.
    pub seq: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Book {
    /// Highest first, oldest first at a price.
    bids: Vec<Order>,
    /// Lowest first, oldest first at a price.
    asks: Vec<Order>,
}

impl Book {
    fn insert(&mut self, order: Order) {
        // Behind every order at its price or better.
        let (list, at) = match order.side {
            Side::Buy => {
                let at = self.bids.iter().position(|o| order.price > o.price);
                (&mut self.bids, at)
            }
            Side::Sell => {
                let at = self.asks.iter().position(|o| order.price < o.price);
                (&mut self.asks, at)
            }
        };
        let at = at.unwrap_or(list.len());
        list.insert(at, order);
    }
}

/// What a trader is owed, waiting to be collected.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub credits: u64,
    pub goods: BTreeMap<Item, u64>,
    /// What happened (fills), for the trader to read.
    pub notes: Vec<String>,
}

impl Account {
    pub fn is_empty(&self) -> bool {
        self.credits == 0 && self.goods.is_empty() && self.notes.is_empty()
    }
}

/// An item's recent prices.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Tape {
    last: Option<u64>,
    /// Quantity traded, all time.
    volume: u64,
    /// A sample a minute, oldest first.
    samples: Vec<u64>,
}

/// One trade, as it happened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fill {
    pub item: Item,
    pub price: u64,
    pub qty: u64,
    /// `None`: the colony.
    pub buyer: Option<Trader>,
    pub seller: Option<Trader>,
    /// Paid by the seller.
    pub fee: u64,
}

/// Why an order was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    NoSuchItem,
    /// A price or quantity of 0, or beyond [`MAX_PRICE`] / [`MAX_QTY`].
    Bounds,
    /// The escrow given doesn't cover the order.
    Escrow,
    TooManyOrders,
}

impl Refused {
    pub fn text(&self) -> &'static str {
        match self {
            Refused::NoSuchItem => "no such item on the exchange",
            Refused::Bounds => "that price or quantity isn't possible",
            Refused::Escrow => "not enough to cover the order",
            Refused::TooManyOrders => "too many open orders",
        }
    }
}

/// What an order did: its fills, and the part that rests (if it does).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Placed {
    pub fills: Vec<Fill>,
    /// The id it rests under, if some of it rests.
    pub resting: Option<u64>,
}

/// The colony's side of one item.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct Stock {
    held: f64,
}

/// The exchange.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Exchange {
    books: BTreeMap<Item, Book>,
    colony: BTreeMap<Item, Stock>,
    accounts: BTreeMap<Trader, Account>,
    tapes: BTreeMap<Item, Tape>,
    next_id: u64,
    seq: u64,
    /// Seconds of trading so far (the clock the colony's stock settles on).
    clock: f64,
    next_sample: f64,
    /// Fees collected, credits the colony has paid out and taken in (for the ledger).
    pub fees: u64,
    pub colony_paid: u64,
    pub colony_took: u64,
}

impl Default for Exchange {
    /// A new exchange ([`Exchange::new`]).
    fn default() -> Self {
        Self::new()
    }
}

/// The colony's price for `side` of `item` with `held` in stock: its bid (it buys) or its ask
/// (it sells), if it does.
fn colony_price(d: &Desk, held: f64, side: Side) -> Option<u64> {
    let target = d.target as f64;
    let s = held.max(target / 32.0).max(1e-9);
    let mid = (d.base as f64 * (target / s).powf(0.6)).clamp(d.base as f64 / 5.0, d.base as f64 * 5.0);
    match side {
        Side::Sell => d.buys.then(|| (mid * 0.9).floor().max(1.0) as u64),
        Side::Buy => (d.sells && held >= 1.0).then(|| (mid * 1.1).ceil() as u64),
    }
}

/// How much the colony trades at one price before it re-prices.
fn colony_step(d: &Desk) -> u64 {
    (d.target / 50).max(1)
}

impl Exchange {
    /// A new exchange, the colony holding what it wants of everything.
    pub fn new() -> Self {
        let colony = Item::all()
            .into_iter()
            .filter_map(|item| desk(item).map(|d| (item, Stock { held: d.target as f64 })))
            .collect();
        Self {
            books: BTreeMap::new(),
            colony,
            accounts: BTreeMap::new(),
            tapes: BTreeMap::new(),
            next_id: 1,
            seq: 0,
            clock: 0.0,
            next_sample: 0.0,
            fees: 0,
            colony_paid: 0,
            colony_took: 0,
        }
    }

    /// Stocks the colony's desk for anything it deals in but holds no record of: an exchange kept
    /// from before the item existed would otherwise quote it as if the colony had none (dear, and
    /// never settling). Returns how many desks it opened.
    pub fn seed_missing(&mut self) -> usize {
        let mut n = 0;
        for item in Item::all() {
            if let Some(d) = desk(item)
                && !self.colony.contains_key(&item)
            {
                self.colony.insert(item, Stock { held: d.target as f64 });
                n += 1;
            }
        }
        n
    }

    /// Credits a buy order must hold in escrow.
    pub fn escrow_for(item: Item, price: u64, qty: u64) -> u64 {
        worth(item, price, qty)
    }

    fn held(&self, item: Item) -> f64 {
        self.colony.get(&item).map_or(0.0, |s| s.held)
    }

    /// The colony's quote on `side` (its bid, for a seller; its ask, for a buyer) and how much it
    /// deals at that price.
    fn colony_quote(&self, item: Item, side: Side) -> Option<(u64, u64)> {
        let d = desk(item)?;
        let held = self.held(item);
        let price = colony_price(&d, held, side)?;
        let qty = match side {
            Side::Sell => colony_step(&d),
            Side::Buy => colony_step(&d).min(held.floor() as u64),
        };
        (qty > 0).then_some((price, qty))
    }

    /// Places an order. `escrow` is what the trader has handed over for it: for a buy, credits
    /// (at least [`Exchange::escrow_for`]); for a sell, the goods themselves are taken to be
    /// handed over (`qty` of them) and `escrow` is 0. With `rest`, what doesn't fill at once rests
    /// in the book; otherwise it's handed back (to the trader's account).
    #[allow(clippy::too_many_arguments)]
    pub fn place(
        &mut self,
        trader: &str,
        item: Item,
        side: Side,
        price: u64,
        qty: u64,
        escrow: u64,
        rest: bool,
    ) -> Result<Placed, Refused> {
        if !item.valid() {
            return Err(Refused::NoSuchItem);
        }
        if price == 0 || qty == 0 || price > MAX_PRICE || qty > MAX_QTY {
            return Err(Refused::Bounds);
        }
        if side == Side::Buy && escrow < Self::escrow_for(item, price, qty) {
            return Err(Refused::Escrow);
        }
        if rest && self.orders(trader).count() >= MAX_ORDERS {
            return Err(Refused::TooManyOrders);
        }
        self.seq += 1;
        let mut order = Order {
            id: self.next_id,
            trader: trader.to_string(),
            item,
            side,
            price,
            qty,
            escrow: if side == Side::Buy { escrow } else { 0 },
            seq: self.seq,
        };
        self.next_id += 1;
        let mut placed = Placed::default();
        while order.qty > 0 {
            let Some(fill) = self.match_one(&mut order) else { break };
            placed.fills.push(fill);
        }
        if order.qty > 0 && rest {
            placed.resting = Some(order.id);
            self.books.entry(item).or_default().insert(order);
        } else {
            self.hand_back(&order);
        }
        Ok(placed)
    }

    /// Fills the best counter-offer against `order`, if one crosses it.
    fn match_one(&mut self, order: &mut Order) -> Option<Fill> {
        let item = order.item;
        // The best resting order on the other side, and the colony's quote.
        let book = self.books.entry(item).or_default();
        let resting = match order.side {
            Side::Buy => book.asks.first().map(|o| (o.price, o.qty)),
            Side::Sell => book.bids.first().map(|o| (o.price, o.qty)),
        };
        let colony = self.colony_quote(item, order.side);
        let better = |a: u64, b: u64| match order.side {
            Side::Buy => a < b,
            Side::Sell => a > b,
        };
        // Pilots first at an equal price.
        let from_colony = match (resting, colony) {
            (Some((p, _)), Some((c, _))) => better(c, p),
            (None, Some(_)) => true,
            (Some(_), None) => false,
            (None, None) => return None,
        };
        let (price, available) = if from_colony { colony? } else { resting? };
        let crosses = match order.side {
            Side::Buy => price <= order.price,
            Side::Sell => price >= order.price,
        };
        if !crosses {
            return None;
        }
        let qty = order.qty.min(available);
        let value = worth(item, price, qty);
        let (buyer, seller) = if from_colony {
            let d = desk(item)?;
            let stock = self.colony.entry(item).or_insert(Stock { held: d.target as f64 });
            match order.side {
                Side::Buy => {
                    stock.held -= qty as f64;
                    self.colony_took += value;
                    (Some(order.trader.clone()), None)
                }
                Side::Sell => {
                    stock.held += qty as f64;
                    self.colony_paid += value;
                    (None, Some(order.trader.clone()))
                }
            }
        } else {
            let book = self.books.get_mut(&item)?;
            let list = match order.side {
                Side::Buy => &mut book.asks,
                Side::Sell => &mut book.bids,
            };
            let maker = &mut list[0];
            maker.qty -= qty;
            let maker_trader = maker.trader.clone();
            if order.side == Side::Sell {
                // The resting buyer pays out of its escrow.
                maker.escrow -= value;
            }
            let done = maker.qty == 0;
            let finished = if done { Some(list.remove(0)) } else { None };
            if let Some(f) = finished {
                self.hand_back(&f);
            }
            match order.side {
                Side::Buy => (Some(order.trader.clone()), Some(maker_trader)),
                Side::Sell => (Some(maker_trader), Some(order.trader.clone())),
            }
        };
        order.qty -= qty;
        if order.side == Side::Buy {
            order.escrow -= value;
        }
        let fee = if seller.is_some() { value * FEE_BP / 10_000 } else { 0 };
        self.fees += fee;
        if let Some(b) = &buyer {
            let a = self.accounts.entry(b.clone()).or_default();
            *a.goods.entry(item).or_default() += qty;
            a.notes.push(format!("BOUGHT {} {} FOR {} CR", item.amount(qty), item.name(), value));
        }
        if let Some(s) = &seller {
            let a = self.accounts.entry(s.clone()).or_default();
            a.credits += value - fee;
            a.notes.push(format!(
                "SOLD {} {} FOR {} CR (FEE {})",
                item.amount(qty),
                item.name(),
                value - fee,
                fee
            ));
        }
        let tape = self.tapes.entry(item).or_default();
        tape.last = Some(price);
        tape.volume += qty;
        Some(Fill { item, price, qty, buyer, seller, fee })
    }

    /// Returns what an order no longer needs to its trader's account: a buy's unspent escrow, a
    /// sell's unsold goods.
    fn hand_back(&mut self, order: &Order) {
        let a = self.accounts.entry(order.trader.clone()).or_default();
        match order.side {
            Side::Buy => a.credits += order.escrow,
            Side::Sell => {
                if order.qty > 0 {
                    *a.goods.entry(order.item).or_default() += order.qty;
                }
            }
        }
        if a.is_empty() {
            self.accounts.remove(&order.trader);
        }
    }

    /// Cancels one of `trader`'s orders; what it held goes back to their account.
    pub fn cancel(&mut self, trader: &str, id: u64) -> bool {
        let mut found = None;
        for book in self.books.values_mut() {
            for list in [&mut book.bids, &mut book.asks] {
                if let Some(i) = list.iter().position(|o| o.id == id && o.trader == trader) {
                    found = Some(list.remove(i));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        match found {
            Some(order) => {
                self.hand_back(&order);
                true
            }
            None => false,
        }
    }

    /// Cancels every order `trader` has (a guest leaving).
    pub fn cancel_all(&mut self, trader: &str) {
        let ids: Vec<u64> = self.orders(trader).map(|o| o.id).collect();
        for id in ids {
            self.cancel(trader, id);
        }
    }

    /// Takes what `trader` is owed.
    pub fn collect(&mut self, trader: &str) -> Account {
        self.accounts.remove(trader).unwrap_or_default()
    }

    /// Whether `trader` has something to collect.
    pub fn owed(&self, trader: &str) -> bool {
        self.accounts.get(trader).is_some_and(|a| !a.is_empty())
    }

    /// `trader`'s open orders.
    pub fn orders<'a>(&'a self, trader: &'a str) -> impl Iterator<Item = &'a Order> + 'a {
        self.books
            .values()
            .flat_map(|b| b.bids.iter().chain(b.asks.iter()))
            .filter(move |o| o.trader == trader)
    }

    /// Runs the clock on by `dt` seconds: the colony's stocks settle towards what it wants, and
    /// prices are sampled once a minute.
    pub fn tick(&mut self, dt: f64) {
        let dt = dt.max(0.0);
        self.clock += dt;
        for (item, stock) in &mut self.colony {
            if let Some(d) = desk(*item) {
                let k = 1.0 - (-dt / (d.settle_hours * 3_600.0)).exp();
                stock.held += (d.target as f64 - stock.held) * k;
            }
        }
        while self.clock >= self.next_sample {
            self.next_sample += SAMPLE_SECS;
            for item in Item::all() {
                if let Some(p) = self.mark(item) {
                    let tape = self.tapes.entry(item).or_default();
                    tape.samples.push(p);
                    if tape.samples.len() > HISTORY {
                        tape.samples.remove(0);
                    }
                }
            }
        }
    }

    /// The best bid and ask for `item` (pilots' or the colony's): each its price and how much is
    /// there.
    pub fn top(&self, item: Item) -> (Option<Top>, Option<Top>) {
        let book = self.books.get(&item);
        let level = |list: Option<&Vec<Order>>| {
            let list = list?;
            let p = list.first()?.price;
            Some((p, list.iter().take_while(|o| o.price == p).map(|o| o.qty).sum()))
        };
        let pick = |a: Option<(u64, u64)>, b: Option<(u64, u64)>, higher: bool| match (a, b) {
            (Some(x), Some(y)) if x.0 == y.0 => Some((x.0, x.1 + y.1)),
            (Some(x), Some(y)) => Some(if (x.0 > y.0) == higher { x } else { y }),
            (x, None) => x,
            (None, y) => y,
        };
        let bid = pick(level(book.map(|b| &b.bids)), self.colony_quote(item, Side::Sell), true);
        let ask = pick(level(book.map(|b| &b.asks)), self.colony_quote(item, Side::Buy), false);
        (bid, ask)
    }

    /// A fair price for `item` now: the middle of the market, else its last trade.
    pub fn mark(&self, item: Item) -> Option<u64> {
        match self.top(item) {
            (Some((b, _)), Some((a, _))) => Some((b + a) / 2),
            (Some((p, _)), None) | (None, Some((p, _))) => Some(p),
            (None, None) => self.tapes.get(&item).and_then(|t| t.last),
        }
    }

    /// The book for `item`, `levels` deep each side (the colony's quote as one of them).
    pub fn depth(&self, item: Item, levels: usize) -> Depth {
        let mut bids: Vec<Level> = Vec::new();
        let mut asks: Vec<Level> = Vec::new();
        let add = |v: &mut Vec<Level>, price: u64, qty: u64, colony: bool| match v
            .iter_mut()
            .find(|l| l.price == price)
        {
            Some(l) => {
                l.qty += qty;
                l.colony |= colony;
            }
            None => v.push(Level { price, qty, colony }),
        };
        if let Some(b) = self.books.get(&item) {
            for o in &b.bids {
                add(&mut bids, o.price, o.qty, false);
            }
            for o in &b.asks {
                add(&mut asks, o.price, o.qty, false);
            }
        }
        if let Some((p, q)) = self.colony_quote(item, Side::Sell) {
            add(&mut bids, p, q, true);
        }
        if let Some((p, q)) = self.colony_quote(item, Side::Buy) {
            add(&mut asks, p, q, true);
        }
        bids.sort_by_key(|l| std::cmp::Reverse(l.price));
        asks.sort_by_key(|l| l.price);
        bids.truncate(levels);
        asks.truncate(levels);
        Depth { item, bids, asks }
    }

    /// Every item's quote.
    pub fn quotes(&self) -> Vec<Quote> {
        Item::all()
            .into_iter()
            .map(|item| {
                let (bid, ask) = self.top(item);
                let tape = self.tapes.get(&item);
                Quote {
                    item,
                    bid: bid.map(|b| b.0),
                    bid_qty: bid.map_or(0, |b| b.1),
                    ask: ask.map(|a| a.0),
                    ask_qty: ask.map_or(0, |a| a.1),
                    colony: desk(item).is_some(),
                    last: tape.and_then(|t| t.last),
                    volume: tape.map_or(0, |t| t.volume),
                }
            })
            .collect()
    }

    /// `item`'s price a minute at a time, oldest first (at most [`HISTORY`]).
    pub fn history(&self, item: Item) -> Vec<u64> {
        self.tapes.get(&item).map(|t| t.samples.clone()).unwrap_or_default()
    }

    /// What the colony holds of `item` (kg, or pieces).
    pub fn colony_stock(&self, item: Item) -> Option<f64> {
        self.colony.get(&item).map(|s| s.held)
    }

    /// Every trader's resting escrow: credits held by buy orders, goods by sell orders (the
    /// ledger's tests count these).
    pub fn escrowed(&self) -> (u64, BTreeMap<Item, u64>) {
        let mut credits = 0;
        let mut goods: BTreeMap<Item, u64> = BTreeMap::new();
        for b in self.books.values() {
            credits += b.bids.iter().map(|o| o.escrow).sum::<u64>();
            for o in &b.asks {
                *goods.entry(o.item).or_default() += o.qty;
            }
        }
        (credits, goods)
    }

    /// Everything owed, in all accounts.
    pub fn accounts(&self) -> impl Iterator<Item = (&Trader, &Account)> {
        self.accounts.iter()
    }
}

/// A side's best price, and how much is there.
pub type Top = (u64, u64);

/// One price level of a book.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Level {
    pub price: u64,
    pub qty: u64,
    /// The colony quotes (some of) it.
    pub colony: bool,
}

/// A book, a few levels deep.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Depth {
    pub item: Item,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}

/// An item's market at a glance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    pub item: Item,
    pub bid: Option<u64>,
    pub bid_qty: u64,
    pub ask: Option<u64>,
    pub ask_qty: u64,
    /// The colony deals in it.
    pub colony: bool,
    pub last: Option<u64>,
    pub volume: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::value;
    use crate::item::{Material, Ore};
    use bc_proto::{FrameId, Part};

    const TI: Item = Item::Ore(Ore::Titanium);
    const GUNDANIUM: Item = Item::Material(Material::Gundanium);

    #[test]
    fn selling_to_the_colony_moves_its_price_and_time_brings_it_back() {
        let mut ex = Exchange::new();
        let (bid, _) = ex.top(TI);
        let (p0, step) = bid.unwrap();
        assert!(p0 < value(TI) && p0 > value(TI) * 8 / 10, "{p0}");
        // Dump 20 t of titanium ore: each step sells lower than the last.
        let placed = ex.place("alice", TI, Side::Sell, 1, 20_000, 0, false).unwrap();
        assert!(placed.fills.len() >= 20_000 / step as usize);
        assert!(placed.fills.windows(2).all(|w| w[1].price <= w[0].price));
        assert!(placed.fills.iter().all(|f| f.buyer.is_none()));
        let p1 = ex.top(TI).0.unwrap().0;
        assert!(p1 < p0 * 9 / 10, "{p1} vs {p0}");
        let got = ex.collect("alice");
        let value: u64 = placed.fills.iter().map(|f| worth(TI, f.price, f.qty)).sum();
        assert_eq!(got.credits + ex.fees, value);
        assert_eq!(ex.fees, placed.fills.iter().map(|f| f.fee).sum::<u64>());
        // A day later the colony has used it up: the price is back.
        for _ in 0..24 * 6 {
            ex.tick(600.0);
        }
        let p2 = ex.top(TI).0.unwrap().0;
        assert!(p2 + 5 >= p0, "{p2} vs {p0}");
        assert!(!ex.history(TI).is_empty());
    }

    #[test]
    fn pilots_trade_with_each_other_at_the_resting_price() {
        let mut ex = Exchange::new();
        // Nobody else deals in gundanium.
        assert_eq!(ex.top(GUNDANIUM), (None, None));
        let r = ex.place("bob", GUNDANIUM, Side::Sell, 40_000, 300, 0, true).unwrap();
        assert!(r.fills.is_empty() && r.resting.is_some());
        let r2 = ex.place("carol", GUNDANIUM, Side::Sell, 38_000, 100, 0, true).unwrap();
        assert!(r2.resting.is_some());
        assert_eq!(ex.top(GUNDANIUM).1, Some((38_000, 100)));
        // Alice buys 250 kg up to 41,000 cr/t: 100 from carol at 38,000, 150 from bob at 40,000.
        let escrow = Exchange::escrow_for(GUNDANIUM, 41_000, 250);
        let r3 = ex.place("alice", GUNDANIUM, Side::Buy, 41_000, 250, escrow, true).unwrap();
        assert_eq!(r3.fills.len(), 2);
        assert_eq!((r3.fills[0].price, r3.fills[0].qty), (38_000, 100));
        assert_eq!((r3.fills[1].price, r3.fills[1].qty), (40_000, 150));
        assert!(r3.resting.is_none());
        let alice = ex.collect("alice");
        assert_eq!(alice.goods[&GUNDANIUM], 250);
        assert_eq!(alice.credits, escrow - 3_800 - 6_000, "the price improvement comes back");
        assert_eq!(ex.collect("carol").credits, 3_800 - 76);
        assert_eq!(ex.collect("bob").credits, 6_000 - 120);
        // Bob's 150 kg still rest; cancelling hands them back.
        assert_eq!(ex.orders("bob").count(), 1);
        assert!(ex.cancel("bob", r.resting.unwrap()));
        assert_eq!(ex.collect("bob").goods[&GUNDANIUM], 150);
        assert!(!ex.cancel("bob", r.resting.unwrap()));
    }

    #[test]
    fn pilots_beat_the_colony_at_a_better_price_and_share_the_top() {
        let mut ex = Exchange::new();
        let rifle = Item::Weapon(bc_proto::WeaponKind::BeamRifle);
        let (_, ask) = ex.top(rifle);
        let (colony_ask, _) = ask.unwrap();
        ex.place("dave", rifle, Side::Sell, colony_ask - 100, 1, 0, true).unwrap();
        let price = colony_ask + 1_000;
        let r = ex
            .place("erin", rifle, Side::Buy, price, 2, Exchange::escrow_for(rifle, price, 2), false)
            .unwrap();
        assert_eq!(r.fills[0].seller.as_deref(), Some("dave"));
        assert_eq!(r.fills[0].price, colony_ask - 100);
        assert_eq!(r.fills[1].seller, None, "then the colony");
        assert!(r.fills[1].price >= colony_ask);
        let torso = Item::Part(FrameId::Leo, Part::Torso);
        assert!(ex.depth(torso, 5).asks.iter().any(|l| l.colony));
    }

    #[test]
    fn refusals() {
        let mut ex = Exchange::new();
        assert_eq!(ex.place("a", TI, Side::Sell, 0, 10, 0, true), Err(Refused::Bounds));
        assert_eq!(ex.place("a", TI, Side::Buy, 5_000, 1_000, 4_999, true), Err(Refused::Escrow));
        for k in 0..MAX_ORDERS as u64 {
            ex.place("a", GUNDANIUM, Side::Sell, 90_000 + k, 10, 0, true).unwrap();
        }
        assert_eq!(ex.place("a", GUNDANIUM, Side::Sell, 1, 10, 0, true), Err(Refused::TooManyOrders));
        ex.cancel_all("a");
        assert_eq!(ex.orders("a").count(), 0);
        assert_eq!(ex.collect("a").goods[&GUNDANIUM], 10 * MAX_ORDERS as u64);
    }

    #[test]
    fn it_round_trips_as_json() {
        let mut ex = Exchange::new();
        ex.place("bob", GUNDANIUM, Side::Sell, 40_000, 300, 0, true).unwrap();
        ex.place("amy", TI, Side::Sell, 1, 5_000, 0, false).unwrap();
        ex.tick(3_600.0);
        let json = serde_json::to_string(&ex).unwrap();
        let back: Exchange = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ex);
    }
}
