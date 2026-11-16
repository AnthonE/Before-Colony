//! The town's roads as a graph a car is routed on (`docs/FREE_ROAM.md`, 4: the Watch's dispatch
//! drives its units along it), a closed form of the town's plan like the rest of it: nothing stored,
//! nothing allocated, the same route to the bit on the server and in the browser.
//!
//! **The graph.** [`LINES`] roads run the whole way round the ring, fore to aft: the fore rim road,
//! the six streets between the fore columns of blocks, the boulevard's two carriageways, the three
//! streets between the middle columns, the street between the two columns of depots, and the aft rim
//! road. A cross street (an avenue at a district's edge) runs across the ring at every cell's edge,
//! bridging the canal, from rim road to rim road. Where they meet is a junction. What a car can't
//! cross is left out:
//!
//! - each of the boulevard's carriageways runs one way, as its traffic does: the way `t` grows aft of
//!   the median, back fore of it;
//! - the median is crossed only where a cross street meets the boulevard ([`crossing`]);
//! - a lift's square has its avenue: there the avenue stops at the streets either side of the square,
//!   and the boulevard runs on under the lift's lintel with no junction.
//!
//! **A route** ([`route`]) runs from the road nearest one point of the floor to the road nearest
//! another, the shorter way round the ring, as the waypoints where it turns: on the lanes a car keeps
//! to (right of a two-way road's middle, and the boulevard's kerb lane, which its traffic leaves
//! clear), each leg straight along a road or across the ring. A turn costs [`TURN`] m more than going
//! straight on, so of two routes the same length the one with fewer turns is taken. Points are the
//! town's own: `x` across, `t` round from Hub Gate.

use super::street::{RIM_LANE, STREET_LANE};
use super::town::{
    self, AVENUE, BOULEVARD_HALF, CELL, CELLS, CELLS_PER_DISTRICT, COLUMNS, Ground, MEDIAN_HALF, RIM_ROAD,
    RIM_WALK, ROUND, STREET, TownBox,
};
use crate::math::{floor, hypot};
use crate::world::COLONY_HALF_WIDTH;

/// The roads round the ring.
pub const LINES: usize = 14;
/// The boulevard's carriageways among them: fore (driven back round, `t` falling) and aft.
pub const BOULEVARD_LINES: (usize, usize) = (7, 8);
/// What a turn costs, m of straight road: a car slows for a corner.
pub const TURN: f32 = 20.0;
/// How far off a cross street's middle a car keeps, m (right of it).
const CROSS_LANE: f32 = STREET_LANE;
/// The boulevard's kerb lane, off the median's middle, m: its traffic keeps out of it.
pub const KERB_LANE: f32 = BOULEVARD_HALF - 3.0;
/// Enough waypoints for any route [`route`] gives.
pub const MOST: usize = 32;

/// A point a route turns at: `x` across the ring, `t` round it from Hub Gate (in the turn of the ring
/// the route set out in, so it may run below 0 or past [`ROUND`] where it goes on past Hub Gate).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Waypoint {
    pub x: f32,
    pub t: f32,
}

/// The middles of the roads round the ring, across it, fore to aft.
pub fn lines() -> [f32; LINES] {
    let rim = COLONY_HALF_WIDTH - RIM_WALK - 0.5 * RIM_ROAD;
    let carriageway = 0.5 * (BOULEVARD_HALF + MEDIAN_HALF);
    let mut xs = [rim; LINES];
    xs[0] = -rim;
    let mut n = 1;
    for c in 0..COLUMNS {
        let (_, x1, _, hi) = town::column(c);
        if hi == STREET * 0.5 && n < LINES - 1 {
            xs[n] = x1;
            n += 1;
        }
        if x1 == -BOULEVARD_HALF && n + 1 < LINES - 1 {
            xs[n] = -carriageway;
            xs[n + 1] = carriageway;
            n += 2;
        }
    }
    xs
}

/// Whether the cross street at cell edge `k` is a lift's square's avenue.
fn at_a_lift(k: i32) -> bool {
    k.rem_euclid(i32::from(CELLS)) % i32::from(2 * CELLS_PER_DISTRICT) == 0
}

/// Whether road `i` may be driven the way `dir` says (1 the way `t` grows, −1 back).
fn may_drive(i: usize, dir: i32) -> bool {
    match i {
        _ if i == BOULEVARD_LINES.0 => dir < 0,
        _ if i == BOULEVARD_LINES.1 => dir > 0,
        _ => true,
    }
}

/// Whether the cross street at edge `k` runs on from road `i` to road `i + 1`: everywhere but over a
/// lift's square and its foot.
fn runs_on(i: usize, k: i32) -> bool {
    !(at_a_lift(k) && (BOULEVARD_LINES.0 - 1..=BOULEVARD_LINES.1).contains(&i))
}

