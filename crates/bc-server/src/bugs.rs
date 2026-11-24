//! Players' reports, kept on our own disk: `POST /bugs`, there only with a data dir
//! (`--data-dir`), each report a file under `<data dir>/reports/`.
//!
//! The page files one when a player presses F7 (`web/ui.js`, the composer) and when it crashes
//! (`web/boot.js`): `{game, report, shot_png_b64?}`, the body the Elo Pros intake takes for every
//! listed game (scry-forge `docs/client/GAMEDEV.md` §3c, `meter/bugs.py`), so the same document
//! can go to either. `report` is checked by that intake's rules: `format` 1, a known `kind`, a
//! one-line `issue_title`, `read_first` (a doc's file name) and `reported`, and `build` with its
//! `version`, `commit`, `platform` and `runtime`. Anything else in it is kept as sent, up to
//! [`REPORT_MAX`] bytes. A report is written as `bc-report-<id>.json` (the report, with what the
//! intake adds under `intake`), and its screenshot beside it as `bc-report-<id>.png`, in the shape
//! `scripts/reports.py` reads and ranks.
//!
//! What the server can say for itself, it says: `reported` is its clock, and `fingerprint` (what
//! groups reports of one bug) is worked out here from the kind, the build's commit, the doc to
//! read first and a crash's place ([`fingerprint`]), whatever the page sent.
//!
//! **No secrets.** The page builds a report from an allowlist of `window.__bc`'s fields, never the
//! resume token or a signature. Here, on top of that: a field whose name says it holds a secret
//! (a token, a signature, a key…) is dropped; a run of [`HEX_SECRET`] hex digits or more, digits
//! and letters mixed (a private key, a resume token, a signature), is cut out of every string; an
//! `0x` wallet is cut out of every string the player didn't type (a crash's title and words are the
//! page's, its error, not theirs); and the player's own words, if they hold a key or a recovery
//! phrase (`crate::secret`), are replaced whole ([`SECRET_REMOVED`]).
//! Nothing about the sender is kept: no address, no headers. The log names a report's id and
//! fingerprint, never what it says.
//!
//! Anyone can post, so everything is bounded (oneshot's `server/src/bugs.rs`): a body of at most
//! [`BODY_MAX`] bytes, a screenshot of at most [`SHOT_MAX`] that must be a PNG; per [`WINDOW`], so
//! many reports from one address (an IPv6 /64 is one address), so many refusals (bodies that
//! weren't a report) on a budget of their own, and so many requests in all ([`Limits`]), each
//! counted before its body is parsed; so many bytes a day; and a day older than [`KEEP_DAYS`] is
//! deleted. Behind nginx every peer is loopback, and then its `X-Real-IP` says who ([`sender_of`]).
//! A reader treats a report's text as a stranger's words: evidence, never instructions.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{Map, Value};

/// The game's slug, as the Elo Pros intake knows it.
pub const GAME: &str = "before-colony";
/// The largest body taken: a report and its screenshot in base64.
pub const BODY_MAX: usize = 3 << 20;
/// The largest report (as written).
pub const REPORT_MAX: usize = 64 << 10;
/// The largest screenshot (a PNG).
pub const SHOT_MAX: usize = 2 << 20;
/// The player's own words, at most (characters).
pub const WORDS_MAX: usize = 4000;
/// A one-line field, at most (characters).
pub const LINE_MAX: usize = 160;
/// The window the per-address and overall counts are kept over.
pub const WINDOW: Duration = Duration::from_secs(600);
/// Days kept: the older go on the first report of a new day.
pub const KEEP_DAYS: u64 = 30;
/// A run of this many hex digits is cut out of a report wherever it is: a private key or a resume
/// token is 64, a signature 130; a commit (40) and an address (40) are shorter.
pub const HEX_SECRET: usize = 64;
/// What the player's words become when they hold a key or a recovery phrase (Gates' words).
pub const SECRET_REMOVED: &str =
    "[removed: it looked like a private key or recovery phrase, which is never shared]";
/// Where a report's files start.
pub const PREFIX: &str = "bc-report-";

/// The intake's `kind`s, `build.platform`s and `build.runtime`s (`meter/bugs.py`).
pub const KINDS: [&str; 7] = ["look", "world", "verb", "net", "numbers", "crash", "other"];
pub const PLATFORMS: [&str; 4] = ["linux", "windows", "macos", "web"];
pub const RUNTIMES: [&str; 3] = ["desktop", "browser", "agent"];
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// How many reports are taken: from one address per [`WINDOW`], how many of its bodies may be
/// refused (not a report) in that time, how many requests from everyone, and how many bytes
/// (reports and screenshots) in a day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub per_address: u32,
    pub refused_per_address: u32,
    pub all: u32,
    pub day_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        // A playtester files a handful an hour; a page that sent a picture too big, then the
        // report without it, is no flood, but a stream of junk is; well over the addresses a flood
        // has to hand, so one doesn't starve the rest; and a day's disk at about 30 shots of 2 MiB.
        Limits { per_address: 5, refused_per_address: 20, all: 600, day_bytes: 64 << 20 }
    }
}

/// Why a report wasn't kept.
#[derive(Debug, PartialEq, Eq)]
pub enum No {
    /// Not a report this takes, and why (400).
    Bad(String),
    /// Too many from this address, or from everyone, just now (429).
    Busy,
    /// Today's room is used up, or the disk wouldn't take it (503).
    Full,
}

/// What a kept report's sender is told: its id, its fingerprint, how many reports of it there are
/// (this one too) and whether its screenshot was kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub id: String,
    pub fingerprint: String,
    pub seen: u64,
    pub shot: bool,
}

impl Receipt {
    pub fn json(&self) -> Value {
        serde_json::json!({
            "id": self.id,
            "fingerprint": self.fingerprint,
            "seen": self.seen,
            "shot": self.shot,
        })
    }
}

#[derive(Default)]
struct Window {
    since: Option<Instant>,
    all: u32,
    by: BTreeMap<IpAddr, Share>,
}

/// One address's count in a window: reports let in, and bodies refused.
#[derive(Clone, Copy, Default)]
struct Share {
    kept: u32,
    refused: u32,
}

/// What's on disk, read once on the first report.
#[derive(Default)]
struct Disk {
    loaded: bool,
    /// The day last pruned, and the bytes written on it.
    day: Option<u64>,
    bytes_today: u64,
    /// Reports kept, by fingerprint.
    seen: BTreeMap<String, u64>,
}

pub struct Intake {
    dir: PathBuf,
    limits: Limits,
    window: Mutex<Window>,
    /// Held while writing: a day's room is checked, and the files written, as one step.
    disk: Mutex<Disk>,
}

/// Who counts as one sender: an IPv4 address (an IPv4 inside IPv6 is that address), or an IPv6 /64.
pub fn sender(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(v4) => IpAddr::V4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(Ipv6Addr::from(u128::from(v6) & !((1u128 << 64) - 1))),
        },
    }
}

/// Who sent a request that came from `peer`: behind nginx every peer is loopback (over IPv4 on a
/// dual-stack listener, `::ffff:127.0.0.1`), and then its `X-Real-IP` says who (oneshot's rule);
/// from anywhere else the header is anyone's to forge, so the peer it is.
pub fn sender_of(peer: IpAddr, real_ip: Option<&str>) -> IpAddr {
    let from = if peer.to_canonical().is_loopback() {
        real_ip.and_then(|v| v.trim().parse().ok()).unwrap_or(peer)
    } else {
        peer
    };
    sender(from)
}