/// Where a car keeps on road `i` going the way `dir` says, m across: right of a two-way road's middle;
/// on the boulevard, its kerb lane.
fn lane_x(xs: &[f32; LINES], i: usize, dir: i32) -> f32 {
    let d = dir as f32;
    match i {
        _ if i == BOULEVARD_LINES.0 => -KERB_LANE,
        _ if i == BOULEVARD_LINES.1 => KERB_LANE,
        0 => xs[0] + d * RIM_LANE,
        _ if i == LINES - 1 => xs[i] + d * RIM_LANE,
        _ => xs[i] + d * STREET_LANE,
    }
}

/// Where a car keeps on the cross street at edge `k` going `dir` across (1 aft, −1 fore), m round:
/// right of its middle (facing aft, right is back round the ring).
fn lane_t(k: i32, dir: i32) -> f32 {
    CELL * k as f32 - dir as f32 * CROSS_LANE
}

/// Whether the boulevard's median is crossed at `t` round the ring: where a cross street meets the
/// boulevard, but for a lift's (its square's avenue).
pub fn crossing(t: f32) -> bool {
    let t = town::wrap(t);
    let k = floor(t / CELL + 0.5) as i32;
    let half = if k % i32::from(CELLS_PER_DISTRICT) == 0 { AVENUE * 0.5 } else { STREET * 0.5 };
    !at_a_lift(k) && (t - CELL * k as f32).abs() < half
}

/// Whether a car can be at `(x, t)` on the town's floor: on a road's carriageway, or over the median
/// at a crossing; not on a lift's square's avenue; clear of everything standing on the floor (a
/// lift's piers, the railings along the canal's bridges).
pub fn drivable(x: f32, t: f32) -> bool {
    let ok = match town::ground_at(x, t) {
        Ground::Road => true,
        Ground::Median => crossing(t),
        _ => false,
    };
    if !ok {
        return false;
    }
    // A lift's square's avenue, between the streets either side of it.
    let xs = lines();
    let w = town::wrap(t);
    let k = floor(w / CELL + 0.5) as i32;
    let square = (xs[BOULEVARD_LINES.0 - 1] + STREET * 0.5..-BOULEVARD_HALF).contains(&x)
        || (BOULEVARD_HALF..xs[BOULEVARD_LINES.1 + 1] - STREET * 0.5).contains(&x);
    if at_a_lift(k) && (w - CELL * k as f32).abs() < AVENUE * 0.5 && square {
        return false;
    }
    let q = TownBox::new(x - 0.3, x + 0.3, t - 0.3, t + 0.3, 0.1, 1.8);
    let mut clear = true;
    town::each_box(q.x0, q.x1, q.t0, q.t1, &mut |b| clear &= !b.overlaps(&q));
    clear
}

/// Where a point is on the graph.
#[derive(Clone, Copy, Debug, PartialEq)]
enum At {
    /// On road `i`, `t` round (in the asker's turn of the ring).
    Line(usize, f32),
    /// On the cross street at edge `k` (in the asker's turn), `x` across, between roads `j` and
    /// `j + 1`.
    Cross(i32, f32, usize),
}

impl At {
    /// Its point, `(x, t)`, on the roads' middles.
    fn point(self, xs: &[f32; LINES]) -> (f32, f32) {
        match self {
            At::Line(i, t) => (xs[i], t),
            At::Cross(k, x, _) => (x, CELL * k as f32),
        }
    }
}

/// The road nearest `(x, t)`: a road round the ring, or a cross street where it runs (not over the
/// boulevard, whose carriageways are nearer, nor over a lift's square).
fn snap(xs: &[f32; LINES], x: f32, t: f32) -> At {
    let mut best = (f32::MAX, At::Line(0, t));
    for (i, &lx) in xs.iter().enumerate() {
        let d = (x - lx).abs();
        if d < best.0 {
            best = (d, At::Line(i, t));
        }
    }
    let k0 = floor(t / CELL) as i32;
    for k in [k0, k0 + 1] {
        // The cross street's two runs: fore of the boulevard (or the square) and aft of it.
        let (fore, aft) = if at_a_lift(k) {
            (xs[BOULEVARD_LINES.0 - 1], xs[BOULEVARD_LINES.1 + 1])
        } else {
            (-BOULEVARD_HALF, BOULEVARD_HALF)
        };
        let cx = if x <= 0.5 * (fore + aft) { x.clamp(xs[0], fore) } else { x.clamp(aft, xs[LINES - 1]) };
        let d = hypot(x - cx, t - CELL * k as f32);
        if d < best.0 {
            let j = (0..LINES - 1).rfind(|&j| xs[j] <= cx).unwrap_or(0);
            best = (d, At::Cross(k, cx, j));
        }
    }
    best.1
}

/// States of the search: a junction (road `i` at edge `k`, `k` round the ring from Hub Gate) and how
/// it was come into (along the road, or across the ring).
const STATES: usize = 2 * LINES * CELLS as usize;
const NONE: u16 = u16::MAX;
const DONE: u16 = u16::MAX - 1;
const FROM_START: u16 = u16::MAX - 2;
const ALONG: usize = 0;
const ACROSS: usize = 1;

fn state(i: usize, k: i32, axis: usize) -> usize {
    (k.rem_euclid(i32::from(CELLS)) as usize * LINES + i) * 2 + axis
}

/// A state's road, edge (round the ring, 0 to [`CELLS`]) and axis.
fn unstate(s: usize) -> (usize, i32, usize) {
    let n = s / 2;
    (n % LINES, (n / LINES) as i32, s % 2)
}

/// The search's books, sized for every state: what each costs, where it came from, and the heap.
struct Books {
    g: [f32; STATES],
    prev: [u16; STATES],
    /// Where in the heap a state is ([`NONE`]: never put in; [`DONE`]: settled).
    at: [u16; STATES],
    heap: [u16; STATES],
    len: usize,
}

impl Books {
    fn new() -> Self {
        Self {
            g: [f32::INFINITY; STATES],
            prev: [NONE; STATES],
            at: [NONE; STATES],
            heap: [0; STATES],
            len: 0,
        }
    }

    fn key(&self, s: usize, h: &impl Fn(usize) -> f32) -> (f32, usize) {
        (self.g[s] + h(s), s)
    }

    fn less(&self, a: usize, b: usize, h: &impl Fn(usize) -> f32) -> bool {
        let (ka, kb) = (self.key(a, h), self.key(b, h));
        ka.0 < kb.0 || (ka.0 == kb.0 && ka.1 < kb.1)
    }

    fn place(&mut self, k: usize, s: usize) {
        self.heap[k] = s as u16;
        self.at[s] = k as u16;
    }

    fn up(&mut self, mut k: usize, h: &impl Fn(usize) -> f32) {
        let s = usize::from(self.heap[k]);
        while k > 0 {
            let p = (k - 1) / 2;
            let ps = usize::from(self.heap[p]);
            if !self.less(s, ps, h) {
                break;
            }
            self.place(k, ps);
            k = p;
        }
        self.place(k, s);
    }

    fn down(&mut self, mut k: usize, h: &impl Fn(usize) -> f32) {
        let s = usize::from(self.heap[k]);
        loop {
            let (l, r) = (2 * k + 1, 2 * k + 2);
            let mut m = s;
            let mut mk = k;
            if l < self.len && self.less(usize::from(self.heap[l]), m, h) {
                m = usize::from(self.heap[l]);
                mk = l;
            }
            if r < self.len && self.less(usize::from(self.heap[r]), m, h) {
                m = usize::from(self.heap[r]);
                mk = r;
            }
            if mk == k {
                break;
            }
            self.place(k, m);
            k = mk;
        }
        self.place(k, s);
    }

    /// `s` reached for `g` from `prev`, if that's cheaper than it was.
    fn relax(&mut self, s: usize, g: f32, prev: u16, h: &impl Fn(usize) -> f32) {
        if self.at[s] == DONE || g >= self.g[s] {
            return;
        }
        self.g[s] = g;
        self.prev[s] = prev;
        if self.at[s] == NONE {
            self.len += 1;
            self.place(self.len - 1, s);
            self.up(self.len - 1, h);
        } else {
            self.up(usize::from(self.at[s]), h);
        }
    }

    fn pop(&mut self, h: &impl Fn(usize) -> f32) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        let s = usize::from(self.heap[0]);
        self.len -= 1;
        if self.len > 0 {
            let last = usize::from(self.heap[self.len]);
            self.place(0, last);
            self.down(0, h);
        }
        self.at[s] = DONE;
        Some(s)
    }
}

/// A move between two places on the graph: along a road (`axis` [`ALONG`]) or across the ring, the
/// way `dir` says, and how far.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Move {
    axis: usize,
    dir: i32,
    len: f32,
}