impl Intake {
    /// Reports go to `dir` (`<data dir>/reports`), made on the first one.
    pub fn new(dir: PathBuf, limits: Limits) -> Intake {
        Intake { dir, limits, window: Mutex::default(), disk: Mutex::default() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Whether `from` may send one more now, before its body is parsed: neither past its share of
    /// reports nor of refusals, nor everyone past theirs. If so it's counted, as a report until
    /// [`Intake::refused`] says otherwise. The window restarts whole once it's over (one address
    /// can't hold it open).
    pub fn admit(&self, from: IpAddr, now: Instant) -> bool {
        let from = sender(from);
        let Ok(mut w) = self.window.lock() else {
            return false;
        };
        let w = &mut *w;
        if w.since.is_none_or(|t| now.duration_since(t) >= WINDOW) {
            *w = Window { since: Some(now), ..Window::default() };
        }
        let mine = w.by.get(&from).copied().unwrap_or_default();
        if w.all >= self.limits.all
            || mine.kept >= self.limits.per_address
            || mine.refused >= self.limits.refused_per_address
        {
            return false;
        }
        w.all += 1;
        w.by.insert(from, Share { kept: mine.kept + 1, ..mine });
        true
    }

    /// What [`Intake::admit`] let in from `from` wasn't a report: it goes back to the address's
    /// share and on its refusals (everyone's count keeps it, so a flood of junk is bounded too).
    fn refused(&self, from: IpAddr) {
        let Ok(mut w) = self.window.lock() else { return };
        if let Some(mine) = w.by.get_mut(&sender(from)) {
            mine.kept = mine.kept.saturating_sub(1);
            mine.refused += 1;
        }
    }

    /// Checks `body`, and keeps it as a report from `from` at `unix_s` (the wall clock, s). `from`
    /// is counted first, so what isn't a report is limited too.
    pub fn file(&self, from: IpAddr, body: &[u8], unix_s: u64, now: Instant) -> Result<Receipt, No> {
        if !self.admit(from, now) {
            return Err(No::Busy);
        }
        let (mut report, shot) = parse(body, unix_s).inspect_err(|_| self.refused(from))?;
        let Ok(mut disk) = self.disk.lock() else {
            return Err(No::Full);
        };
        let today = unix_s / 86_400;
        if disk.day != Some(today) {
            prune(&self.dir, today);
            *disk = Disk { day: Some(today), ..Disk::default() };
        }
        if !disk.loaded {
            load(&self.dir, today, &mut disk);
        }
        let id = new_id(unix_s);
        let stem = format!("{PREFIX}{id}");
        let shot_name = shot.as_ref().map(|_| format!("{stem}.png"));
        let fingerprint = report["fingerprint"].as_str().unwrap_or_default().to_string();
        if let Some(r) = report.as_object_mut() {
            r.insert("intake".into(), serde_json::json!({ "id": id, "game": GAME, "shot": shot_name }));
        }
        let json = serde_json::to_vec_pretty(&report).map_err(|_| No::Full)?;
        let bytes = json.len() as u64 + shot.as_ref().map_or(0, |s| s.len() as u64);
        if disk.bytes_today + bytes > self.limits.day_bytes {
            return Err(No::Full);
        }
        std::fs::create_dir_all(&self.dir).map_err(|_| No::Full)?;
        // The picture first: a report never names one that isn't there.
        if let (Some(png), Some(name)) = (&shot, &shot_name) {
            write_atomic(&self.dir, name, png)?;
        }
        write_atomic(&self.dir, &format!("{stem}.json"), &json)?;
        disk.bytes_today += bytes;
        let seen = disk.seen.entry(fingerprint.clone()).or_default();
        *seen += 1;
        Ok(Receipt { id, fingerprint, seen: *seen, shot: shot.is_some() })
    }
}

/// The report in `body` as it will be kept, and its screenshot: the intake's rules, the secrets
/// out, the server's own clock and fingerprint.
pub fn parse(body: &[u8], unix_s: u64) -> Result<(Value, Option<Vec<u8>>), No> {
    let bad = |why: &str| No::Bad(why.to_string());
    if body.len() > BODY_MAX {
        return Err(bad("the body is too big"));
    }
    let mut doc: Value = serde_json::from_slice(body).map_err(|_| bad("not JSON"))?;
    let Some(doc) = doc.as_object_mut() else {
        return Err(bad("not an object: {game, report, shot_png_b64?}"));
    };
    if doc.get("game").and_then(Value::as_str) != Some(GAME) {
        return Err(bad("game must be before-colony"));
    }
    let shot = match doc.get("shot_png_b64") {
        None | Some(Value::Null) => None,
        Some(Value::String(b64)) => {
            if b64.len() > SHOT_MAX.div_ceil(3) * 4 + 4 {
                return Err(bad("the screenshot is too big"));
            }
            let png = base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .map_err(|_| bad("shot_png_b64 isn't base64"))?;
            if png.len() > SHOT_MAX {
                return Err(bad("the screenshot is too big"));
            }
            if !png.starts_with(PNG_MAGIC) {
                return Err(bad("the screenshot must be a PNG"));
            }
            Some(png)
        }
        Some(_) => return Err(bad("shot_png_b64 must be text")),
    };
    let mut report = doc.remove("report").ok_or_else(|| bad("no report"))?;
    if !report.is_object() {
        return Err(bad("report must be an object"));
    }
    clean(&mut report);
    let r = report.as_object_mut().ok_or_else(|| bad("report must be an object"))?;
    // The player's words: one line, and the rest.
    let mut removed = false;
    let mut cut = false;
    for (key, max, one_line) in [("issue_title", LINE_MAX, true), ("words", WORDS_MAX, false)] {
        let Some(v) = r.get_mut(key) else { continue };
        let Some(text) = v.as_str() else {
            return Err(No::Bad(format!("report.{key} must be text")));
        };
        let text = if crate::secret::holds_secret(text) {
            removed = true;
            SECRET_REMOVED.to_string()
        } else {
            text.to_string()
        };
        let (text, was_cut) = if one_line { line(&text, max) } else { capped(&text, max) };
        cut |= was_cut;
        *v = Value::String(text);
    }
    if removed {
        r.insert("secret_removed".into(), Value::Bool(true));
    }
    if cut {
        r.insert("truncated".into(), Value::Bool(true));
    }
    // What the page says of itself, one line each.
    for key in ["read_first", "kind"] {
        if let Some(Value::String(s)) = r.get_mut(key) {
            *s = line(s, LINE_MAX).0;
        }
    }
    r.insert("reported".into(), Value::String(iso(unix_s)));
    check(r)?;
    let fp = fingerprint(r);
    r.insert("fingerprint".into(), Value::String(fp));
    if serde_json::to_vec(&report).map_or(usize::MAX, |b| b.len()) > REPORT_MAX {
        return Err(bad("the report is too big"));
    }
    Ok((report, shot))
}

/// The intake's rules on a report (`meter/bugs.py`'s `report_error`), but the fingerprint, which
/// is ours to say.
fn check(r: &Map<String, Value>) -> Result<(), No> {
    let bad = |why: &str| Err(No::Bad(why.to_string()));
    if r.get("format").and_then(Value::as_u64) != Some(1) {
        return bad("report.format must be 1");
    }
    let kind = r.get("kind").and_then(Value::as_str).unwrap_or_default();
    if !KINDS.contains(&kind) {
        return bad("report.kind is not a known kind");
    }
    for key in ["issue_title", "read_first"] {
        match r.get(key).and_then(Value::as_str) {
            Some(s) if !s.is_empty() && s.chars().count() <= 300 && !s.chars().any(unseen) => {}
            _ => return Err(No::Bad(format!("report.{key} must be one line of plain text"))),
        }
    }
    let doc = r.get("read_first").and_then(Value::as_str).unwrap_or_default();
    if !(1..=64).contains(&doc.len())
        || !doc.bytes().all(|b| b.is_ascii_alphanumeric() || b"_./-".contains(&b))
    {
        return bad("report.read_first must be a doc's file name");
    }
    let Some(b) = r.get("build").and_then(Value::as_object) else {
        return bad("report.build must be an object");
    };
    for key in ["version", "commit"] {
        let v = b.get(key).and_then(Value::as_str).unwrap_or_default();
        if !(1..=40).contains(&v.len())
            || !v.bytes().all(|c| c.is_ascii_alphanumeric() || b".+_-".contains(&c))
        {
            return Err(No::Bad(format!("report.build.{key} is missing or malformed")));
        }
    }
    if !b.get("platform").and_then(Value::as_str).is_some_and(|p| PLATFORMS.contains(&p)) {
        return bad("report.build.platform must be one of linux, windows, macos, web");
    }
    if !b.get("runtime").and_then(Value::as_str).is_some_and(|p| RUNTIMES.contains(&p)) {
        return bad("report.build.runtime must be one of desktop, browser, agent");
    }
    for key in ["agent", "place", "vitals", "crash"] {
        if r.get(key).is_some_and(|v| !v.is_object() && !v.is_null()) {
            return Err(No::Bad(format!("report.{key} must be an object")));
        }
    }
    Ok(())
}

/// What groups reports of one bug, as 16 hex digits: FNV-1a (64-bit) over the kind, the build's
/// commit, the doc to read first and, for a crash, where it happened (`crash.where`), joined by
/// `|`. Blind to the player's words, the clock and the place on the map, so two people who hit
/// the same thing in one build share it. `web/ui.js` (`fingerprint`) works out the same.
pub fn fingerprint(r: &Map<String, Value>) -> String {
    let s = |v: Option<&Value>| v.and_then(Value::as_str).unwrap_or_default().to_string();
    let commit = s(r.get("build").and_then(|b| b.get("commit")));
    let at = s(r.get("crash").and_then(|c| c.get("where")));
    let key = format!("{}|{commit}|{}|{at}", s(r.get("kind")), s(r.get("read_first")));
    format!("{:016x}", fnv1a(key.as_bytes()))
}

pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Characters that change how the text round them shows without being seen (bidi overrides and
/// isolates, zero-width marks: the trojan-source kind), and every control character but a line's
/// end and a tab. A report is read by a person or an agent: what it shows must be what it says.
fn hidden(c: char) -> bool {
    matches!(c,
        '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2069}' | '\u{FEFF}' | '\u{FFF9}'..='\u{FFFB}')
        || (c.is_control() && c != '\n' && c != '\t')
}

/// What the intake refuses in a one-line field.
fn unseen(c: char) -> bool {
    hidden(c) || c.is_control()
}

/// A field whose name says it holds a secret: never written, whatever it holds.
fn secret_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    ["resume", "token", "signature", "secret", "password", "private", "mnemonic", "recovery", "seed"]
        .iter()
        .any(|s| k.contains(s))
}

/// Every string in `v` (keys too) without [`hidden`] characters or a [`HEX_SECRET`]-long hex run,
/// every field named for a secret gone, and an `0x` wallet cut out of whatever the player didn't
/// type (`issue_title` and `words` are theirs, but in a crash's report, where the page fills them
/// with its error).
fn clean(v: &mut Value) {
    let kind = v.get("kind").and_then(Value::as_str).map(|k| line(&scrub(k, true), LINE_MAX).0);
    let typed = kind.is_some_and(|k| k != "crash");
    clean_at(v, typed);
}

/// `typed`: `v` is a report whose `issue_title` and `words` the player typed.
fn clean_at(v: &mut Value, typed: bool) {
    match v {
        Value::String(s) => *s = scrub(s, true),
        Value::Array(a) => a.iter_mut().for_each(|x| clean_at(x, false)),
        Value::Object(o) => {
            let old = std::mem::take(o);
            for (k, mut x) in old {
                if secret_key(&k) {
                    continue;
                }
                let theirs = typed && (k == "issue_title" || k == "words");
                match &mut x {
                    Value::String(s) => *s = scrub(s, !theirs),
                    _ => clean_at(&mut x, false),
                }
                o.insert(scrub(&k, true), x);
            }
        }
        _ => {}
    }
}

/// `s` without [`hidden`] characters and hex runs of [`HEX_SECRET`] digits or more that mix digits
/// and letters (and, with `wallets`, without an `0x` and 40 hex digits).
fn scrub(s: &str, wallets: bool) -> String {
    let chars: Vec<char> = s.chars().filter(|c| !hidden(*c)).collect();
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        let run = chars[i..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
        // A key mixes digits and `a-f` the way any real one does (`crate::secret`): a held key
        // (`aaaa…`) or a long number is no secret.
        let digits = chars[i..i + run].iter().filter(|c| c.is_ascii_digit()).count();
        if run >= HEX_SECRET && digits > 0 && digits < run {
            out.push_str("[removed]");
            i += run;
            continue;
        }
        let wallet = wallets
            && chars[i] == '0'
            && chars.get(i + 1).is_some_and(|c| *c == 'x' || *c == 'X')
            && chars[i + 2..].iter().take_while(|c| c.is_ascii_hexdigit()).count() == 40;
        if wallet {
            out.push_str("[wallet]");
            i += 42;
            continue;
        }
        // Copy the run (or the one character) as it is.
        let n = run.max(1);
        out.extend(&chars[i..i + n]);
        i += n;
    }
    out
}

/// `s` on one line (line ends and tabs become spaces, runs of space one), at most `max`
/// characters; and whether it was cut.
fn line(s: &str, max: usize) -> (String, bool) {
    let flat = s.split(|c: char| c.is_whitespace() || unseen(c)).filter(|w| !w.is_empty());
    let flat = flat.collect::<Vec<_>>().join(" ");
    capped(&flat, max)
}

fn capped(s: &str, max: usize) -> (String, bool) {
    match s.char_indices().nth(max) {
        Some((at, _)) => (s[..at].to_string(), true),
        None => (s.to_string(), false),
    }
}

/// `YYYYmmdd-HHMMSS-<8 hex>`: sortable by time, then told apart (the intake's id).
fn new_id(unix_s: u64) -> String {
    let mut r = [0u8; 4];
    if getrandom::fill(&mut r).is_err() {
        // A clock's nanoseconds will do to tell two apart.
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        r = n.to_le_bytes();
    }
    let (y, m, d) = civil((unix_s / 86_400) as i64);
    let t = unix_s % 86_400;
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}-{}", t / 3600, t / 60 % 60, t % 60, hex::encode(r))
}

/// `YYYY-mm-ddTHH:MM:SSZ`.
pub fn iso(unix_s: u64) -> String {
    let (y, m, d) = civil((unix_s / 86_400) as i64);
    let t = unix_s % 86_400;
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", t / 3600, t / 60 % 60, t % 60)
}

/// `YYYYmmdd` for day `day` since 1970-01-01.
fn day_stamp(day: u64) -> String {
    let (y, m, d) = civil(day as i64);
    format!("{y:04}{m:02}{d:02}")
}

/// The day stamp in a report's file name.
fn stamp_of(name: &str) -> Option<&str> {
    name.strip_prefix(PREFIX)?.get(..8).filter(|d| d.bytes().all(|b| b.is_ascii_digit()))
}

fn write_atomic(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), No> {
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, bytes).map_err(|_| No::Full)?;
    std::fs::rename(&tmp, dir.join(name)).map_err(|_| {
        let _ = std::fs::remove_file(&tmp);
        No::Full
    })
}