/// The ways out of `a` onto the graph: the states it reaches first, what that costs, and the move.
fn starts(xs: &[f32; LINES], a: At) -> [Option<(usize, i32, f32, Move)>; 2] {
    match a {
        At::Line(i, t) => {
            let kb = floor(t / CELL) as i32;
            let off = t - CELL * kb as f32;
            let ahead = may_drive(i, 1).then_some((
                state(i, kb + 1, ALONG),
                kb + 1,
                CELL - off,
                Move { axis: ALONG, dir: 1, len: CELL - off },
            ));
            let back = may_drive(i, -1)
                .then_some((state(i, kb, ALONG), kb, off, Move { axis: ALONG, dir: -1, len: off }));
            [ahead, back]
        }
        At::Cross(k, x, j) => [
            Some((state(j, k, ACROSS), k, x - xs[j], Move { axis: ACROSS, dir: -1, len: x - xs[j] })),
            Some((
                state(j + 1, k, ACROSS),
                k,
                xs[j + 1] - x,
                Move { axis: ACROSS, dir: 1, len: xs[j + 1] - x },
            )),
        ],
    }
}

/// From state `s` onto `b`, if `s` is a junction at either end of the run `b` is on: what it costs
/// (a turn too, if it's come in the other way) and the move.
fn arrive(xs: &[f32; LINES], s: usize, b: At) -> Option<(f32, Move)> {
    let (i, k, axis) = unstate(s);
    let turn = |want: usize| if axis == want { 0.0 } else { TURN };
    match b {
        At::Line(bi, t) => {
            if i != bi {
                return None;
            }
            let w = town::wrap(t);
            let kb = (floor(w / CELL) as i32).clamp(0, i32::from(CELLS) - 1);
            let off = w - CELL * kb as f32;
            if k == kb && may_drive(i, 1) {
                Some((off + turn(ALONG), Move { axis: ALONG, dir: 1, len: off }))
            } else if k == (kb + 1) % i32::from(CELLS) && may_drive(i, -1) {
                Some((CELL - off + turn(ALONG), Move { axis: ALONG, dir: -1, len: CELL - off }))
            } else {
                None
            }
        }
        At::Cross(bk, x, j) => {
            if k != bk.rem_euclid(i32::from(CELLS)) {
                return None;
            }
            if i == j {
                Some((x - xs[j] + turn(ACROSS), Move { axis: ACROSS, dir: 1, len: x - xs[j] }))
            } else if i == j + 1 {
                Some((xs[j + 1] - x + turn(ACROSS), Move { axis: ACROSS, dir: -1, len: xs[j + 1] - x }))
            } else {
                None
            }
        }
    }
}

/// Straight from `a` to `b` with no junction turned at: along one road, or across at one edge.
fn direct(a: At, b: At) -> Option<(f32, Move)> {
    match (a, b) {
        (At::Line(i, ta), At::Line(j, tb)) if i == j => {
            let d = tb - ta - floor((tb - ta) / ROUND + 0.5) * ROUND;
            let dir = if d < 0.0 { -1 } else { 1 };
            may_drive(i, dir).then_some((d.abs(), Move { axis: ALONG, dir, len: d.abs() }))
        }
        (At::Cross(ka, xa, ja), At::Cross(kb, xb, jb))
            if ka.rem_euclid(i32::from(CELLS)) == kb.rem_euclid(i32::from(CELLS)) =>
        {
            let open = (ja.min(jb)..=ja.max(jb)).all(|j| runs_on(j, ka));
            let dir = if xb < xa { -1 } else { 1 };
            open.then_some(((xb - xa).abs(), Move { axis: ACROSS, dir, len: (xb - xa).abs() }))
        }
        _ => None,
    }
}

/// How far round the ring `a` is from `b`, the short way, m.
fn round_apart(a: f32, b: f32) -> f32 {
    let d = town::wrap(a - b);
    d.min(ROUND - d)
}

/// The most junctions a route passes.
const PATH: usize = 512;

/// A route by the town's roads from the road nearest `from` to the road nearest `to` (each `(x, t)`:
/// across, and round from Hub Gate, any turn of the ring), the shorter way round: the waypoints it
/// turns at, first to last, written into `out` (as many as fit: [`MOST`] always do). How many there
/// are; 0 if `from` or `to` isn't a point.
pub fn route(from: (f32, f32), to: (f32, f32), out: &mut [Waypoint]) -> usize {
    if !(from.0.is_finite() && from.1.is_finite() && to.0.is_finite() && to.1.is_finite()) {
        return 0;
    }
    let xs = lines();
    let a = snap(&xs, from.0, from.1);
    let b = snap(&xs, to.0, to.1);
    let (bx, bt) = b.point(&xs);
    let h = |s: usize| {
        let (i, k, _) = unstate(s);
        (xs[i] - bx).abs() + round_apart(CELL * k as f32, bt)
    };
    let mut books = Books::new();
    // The best way found: its cost, and the state it's left from onto `b` (none: straight there).
    let mut best: (f32, Option<(usize, Move)>, Option<Move>) = match direct(a, b) {
        Some((c, m)) => (c, None, Some(m)),
        None => (f32::INFINITY, None, None),
    };
    let ways_out = starts(&xs, a);
    for (s, _, c, _) in ways_out.iter().flatten() {
        books.relax(*s, *c, FROM_START, &h);
    }
    while let Some(u) = books.pop(&h) {
        let g = books.g[u];
        if g + h(u) >= best.0 {
            break;
        }
        if let Some((c, m)) = arrive(&xs, u, b)
            && g + c < best.0
        {
            best = (g + c, Some((u, m)), None);
        }
        let (i, k, axis) = unstate(u);
        let turn = |want: usize| if axis == want { 0.0 } else { TURN };
        for dir in [1, -1] {
            if may_drive(i, dir) {
                books.relax(state(i, k + dir, ALONG), g + CELL + turn(ALONG), u as u16, &h);
            }
        }
        if i + 1 < LINES && runs_on(i, k) {
            books.relax(state(i + 1, k, ACROSS), g + xs[i + 1] - xs[i] + turn(ACROSS), u as u16, &h);
        }
        if i > 0 && runs_on(i - 1, k) {
            books.relax(state(i - 1, k, ACROSS), g + xs[i] - xs[i - 1] + turn(ACROSS), u as u16, &h);
        }
    }
    if !best.0.is_finite() {
        return 0;
    }
    let mut legs = Legs::new(&xs, a, out);
    let straight = best.2.is_some();
    match best {
        (_, None, Some(m)) => {
            // Straight there: one leg, on `a`'s road or cross street.
            let (i, k) = match a {
                At::Line(i, t) => (i, floor(t / CELL) as i32),
                At::Cross(k, x, j) => (if x - xs[j] <= xs[j + 1] - x { j } else { j + 1 }, k),
            };
            legs.go(i, k, m);
        }
        (_, Some((last, end)), _) => {
            // The junctions passed, first to last.
            let mut path = [0u16; PATH];
            let mut n = 0;
            let mut s = last as u16;
            loop {
                if n < PATH {
                    path[n] = s;
                    n += 1;
                }
                let p = books.prev[usize::from(s)];
                if p == FROM_START || p == NONE {
                    break;
                }
                s = p;
            }
            path[..n].reverse();
            // Onto the graph: the move into the first junction, from `a`.
            let first = usize::from(path[0]);
            let Some((_, k0, _, m0)) = ways_out.iter().flatten().find(|w| w.0 == first) else { return 0 };
            let (i0, _, _) = unstate(first);
            let (mut i, mut k) = (i0, *k0);
            legs.go(i, k, *m0);
            for w in path[1..n].iter().map(|&s| unstate(usize::from(s))) {
                let m = if w.0 != i {
                    let dir = if w.0 > i { 1 } else { -1 };
                    Move { axis: ACROSS, dir, len: (xs[w.0] - xs[i]).abs() }
                } else {
                    let dir = if w.1 == (k + 1).rem_euclid(i32::from(CELLS)) { 1 } else { -1 };
                    Move { axis: ALONG, dir, len: CELL }
                };
                (i, k) = if m.axis == ALONG { (i, k + m.dir) } else { (w.0, k) };
                legs.go(i, k, m);
            }
            legs.go(i, k, end);
        }
        _ => return 0,
    }
    legs.finish(b.point(&xs), straight)
}

/// The legs of a route as it's walked out move by move, and the waypoints they turn at.
struct Legs<'a> {
    xs: &'a [f32; LINES],
    out: &'a mut [Waypoint],
    n: usize,
    start: (f32, f32),
    /// The leg under way: its axis and way, its road (along) or edge (across), where it's got to:
    /// the junction it's at or passing.
    leg: Option<(usize, i32, usize, i32)>,
}

impl<'a> Legs<'a> {
    fn new(xs: &'a [f32; LINES], a: At, out: &'a mut [Waypoint]) -> Self {
        Self { xs, out, n: 0, start: a.point(xs), leg: None }
    }

    fn put(&mut self, x: f32, t: f32) {
        if self.n > 0
            && let Some(last) = self.out.get(self.n - 1)
            && (last.x - x).abs() < 1e-3
            && (last.t - t).abs() < 1e-3
        {
            return;
        }
        if let Some(w) = self.out.get_mut(self.n) {
            *w = Waypoint { x, t };
        }
        self.n += 1;
    }