/// Deletes the reports older than [`KEEP_DAYS`] before `today`.
fn prune(dir: &Path, today: u64) {
    let Some(first) = today.checked_sub(KEEP_DAYS) else {
        return;
    };
    let keep_from = day_stamp(first);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(name) = name.to_str() else { continue };
        // Day stamps sort as they read.
        if stamp_of(name).is_some_and(|d| d < keep_from.as_str()) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// What's kept already: today's bytes and the reports by fingerprint.
fn load(dir: &Path, today: u64, disk: &mut Disk) {
    disk.loaded = true;
    let today = day_stamp(today);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(day) = stamp_of(name) else { continue };
        if day == today {
            disk.bytes_today += e.metadata().map_or(0, |m| m.len());
        }
        if !name.ends_with(".json") {
            continue;
        }
        let fp = std::fs::read(e.path())
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v.get("fingerprint")?.as_str().map(str::to_string));
        if let Some(fp) = fp {
            *disk.seen.entry(fp).or_default() += 1;
        }
    }
}

/// The date of day `z` since 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A report as the page sends it.
    pub fn report() -> Value {
        serde_json::json!({
            "format": 1,
            "kind": "look",
            "fingerprint": "0000000000000000",
            "issue_title": "[look] the canal is the wrong colour",
            "read_first": "docs/FREE_ROAM.md",
            "reported": "2026-11-23T10:00:00Z",
            "build": {"version": "0.1.0", "commit": "9fc2ff6abc", "platform": "web", "runtime": "browser"},
            "words": "the canal is the wrong colour",
            "place": {"place": "town", "town_feet": "1.0,0.0,-200.0"},
        })
    }

    fn body(r: Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({ "game": GAME, "report": r })).unwrap()
    }

    #[test]
    fn dates_and_ids() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(951_782_400 + 3661), "2000-02-29T01:01:01Z");
        let id = new_id(1_795_427_000);
        assert!(id.starts_with("20261123-"), "{id}");
        assert_eq!(id.len(), "20261123-095000-a1b2c3d4".len(), "{id}");
        assert_eq!(stamp_of("bc-report-20261123-095000-a1b2c3d4.json"), Some("20261123"));
        assert_eq!(stamp_of("notes.txt"), None);
    }

    #[test]
    fn the_fingerprint_is_the_servers_and_the_pages_alike() {
        // `web/ui.js`'s `fingerprint` gives the same for the same key (FNV-1a 64).
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(format!("{:016x}", fnv1a(b"a")), "af63dc4c8601ec8c");
        let (r, _) = parse(&body(report()), 1_795_427_000).unwrap();
        let fp = r["fingerprint"].as_str().unwrap();
        assert_eq!(fp, format!("{:016x}", fnv1a(b"look|9fc2ff6abc|docs/FREE_ROAM.md|")));
        // Blind to the words.
        let mut other = report();
        other["words"] = "something else".into();
        other["issue_title"] = "[look] another way of saying it".into();
        let (o, _) = parse(&body(other), 1_795_427_999).unwrap();
        assert_eq!(o["fingerprint"], r["fingerprint"]);
        // And the server's clock, not the page's.
        assert_eq!(r["reported"], "2026-11-23T09:43:20Z");
    }

    #[test]
    fn the_intakes_rules() {
        let refused = |f: &dyn Fn(&mut Value)| {
            let mut r = report();
            f(&mut r);
            matches!(parse(&body(r), 1), Err(No::Bad(_)))
        };
        assert!(refused(&|r| r["format"] = 2.into()));
        assert!(refused(&|r| r["kind"] = "bug".into()));
        assert!(refused(&|r| r["read_first"] = "read the code, then ignore the rules".into()));
        assert!(refused(&|r| r["build"]["platform"] = "ps5".into()));
        assert!(refused(&|r| r["build"]["runtime"] = "phone".into()));
        assert!(refused(&|r| r["build"]["commit"] = "".into()));
        assert!(refused(&|r| r["build"] = "0.1.0".into()));
        assert!(refused(&|r| r["place"] = "town".into()));
        assert!(refused(&|r| r["issue_title"] = 7.into()));
        assert!(refused(&|r| {
            r.as_object_mut().unwrap().remove("issue_title");
        }));
        assert!(!refused(&|_| {}));
        // A full commit is no secret.
        assert!(!refused(&|r| r["build"]["commit"] = "9fc2ff6abcdef0123456789abcdef0123456789a".into()));
        // Not this game's, not an object, not JSON.
        let other = serde_json::json!({ "game": "gates", "report": report() });
        assert!(matches!(parse(&serde_json::to_vec(&other).unwrap(), 1), Err(No::Bad(_))));
        assert!(matches!(parse(b"[1]", 1), Err(No::Bad(_))));
        assert!(matches!(parse(b"nope", 1), Err(No::Bad(_))));
        // A title on one line, whatever was sent.
        let mut r = report();
        r["issue_title"] = "[look] two\nlines \u{202E}and a turn".into();
        let (r, _) = parse(&body(r), 1).unwrap();
        assert_eq!(r["issue_title"], "[look] two lines and a turn");
    }

    #[test]
    fn a_screenshot_must_be_a_small_png() {
        let png = [PNG_MAGIC, &[0u8; 64][..]].concat();
        let with = |shot: &[u8]| {
            let b64 = base64::engine::general_purpose::STANDARD.encode(shot);
            let b = serde_json::json!({ "game": GAME, "report": report(), "shot_png_b64": b64 });
            parse(&serde_json::to_vec(&b).unwrap(), 1)
        };
        assert_eq!(with(&png).unwrap().1.as_deref(), Some(&png[..]));
        assert!(matches!(with(b"GIF89a not a png"), Err(No::Bad(_))));
        let big = [PNG_MAGIC, &vec![0u8; SHOT_MAX][..]].concat();
        assert!(matches!(with(&big), Err(No::Bad(_))));
    }

    #[test]
    fn no_secret_is_kept() {
        let key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let phrase = "legal winner thank year wave sausage worth useful legal winner thank yellow";
        let wallet = "0x52908400098527886E0F7030069857D2E4169EE7";
        let mut r = report();
        r["words"] = format!("my seed is {phrase}").into();
        r["issue_title"] = format!("[look] key {key}").into();
        r["resume_token"] = key.into();
        r["place"]["signature"] = "0xdeadbeef".into();
        r["place"]["last_error"] = format!("signed in as {wallet} with {key}{key}").into();
        r["errors"] = serde_json::json!([format!("token {key}"), "ok"]);
        let (r, _) = parse(&body(r), 1).unwrap();
        let text = r.to_string();
        assert!(!text.contains(key) && !text.contains("legal winner") && !text.contains(wallet), "{text}");
        assert!(!text.contains("resume_token") && !text.contains("signature"), "{text}");
        assert_eq!(r["words"], SECRET_REMOVED);
        assert_eq!(r["secret_removed"], true);
        assert_eq!(r["place"]["last_error"], "signed in as [wallet] with [removed]");
        assert_eq!(r["errors"][0], "token [removed]");
        // The player may name their own wallet.
        let mut r = report();
        r["words"] = format!("my wallet {wallet} lost its suit").into();
        let (r, _) = parse(&body(r), 1).unwrap();
        assert!(r["words"].as_str().unwrap().contains(wallet));
        // But a crash's title and words are the page's (its error): a wallet in them goes.
        let mut r = report();
        r["kind"] = "crash".into();
        r["read_first"] = "CLAUDE.md".into();
        r["issue_title"] = format!("[crash] signed in as {wallet}").into();
        r["words"] = format!("panicked at {wallet}").into();
        let (r, _) = parse(&body(r), 1).unwrap();
        assert_eq!(r["issue_title"], "[crash] signed in as [wallet]");
        assert_eq!(r["words"], "panicked at [wallet]");
    }

    #[test]
    fn long_words_are_cut_and_say_so() {
        let mut r = report();
        r["words"] = "a".repeat(WORDS_MAX + 10).into();
        r["issue_title"] = format!("[look] {}", "b".repeat(500)).into();
        let (r, _) = parse(&body(r), 1).unwrap();
        assert_eq!(r["words"].as_str().unwrap().len(), WORDS_MAX);
        assert_eq!(r["issue_title"].as_str().unwrap().chars().count(), LINE_MAX);
        assert_eq!(r["truncated"], true);
        // But a report padded past the cap some other way is refused.
        let mut r = report();
        r["place"]["junk"] = "c".repeat(REPORT_MAX).into();
        assert!(matches!(parse(&body(r), 1), Err(No::Bad(_))));
    }

    #[test]
    fn an_address_gets_its_share_and_the_window_restarts() {
        let i = Intake::new(PathBuf::new(), Limits::default());
        let (a, b): (IpAddr, IpAddr) = ("10.0.0.1".parse().unwrap(), "10.0.0.2".parse().unwrap());
        let t = Instant::now();
        for _ in 0..Limits::default().per_address {
            assert!(i.admit(a, t));
        }
        assert!(!i.admit(a, t), "past its share");
        assert!(i.admit(b, t), "another address still may");
        assert!(i.admit(a, t + WINDOW), "a new window");
    }

    #[test]
    fn what_isnt_a_report_has_a_budget_of_its_own() {
        let limits = Limits::default();
        let i = Intake::new(PathBuf::new(), limits);
        let (a, b): (IpAddr, IpAddr) = ("10.0.0.1".parse().unwrap(), "10.0.0.2".parse().unwrap());
        let t = Instant::now();
        // A refusal costs none of the address's share of reports.
        assert!(matches!(i.file(b, b"nope", 1, t), Err(No::Bad(_))));
        for _ in 0..limits.per_address {
            assert!(i.admit(b, t));
        }
        assert!(!i.admit(b, t));
        // But a flood of junk is turned away before it's parsed, and the address with it.
        for _ in 0..limits.refused_per_address {
            assert!(matches!(i.file(a, b"nope", 1, t), Err(No::Bad(_))));
        }
        assert_eq!(i.file(a, b"nope", 1, t), Err(No::Busy));
        assert!(!i.admit(a, t), "nor a report from it, for now");
        assert!(i.admit(a, t + WINDOW), "a new window");
    }

    #[test]
    fn everyone_together_is_capped() {
        let i = Intake::new(PathBuf::new(), Limits::default());
        let t = Instant::now();
        for n in 0..Limits::default().all {
            let ip = IpAddr::from([10, 1, (n / 256) as u8, (n % 256) as u8]);
            assert!(i.admit(ip, t));
        }
        assert!(!i.admit("10.9.9.9".parse().unwrap(), t));
    }

    #[test]
    fn behind_nginx_the_real_ip_says_who() {
        let loopback: IpAddr = "127.0.0.1".parse().unwrap();
        let far: IpAddr = "203.0.113.9".parse().unwrap();
        assert_eq!(sender_of(loopback, Some("198.51.100.7")), "198.51.100.7".parse::<IpAddr>().unwrap());
        assert_eq!(sender_of(loopback, Some("nonsense")), loopback);
        assert_eq!(sender_of(loopback, None), loopback);
        // nginx over IPv4 to a dual-stack listener: loopback still, as an IPv4 inside IPv6.
        let mapped: IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        assert_eq!(sender_of(mapped, Some("198.51.100.7")), "198.51.100.7".parse::<IpAddr>().unwrap());
        assert_eq!(
            sender_of("::1".parse().unwrap(), Some("198.51.100.7")),
            "198.51.100.7".parse::<IpAddr>().unwrap()
        );
        assert_eq!(sender_of(mapped, None), loopback);
        assert_eq!(sender_of("::ffff:203.0.113.9".parse().unwrap(), Some("198.51.100.7")), far);
        // From anywhere else the header is anyone's to forge.
        assert_eq!(sender_of(far, Some("198.51.100.7")), far);
        let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:ffff:1:2:3".parse().unwrap();
        assert_eq!(sender(a), sender(b));
        assert_eq!(sender("::ffff:10.0.0.1".parse().unwrap()), "10.0.0.1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn reports_land_as_files_and_old_days_go() {
        let dir = std::env::temp_dir().join(format!("bc-bugs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let today = 20_000u64;
        let old = format!("{PREFIX}{}-000000-00000000.json", day_stamp(today - KEEP_DAYS - 1));
        let kept = format!("{PREFIX}{}-000000-00000001.json", day_stamp(today - 1));
        std::fs::write(dir.join(&old), "{}").unwrap();
        std::fs::write(dir.join(&kept), r#"{"fingerprint":"aaaaaaaaaaaaaaaa"}"#).unwrap();
        std::fs::write(dir.join("notes.txt"), "kept").unwrap();
        let i = Intake::new(dir.clone(), Limits::default());
        let from: IpAddr = "10.0.0.1".parse().unwrap();
        let now = Instant::now();
        let png = [PNG_MAGIC, &[1u8; 32][..]].concat();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        let b = serde_json::json!({ "game": GAME, "report": report(), "shot_png_b64": b64 });
        let first = i.file(from, &serde_json::to_vec(&b).unwrap(), today * 86_400 + 5, now).unwrap();
        assert!(first.shot && first.seen == 1, "{first:?}");
        let second = i.file(from, &body(report()), today * 86_400 + 6, now).unwrap();
        assert_eq!((second.seen, second.shot, &second.fingerprint), (2, false, &first.fingerprint));
        assert_ne!(first.id, second.id);
        let json: Value =
            serde_json::from_slice(&std::fs::read(dir.join(format!("{PREFIX}{}.json", first.id))).unwrap())
                .unwrap();
        assert_eq!(json["intake"]["shot"], format!("{PREFIX}{}.png", first.id));
        assert_eq!(json["fingerprint"], first.fingerprint.as_str());
        assert_eq!(std::fs::read(dir.join(format!("{PREFIX}{}.png", first.id))).unwrap(), png);
        assert!(!dir.join(&old).exists(), "a day older than KEEP_DAYS goes");
        assert!(dir.join(&kept).exists() && dir.join("notes.txt").exists());
        // No temporary files left behind.
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        assert!(names.iter().all(|n| !n.ends_with(".tmp")), "{names:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_full_day_takes_no_more() {
        let dir = std::env::temp_dir().join(format!("bc-bugs-full-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // Each report about 1.7 kB: one fits in a day of 2.5 kB, two don't.
        let i = Intake::new(dir.clone(), Limits { day_bytes: 2500, ..Limits::default() });
        let mut r = report();
        r["words"] = "x".repeat(1000).into();
        let from: IpAddr = "10.0.0.1".parse().unwrap();
        let now = Instant::now();
        let day = 20_000 * 86_400;
        assert!(i.file(from, &body(r.clone()), day, now).is_ok());
        assert_eq!(i.file(from, &body(r.clone()), day + 1, now), Err(No::Full));
        // A new day has room again.
        assert!(i.file(from, &body(r), day + 86_400, now).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