    /// The move `m` on road `i` (along) or at edge `k` (across); after it, at road `i` and edge `k`
    /// for a move between junctions.
    fn go(&mut self, i: usize, k: i32, m: Move) {
        if m.len <= 1e-4 {
            return;
        }
        let xs = self.xs;
        match self.leg {
            None => {
                // The first: from where the route sets out, on its lane.
                if m.axis == ALONG {
                    self.put(lane_x(xs, i, m.dir), self.start.1);
                } else {
                    self.put(self.start.0, lane_t(k, m.dir));
                }
            }
            Some((axis, dir, li, lk)) if (axis, dir) != (m.axis, m.dir) => match (axis, m.axis) {
                // A turn: from one road's lane onto another's.
                (ALONG, ACROSS) => self.put(lane_x(xs, li, dir), lane_t(k, m.dir)),
                (ACROSS, ALONG) => self.put(lane_x(xs, i, m.dir), lane_t(lk, dir)),
                // Turned right round (where nothing else is open): across to the other lane.
                (ALONG, _) => {
                    let t = CELL * lk as f32;
                    self.put(lane_x(xs, li, dir), t);
                    self.put(lane_x(xs, li, m.dir), t);
                }
                _ => {
                    let x = xs[li];
                    self.put(x, lane_t(lk, dir));
                    self.put(x, lane_t(lk, m.dir));
                }
            },
            Some(_) => {}
        }
        self.leg = Some((m.axis, m.dir, i, k));
    }

    /// The last waypoint, at `b` (its point `(x, t)` on the roads' middles) on the last leg's lane, in
    /// the route's own turn of the ring (`straight`: the route is one leg, no junction turned at); and
    /// how many there are.
    fn finish(mut self, (bx, bt): (f32, f32), straight: bool) -> usize {
        let xs = self.xs;
        match self.leg {
            None => {
                // Nowhere to go: where it is.
                self.put(self.start.0, self.start.1);
            }
            Some((axis, dir, i, k)) => {
                if axis == ALONG {
                    // `b`'s `t` in the leg's turn: near the junction it's at, or the start.
                    let near = if straight { self.start.1 } else { CELL * k as f32 };
                    let t = near + (bt - near) - floor((bt - near) / ROUND + 0.5) * ROUND;
                    self.put(lane_x(xs, i, dir), t);
                } else {
                    self.put(bx, lane_t(k, dir));
                }
            }
        }
        self.n
    }
}

/// How far a car goes along `route`'s waypoints, m.
pub fn length(route: &[Waypoint]) -> f32 {
    route.windows(2).map(|w| hypot(w[1].x - w[0].x, w[1].t - w[0].t)).sum()
}

#[cfg(test)]
#[allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]
mod tests {
    extern crate std;

    use super::*;
    use crate::colony::street;
    use crate::colony::town::{BlockId, block_rect, out_of_the_lift, watch_station};
    use alloc::collections::VecDeque;
    use alloc::vec;
    use alloc::vec::Vec;

    const HALF: f32 = COLONY_HALF_WIDTH;
    /// A route is no longer than the flood's shortest way times this, and this far over.
    const LONGER: (f32, f32) = (1.1, 60.0);

    fn way(from: (f32, f32), to: (f32, f32)) -> Vec<Waypoint> {
        let mut out = [Waypoint::default(); MOST];
        let n = route(from, to, &mut out);
        assert!((2..=MOST).contains(&n), "{from:?} to {to:?}: {n} waypoints");
        out[..n].to_vec()
    }

    /// Which of the boulevard's carriageways `x` is on, if one: 1 aft (driven the way `t` grows), −1
    /// fore.
    fn carriageway(x: f32) -> i32 {
        if (MEDIAN_HALF..BOULEVARD_HALF).contains(&x) {
            1
        } else if (-BOULEVARD_HALF..-MEDIAN_HALF).contains(&x) {
            -1
        } else {
            0
        }
    }

    /// Every waypoint on a road, each leg straight along a road or across the ring, drivable all the
    /// way, and the boulevard driven its own way.
    fn keeps_to_the_roads(w: &[Waypoint]) {
        for p in w {
            assert_eq!(town::ground_at(p.x, p.t), Ground::Road, "{p:?} of {w:?}");
        }
        for leg in w.windows(2) {
            let (a, b) = (leg[0], leg[1]);
            let (dx, dt) = (b.x - a.x, b.t - a.t);
            assert!(dx.abs() < 1e-3 || dt.abs() < 1e-3, "{a:?} to {b:?}: not along or across");
            let n = libm::ceilf(dx.abs() + dt.abs()) as u32;
            for k in 0..=n {
                let u = k as f32 / n.max(1) as f32;
                let (x, t) = (a.x + dx * u, a.t + dt * u);
                assert!(drivable(x, t), "{a:?} to {b:?}: ({x}, {t}) is no road");
            }
            if dt.abs() > 1e-3 {
                let side = carriageway(a.x);
                assert!(side == 0 || (dt > 0.0) == (side > 0), "{a:?} to {b:?}: the wrong way");
            }
        }
    }

    /// The floor every 4 m round the whole ring, flooded from a start the way a car may go: the fewest
    /// steps to each square.
    struct Flood {
        nx: usize,
        nt: usize,
        steps: Vec<u32>,
    }

    const STEP: f32 = 4.0;

    impl Flood {
        fn at(nx: usize, nt: usize, x: f32, t: f32) -> usize {
            let j = (town::wrap(t) / STEP) as usize % nt;
            let i = (((x + HALF) / STEP) as usize).min(nx - 1);
            j * nx + i
        }

        fn centre(nx: usize, k: usize) -> (f32, f32) {
            let (i, j) = (k % nx, k / nx);
            (-HALF + (i as f32 + 0.5) * STEP, (j as f32 + 0.5) * STEP)
        }

        /// Where a car can be, square by square.
        fn grid() -> (usize, usize, Vec<bool>) {
            let (nx, nt) = ((2.0 * HALF / STEP) as usize, (ROUND / STEP) as usize);
            let mut ok = vec![false; nx * nt];
            for (k, o) in ok.iter_mut().enumerate() {
                let (x, t) = Self::centre(nx, k);
                *o = drivable(x, t);
            }
            (nx, nt, ok)
        }

        fn from(grid: &(usize, usize, Vec<bool>), x: f32, t: f32) -> Self {
            let (nx, nt, ok) = (grid.0, grid.1, &grid.2);
            let mut f = Flood { nx, nt, steps: vec![u32::MAX; nx * nt] };
            let start = Self::at(nx, nt, x, t);
            assert!(ok[start], "({x}, {t}) is no road");
            let mut todo = VecDeque::new();
            f.steps[start] = 0;
            todo.push_back(start);
            while let Some(k) = todo.pop_front() {
                let (i, j) = (k % nx, k / nx);
                let side = carriageway(Self::centre(nx, k).0);
                let mut next = [None; 4];
                if i > 0 {
                    next[0] = Some(k - 1);
                }
                if i + 1 < nx {
                    next[1] = Some(k + 1);
                }
                // Round the ring, but on the boulevard only the way its carriageway runs.
                if side >= 0 {
                    next[2] = Some(((j + 1) % nt) * nx + i);
                }
                if side <= 0 {
                    next[3] = Some(((j + nt - 1) % nt) * nx + i);
                }
                for n in next.into_iter().flatten() {
                    if ok[n] && f.steps[n] == u32::MAX {
                        f.steps[n] = f.steps[k] + 1;
                        todo.push_back(n);
                    }
                }
            }
            f
        }

        /// The shortest way from the start to the square `(x, t)` is on (or one beside it), m.
        fn to(&self, x: f32, t: f32) -> f32 {
            let k = Self::at(self.nx, self.nt, x, t);
            let (i, j) = (k % self.nx, k / self.nx);
            let mut best = self.steps[k];
            for (di, dj) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let ni = (i as i32 + di).clamp(0, self.nx as i32 - 1) as usize;
                let nj = (j as i32 + dj).rem_euclid(self.nt as i32) as usize;
                best = best.min(self.steps[nj * self.nx + ni].saturating_add(1));
            }
            assert!(best < u32::MAX, "({x}, {t}) isn't reached");
            best as f32 * STEP
        }
    }

    #[test]
    fn the_roads_round_the_ring_and_across_it_are_roads() {
        let xs = lines();
        assert!(xs.windows(2).all(|w| w[0] < w[1]), "{xs:?}");
        assert_eq!((xs[BOULEVARD_LINES.0], xs[BOULEVARD_LINES.1]), (-20.0, 20.0));
        // Each two-way road's lanes are the traffic's.
        let lanes: Vec<f32> = (0..street::LANES).filter_map(street::lane).map(|l| l.x).collect();
        for (i, &x) in xs.iter().enumerate() {
            for dir in [1, -1] {
                let lx = lane_x(&xs, i, dir);
                if carriageway(x) == 0 {
                    assert!(lanes.iter().any(|l| (l - lx).abs() < 1e-3), "road {i}'s lane at {lx}");
                }
                for cell in [0u16, 5, 30, 77, 143] {
                    let t = CELL * (f32::from(cell) + 0.5);
                    assert!(drivable(lx, t), "road {i} going {dir} at {t}");
                }
            }
        }
        // Every cross street, from rim road to rim road, but over a lift's square and its foot.
        for k in 0..i32::from(CELLS) {
            for dir in [1, -1] {
                let t = lane_t(k, dir);
                let mut x = xs[0];
                while x < xs[LINES - 1] {
                    let square = at_a_lift(k)
                        && x > xs[BOULEVARD_LINES.0 - 1] + 11.0
                        && x < xs[BOULEVARD_LINES.1 + 1] - 11.0;
                    if (x - xs[BOULEVARD_LINES.0 - 1] - 10.0).abs() > 1.0
                        && (x - xs[BOULEVARD_LINES.1 + 1] + 10.0).abs() > 1.0
                    {
                        assert_eq!(drivable(x, t), !square, "the cross street at {k} at {x}");
                    }
                    x += 1.0;
                }
            }
            assert_eq!(crossing(CELL * k as f32), !at_a_lift(k));
        }
        assert!(!crossing(CELL * 7.5) && crossing(CELL * 7.0 + 9.0) && !crossing(CELL * 7.0 + 11.0));
    }

    #[test]
    fn from_hub_gates_square_a_car_keeps_to_the_roads_to_every_watch_station_and_every_district() {
        let grid = Flood::grid();
        let (mut routes, mut worst) = (0, 0.0f32);
        let depot = block_rect(BlockId::new(80, 12));
        for from in [out_of_the_lift(0), (0.5 * (depot.x0 + depot.x1), depot.t0 - 2.0)] {
            let start = way(from, from)[0];
            let flood = Flood::from(&grid, start.x, start.t);
            let mut check = |to: (f32, f32)| {
                let w = way(from, to);
                keeps_to_the_roads(&w);
                let (a, b) = (w[0], w[w.len() - 1]);
                assert!((a.x - start.x).abs() < 1e-3 && (a.t - start.t).abs() < 1e-3, "{a:?} vs {start:?}");
                let (long, short) = (length(&w), flood.to(b.x, b.t));
                assert!(
                    long <= short * LONGER.0 + LONGER.1,
                    "{from:?} to {to:?}: {long} m by {w:?}, the flood's {short}"
                );
                // Near where it's going.
                assert!((b.x - to.0).abs() + round_apart(b.t, to.1) < 120.0, "{b:?} for {to:?}");
                worst = worst.max((long - LONGER.1) / short.max(1.0));
                routes += 1;
            };
            for d in 0..town::DISTRICT_COUNT {
                let r = block_rect(watch_station(d));
                check((r.x0 - 1.0, 0.5 * (r.t0 + r.t1)));
            }
            for cell in (0..CELLS).step_by(11) {
                for c in 0..COLUMNS {
                    let r = block_rect(BlockId::new(cell, c));
                    check((0.5 * (r.x0 + r.x1), r.t0 - 2.0));
                }
            }
        }
        assert!(routes > 300, "{routes}");
        std::println!("{routes} routes, the worst {worst} of the flood's");
    }

    #[test]
    fn a_route_goes_the_shorter_way_round_and_is_the_same_from_any_turn() {
        let xs = lines();
        // Along a street, either side of Hub Gate.
        let w = way((xs[3], 100.0), (xs[3], ROUND - 100.0));
        assert!(length(&w) < 210.0 && w.iter().all(|p| (-110.0..110.0).contains(&p.t)), "{w:?}");
        let w = way((xs[3], -100.0), (xs[3], 100.0));
        assert!(length(&w) < 210.0, "{w:?}");
        // The same route asked a turn on, a turn on.
        for (from, to) in [((-300.0, 50.0), (700.0, 9_000.0)), ((15.0, 4_000.0), (-15.0, 3_000.0))] {
            let (a, b) = (way(from, to), way((from.0, from.1 + ROUND), (to.0, to.1 - ROUND)));
            assert_eq!(a.len(), b.len());
            for (p, q) in a.iter().zip(&b) {
                assert!((p.x - q.x).abs() < 1e-3 && (p.t + ROUND - q.t).abs() < 0.05, "{p:?} vs {q:?}");
            }
        }
        // Behind on the boulevard's aft carriageway: on to a crossing, back on the fore one, and
        // across again.
        let w = way((20.0, 1_000.0), (20.0, 700.0));
        keeps_to_the_roads(&w);
        assert!(length(&w) < 300.0 + 2.0 * CELL + 2.0 * 60.0, "{w:?}");
        assert!(w.iter().any(|p| p.x < 0.0), "{w:?}");
        // Somewhere that isn't.
        let mut out = [Waypoint::default(); MOST];
        assert_eq!(route((f32::NAN, 0.0), (0.0, 0.0), &mut out), 0);
        // Too few places for it: as many as fit, and how many it is.
        let mut two = [Waypoint::default(); 2];
        let n = route((-300.0, 50.0), (700.0, 9_000.0), &mut two);
        assert_eq!(&two[..], &way((-300.0, 50.0), (700.0, 9_000.0))[..2]);
        assert!(n > 2);
    }
}
