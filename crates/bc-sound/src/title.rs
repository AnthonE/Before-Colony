//! The title theme: an original piece in the manner of a mid-90s anime opening as a Super Famicom
//! game played it, at 143 BPM in E minor, 28 bars (about 47 s) that loop. It plays on the
//! [`crate::spc`] chip: eight voices, instruments stored as BRR, the chip's echo.
//!
//! - **Intro** (bars 1-4): orchestra hits, brass and bass on a 3-3-2, a tom fill.
//! - **Hook** (5-8): the lead over the full groove, Em C D B, open hats on the off-beats.
//! - **Verse** (9-16): Em C D, sparser, eighth-note arpeggios.
//! - **Pre-chorus** (17-20): Am Bm C B, brass climbing, a snare roll.
//! - **Chorus** (21-28): C D Bm Em (J-pop's "royal road"), a harmony under the lead, and a fill
//!   back to the top.
//!
//! The score is MML (music macro language, the way SNES composers wrote), a string a voice, with
//! the drums as step grids. The instruments are waveforms built here and stored as BRR; they and
//! the echo buffer fit in the chip's 64 KB.

use std::f32::consts::TAU;

use crate::music::midi;
use crate::spc::{self, Adsr, Dsp, Echo, Sample, adsr};
use crate::synth::{Lp, Rng};

/// Beats a minute.
pub const BPM: f64 = 143.0;
/// Bars before it loops.
pub const BARS: u32 = 28;
/// Ticks a bar of 4/4 (an MML whole note), as sound drivers count them.
const BAR: u32 = 192;
/// Samples a tick.
const TICK: f64 = spc::RATE as f64 * 60.0 / (BPM * (BAR / 4) as f64);

/// The loop's length, samples a channel.
pub fn len() -> usize {
    at(BARS * BAR)
}

/// Where tick `t` falls, samples.
fn at(t: u32) -> usize {
    (f64::from(t) * TICK).round() as usize
}

// --- The instruments. ---

/// Tonal samples peak here (15-bit), drums a little higher.
const TONE: f32 = 12_288.0;
const DRUM: f32 = 14_336.0;

/// An instrument, as a driver's table has one: a sample, its tuning, its envelope.
struct Inst {
    sample: usize,
    /// The frequency the sample sounds at pitch 0x1000.
    unity: f32,
    adsr: Adsr,
    /// Vibrato depth, cents.
    vibrato: f32,
}

/// Drums sampled at 16 kHz play middle C at pitch 0x800; at 32 kHz, at 0x1000.
const DRUM_16K: f32 = 523.251_1;
const DRUM_32K: f32 = 261.625_6;

// The instruments' numbers (MML `@n`).
const KICK: u8 = 6;
const SNARE: u8 = 7;
const HAT: u8 = 8;
const OPEN_HAT: u8 = 9;
const CRASH: u8 = 10;

const INSTRUMENTS: [Inst; 11] = [
    // @0 lead, with vibrato.
    Inst { sample: 0, unity: 1_000.0, adsr: adsr(14, 7, 6, 8), vibrato: 18.0 },
    // @1 bass: struck, then settling.
    Inst { sample: 1, unity: 250.0, adsr: adsr(15, 5, 3, 16), vibrato: 0.0 },
    // @2 strings: slow in, held.
    Inst { sample: 2, unity: 500.0, adsr: adsr(9, 0, 7, 0), vibrato: 0.0 },
    // @3 brass: quick in, falling away.
    Inst { sample: 3, unity: 500.0, adsr: adsr(12, 4, 4, 14), vibrato: 0.0 },
    // @4 pluck, for arpeggios.
    Inst { sample: 4, unity: 1_000.0, adsr: adsr(15, 5, 1, 18), vibrato: 0.0 },
    // @5 orchestra hit.
    Inst { sample: 5, unity: 250.0, adsr: adsr(15, 3, 3, 16), vibrato: 0.0 },
    // @6 kick (tuned up, the toms), @7 snare: the samples shape them.
    Inst { sample: 6, unity: DRUM_16K, adsr: adsr(15, 0, 7, 0), vibrato: 0.0 },
    Inst { sample: 7, unity: DRUM_16K, adsr: adsr(15, 0, 7, 0), vibrato: 0.0 },
    // @8 closed hat and @9 open: one sample, cut short or let ring.
    Inst { sample: 8, unity: DRUM_32K, adsr: adsr(15, 7, 0, 26), vibrato: 0.0 },
    Inst { sample: 8, unity: DRUM_32K, adsr: adsr(15, 2, 3, 19), vibrato: 0.0 },
    // @10 crash.
    Inst { sample: 9, unity: DRUM_32K, adsr: adsr(15, 1, 4, 13), vibrato: 0.0 },
];

/// Vibrato comes in this long after a note starts, ticks, at this rate.
const VIBRATO_DELAY: u32 = 24;
const VIBRATO_HZ: f32 = 5.5;

/// The samples, in the order [`INSTRUMENTS`] number them.
fn bank() -> Vec<Sample> {
    vec![lead(), bass(), strings(), brass(), pluck(), orchestra(), kick(), snare(), hat(), crash()]
}

/// To 15 bits, peaking at `peak`.
fn quantize(wave: &[f32], peak: f32) -> Vec<i16> {
    let max = wave.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-9);
    wave.iter().map(|v| (v / max * peak).round() as i16).collect()
}

/// `cycles` periods of `period` samples, each a sum of harmonics at `amp(cycle, harmonic)`. Sine
/// phase: every period starts and ends at zero, so a looped one joins cleanly.
fn additive(period: usize, cycles: usize, amp: impl Fn(usize, usize) -> f32) -> Vec<f32> {
    let sine: Vec<f32> = (0..period).map(|i| (TAU * i as f32 / period as f32).sin()).collect();
    let mut out = vec![0.0; period * cycles];
    for c in 0..cycles {
        for h in 1..period / 2 {
            let a = amp(c, h);
            if a != 0.0 {
                for (i, v) in out[c * period..(c + 1) * period].iter_mut().enumerate() {
                    *v += a * sine[h * i % period];
                }
            }
        }
    }
    out
}

/// A tone that settles: periods whose harmonics move from `amp(0, h)` on, looping on the last.
fn tone(period: usize, cycles: usize, amp: impl Fn(usize, usize) -> f32) -> Sample {
    Sample::new(&quantize(&additive(period, cycles, amp), TONE), Some((cycles - 1) * period))
}

/// Falls away above harmonic `cut`, the steeper the higher `slope` (about 6 dB an octave each).
fn rolloff(h: usize, cut: f32, slope: i32) -> f32 {
    1.0 / (1.0 + (h as f32 / cut).powi(slope))
}

/// Odd harmonics full and even ones about half (between a square and a saw), a little brighter
/// as the note starts.
fn lead() -> Sample {
    tone(32, 7, |c, h| {
        let body = if h % 2 == 1 { 1.0 } else { 0.55 } / h as f32;
        let onset = if h >= 4 { 1.0 + 0.6 * (1.0 - c as f32 / 6.0) } else { 1.0 };
        body * onset
    })
}

/// A saw through a closing filter, the second harmonic popping at the start: a slapped string.
fn bass() -> Sample {
    tone(128, 13, |c, h| {
        let cut = 5.0 + 25.0 * (-(c as f32) / 2.5).exp();
        let pop = if h == 2 { 1.0 + (-(c as f32) / 2.0).exp() } else { 1.0 };
        pop / h as f32 * rolloff(h, cut, 4)
    })
}

/// Three soft saws a little apart (127, 128 and 129 periods in the loop): a section's shimmer.
fn strings() -> Sample {
    const LEN: usize = 8_192;
    let sine: Vec<f32> = (0..LEN).map(|i| (TAU * i as f32 / LEN as f32).sin()).collect();
    let mut wave = vec![0.0f32; LEN];
    for periods in [127, 128, 129] {
        for h in 1..=24 {
            let a = rolloff(h, 9.0, 2) / h as f32;
            for (i, w) in wave.iter_mut().enumerate() {
                *w += a * sine[periods * h * i % LEN];
            }
        }
    }
    Sample::new(&quantize(&wave, TONE), Some(0))
}

/// A saw that opens up over its first dozen periods.
fn brass() -> Sample {
    tone(64, 17, |c, h| {
        let cut = 3.0 + 11.0 * (c as f32 / 12.0).min(1.0);
        rolloff(h, cut, 3) / h as f32
    })
}

/// Bright at the pluck, settling to nearly a sine.
fn pluck() -> Sample {
    tone(32, 25, |c, h| (-((h - 1) as f32) * c as f32 / 6.0).exp() / h as f32)
}

/// Root, octave, twelfth, two octaves and up (no third, so it hits on any chord), bright at the
/// start and with a burst of bow noise.
fn orchestra() -> Sample {
    const PERIOD: usize = 128;
    const CYCLES: usize = 25;
    let mut partials = [0.0f32; PERIOD / 2];
    for (note, level) in [(1, 1.0), (2, 0.9), (3, 0.7), (4, 0.6), (6, 0.4), (8, 0.3)] {
        for k in 1..PERIOD / 2 / note {
            partials[note * k] += level / k as f32;
        }
    }
    let mut wave =
        additive(PERIOD, CYCLES, |c, h| partials[h] * rolloff(h, 10.0 + 40.0 * (-(c as f32) / 5.0).exp(), 2));
    let peak = wave.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let mut r = Rng::new(0x0C4E);
    for (i, w) in wave.iter_mut().enumerate() {
        *w += 0.35 * peak * r.noise() * (-(i as f32) / 300.0).exp();
    }
    Sample::new(&quantize(&wave, TONE), Some((CYCLES - 1) * PERIOD))
}

/// The last `n` of `len` samples fade out.
fn tail(i: usize, len: usize, n: usize) -> f32 {
    ((len - i) as f32 / n as f32).min(1.0)
}

/// A sine falling from 188 Hz to 48 Hz with a click on top (16 kHz).
fn kick() -> Sample {
    const LEN: usize = 4_800;
    let mut r = Rng::new(0x41C);
    let mut phase = 0.0f32;
    let wave: Vec<f32> = (0..LEN)
        .map(|i| {
            let t = i as f32 / 16_000.0;
            phase += (48.0 + 140.0 * (-t / 0.035).exp()) / 16_000.0;
            let body = (TAU * phase).sin() * (-t / 0.1).exp();
            let click = 0.4 * r.noise() * (-t / 0.0015).exp();
            (body + click) * tail(i, LEN, 80)
        })
        .collect();
    Sample::new(&quantize(&wave, DRUM), None)
}

/// A drum's tone under band-passed noise, cut short (16 kHz).
fn snare() -> Sample {
    const LEN: usize = 3_200;
    let mut r = Rng::new(0x5A4E);
    let (mut low, mut high) = (Lp::at_rate(1_200.0, 16_000.0), Lp::at_rate(6_500.0, 16_000.0));
    let wave: Vec<f32> = (0..LEN)
        .map(|i| {
            let t = i as f32 / 16_000.0;
            let body = ((TAU * 190.0 * t).sin() * 0.7 + (TAU * 330.0 * t).sin() * 0.35) * (-t / 0.045).exp();
            let n = r.noise();
            let rattle = 1.6 * high.run(n - low.run(n)) * (-t / 0.075).exp();
            (body + rattle) * tail(i, LEN, 160)
        })
        .collect();
    Sample::new(&quantize(&wave, DRUM), None)
}

/// High noise: a tick, then a steady stretch that loops (the envelope makes it closed or open).
fn hat() -> Sample {
    let mut r = Rng::new(0x4A7);
    let (mut a, mut b) = (Lp::at_rate(7_000.0, 32_000.0), Lp::at_rate(7_000.0, 32_000.0));
    let wave: Vec<f32> = (0..1_536)
        .map(|i| {
            let n = r.noise();
            let once = n - a.run(n);
            let twice = once - b.run(once);
            twice * (1.0 + 1.5 * (-(i as f32) / 100.0).exp())
        })
        .collect();
    Sample::new(&quantize(&wave, DRUM), Some(512))
}

/// Brighter, longer noise that loops while the envelope lets it ring.
fn crash() -> Sample {
    let mut r = Rng::new(0xC4A5);
    let (mut low, mut top) = (Lp::at_rate(3_500.0, 32_000.0), Lp::at_rate(11_000.0, 32_000.0));
    let wave: Vec<f32> = (0..4_096)
        .map(|i| {
            let n = r.noise();
            top.run(n - low.run(n)) * (1.0 + 1.2 * (-(i as f32) / 600.0).exp())
        })
        .collect();
    Sample::new(&quantize(&wave, DRUM), Some(2_048))
}

// --- The score. ---

/// A note as the driver plays it.
#[derive(Clone, Copy, Debug)]
struct Note {
    /// Key on and key off, ticks.
    on: u32,
    off: u32,
    /// MIDI (60 is middle C).
    key: u8,
    inst: u8,
    /// 0..=127.
    vol: u8,
    /// 0 left, 10 centre, 20 right.
    pan: u8,
}

/// A voice's notes in order, and its length in ticks.
struct Part {
    notes: Vec<Note>,
    ticks: u32,
}

/// When a note `len` ticks long keys off at gate `q` (it sounds q/8 of its length, and always
/// lets go a tick before the next, as drivers do).
fn gated(len: u32, q: u32) -> u32 {
    (len * q / 8).clamp(1, len.saturating_sub(1).max(1))
}

/// Reads MML: `o4` octave (o4 c is middle C), `<` `>` down and up one; notes `c d e f g a b` with
/// `+` sharp or `-` flat, then a length (`4` a quarter, `8.` a dotted eighth, none: the `l`
/// default); `^` ties more on, `r` rests; `@n` instrument, `v` volume (0-127), `y` pan (0 left,
/// 10 centre, 20 right), `q` gate (sounds q/8 of a note); `[...]n` repeats; `|` a bar line, which
/// is checked; `;` a comment to the end of the line.
///
/// The score is a constant, so a mistake in it panics (and the tests play it).
fn mml(src: &str) -> Part {
    let mut m = Mml {
        src: src.as_bytes(),
        at: 0,
        tick: 0,
        octave: 4,
        len: BAR / 8,
        inst: 0,
        vol: 64,
        pan: 10,
        gate: 8,
        notes: Vec::new(),
        tied: None,
    };
    m.seq(0);
    Part { notes: m.notes, ticks: m.tick }
}

struct Mml<'a> {
    src: &'a [u8],
    at: usize,
    tick: u32,
    octave: i32,
    len: u32,
    inst: u8,
    vol: u8,
    pan: u8,
    gate: u32,
    notes: Vec<Note>,
    /// The last note's full length and gate, while a `^` may still lengthen it.
    tied: Option<(u32, u32)>,
}

impl Mml<'_> {
    fn seq(&mut self, depth: u32) {
        while let Some(&c) = self.src.get(self.at) {
            self.at += 1;
            match c {
                b' ' | b'\t' | b'\r' | b'\n' => {}
                b';' => {
                    while self.src.get(self.at).is_some_and(|&c| c != b'\n') {
                        self.at += 1;
                    }
                }
                b'|' => assert!(
                    self.tick.is_multiple_of(BAR),
                    "{}: a bar line {} ticks into a bar",
                    self.line(),
                    self.tick % BAR
                ),
                b'o' => self.octave = self.number() as i32,
                b'>' => self.octave += 1,
                b'<' => self.octave -= 1,
                b'l' => self.len = self.length(),
                b'@' => self.inst = self.number() as u8,
                b'v' => self.vol = self.number() as u8,
                b'y' => self.pan = self.number() as u8,
                b'q' => self.gate = self.number(),
                b'[' => {
                    let body = self.at;
                    self.seq(depth + 1);
                    let times = self.number();
                    let after = self.at;
                    for _ in 1..times {
                        self.at = body;
                        self.seq(depth + 1);
                    }
                    self.at = after;
                }
                b']' => {
                    assert!(depth > 0, "{}: `]` without `[`", self.line());
                    return;
                }
                b'r' => {
                    self.tick += self.length();
                    self.tied = None;
                }
                b'^' => {
                    let more = self.length();
                    let (len, q) = self.tied.expect("`^` ties onto a note");
                    let n = self.notes.last_mut().expect("a tied note");
                    n.off = n.on + gated(len + more, q);
                    self.tied = Some((len + more, q));
                    self.tick += more;
                }
                b'a'..=b'g' => self.note(c),
                _ => panic!("{}: `{}` isn't MML", self.line(), c as char),
            }
        }
        assert!(depth == 0, "a `[` without its `]`");
    }

    fn note(&mut self, name: u8) {
        let mut key = 12 * (self.octave + 1)
            + match name {
                b'c' => 0,
                b'd' => 2,
                b'e' => 4,
                b'f' => 5,
                b'g' => 7,
                b'a' => 9,
                _ => 11,
            };
        loop {
            match self.src.get(self.at) {
                Some(b'+' | b'#') => key += 1,
                Some(b'-') => key -= 1,
                _ => break,
            }
            self.at += 1;
        }
        let len = self.length();
        let key = u8::try_from(key).expect("a note in MIDI's range");
        self.notes.push(Note {
            on: self.tick,
            off: self.tick + gated(len, self.gate),
            key,
            inst: self.inst,
            vol: self.vol,
            pan: self.pan,
        });
        self.tied = Some((len, self.gate));
        self.tick += len;
    }

    /// A length in ticks: a note value (`4`, `16`...), then dots.
    fn length(&mut self) -> u32 {
        let base = match self.digits() {
            Some(n) => {
                assert!(n > 0 && BAR.is_multiple_of(n), "{}: no note is 1/{n} of a bar", self.line());
                BAR / n
            }
            None => self.len,
        };
        let (mut len, mut dot) = (base, base);
        while self.src.get(self.at) == Some(&b'.') {
            self.at += 1;
            dot /= 2;
            len += dot;
        }
        len
    }

    fn digits(&mut self) -> Option<u32> {
        let from = self.at;
        while self.src.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        std::str::from_utf8(&self.src[from..self.at]).ok()?.parse().ok()
    }

    fn number(&mut self) -> u32 {
        self.digits().unwrap_or_else(|| panic!("{}: a number", self.line()))
    }

    /// Where the parser is, for a mistake's message.
    fn line(&self) -> String {
        let line = 1 + self.src[..self.at].iter().filter(|&&c| c == b'\n').count();
        format!("MML line {line}")
    }
}

/// A drum from the kit.
#[derive(Clone, Copy)]
struct Hit {
    inst: u8,
    key: u8,
    vol: u8,
    pan: u8,
}

const fn hit(inst: u8, key: u8, vol: u8, pan: u8) -> Hit {
    Hit { inst, key, vol, pan }
}

/// The step grids' letters.
const KIT: [(char, Hit); 9] = [
    ('k', hit(KICK, 60, 26, 10)),
    ('s', hit(SNARE, 60, 40, 10)),
    ('S', hit(SNARE, 60, 54, 10)),
    // Toms: the kick, tuned up, across the stereo field.
    ('t', hit(KICK, 74, 38, 7)),
    ('m', hit(KICK, 69, 38, 10)),
    ('f', hit(KICK, 64, 38, 13)),
    ('h', hit(HAT, 60, 36, 13)),
    ('o', hit(OPEN_HAT, 60, 38, 13)),
    ('x', hit(CRASH, 60, 40, 7)),
];

/// A drum voice's part from a step grid, a character a sixteenth: a letter from [`KIT`], or `.` to
/// let the last hit ring. A hit sounds until the next; `|` is a bar line, checked; `;` a comment.
fn steps(grid: &str) -> Part {
    let mut notes: Vec<Note> = Vec::new();
    let mut tick: u32 = 0;
    let mut comment = false;
    for c in grid.chars() {
        match c {
            '\n' => comment = false,
            _ if comment => {}
            ';' => comment = true,
            ' ' => {}
            '|' => assert!(tick.is_multiple_of(BAR), "a drum bar line {} ticks into a bar", tick % BAR),
            '.' => tick += BAR / 16,
            _ => {
                let (_, h) = KIT.iter().find(|(k, _)| *k == c).unwrap_or_else(|| panic!("no drum `{c}`"));
                notes.push(Note { on: tick, off: 0, key: h.key, inst: h.inst, vol: h.vol, pan: h.pan });
                tick += BAR / 16;
            }
        }
    }
    let next: Vec<u32> = notes.iter().skip(1).map(|n| n.on).chain([tick]).collect();
    for (n, next) in notes.iter_mut().zip(next) {
        n.off = next - 1;
    }
    Part { notes, ticks: tick }
}

const LEAD: &str = "
@0 v31 y8 q7 o5
; Intro: the orchestra has it.
r1 | r1 | r1 | r1 |
; Hook: Em C D B.
e8. g8. b8 a8 g8 f+8 g8 |
e8. g8. >c8< b8 g8 e8 g8 |
f+8. a8. >d8 c8< b8 a8 b8 |
b4. a8 f+4 d+4 |
; Verse: Em Em C D, twice.
o4 r8 b8 b8 >e8 d8. <b8. g8 |
a8. b8. g8 e2 |
r8 >c8 c8 e8 d8. c8. <g8 |
a8. b8. a8 f+2 |
r8 b8 b8 >e8 g8. f+8. e8 |
d8. e8. d8 <b2 |
r8 >c8 e8 g8 a8. g8. e8 |
f+8. e8. d8 <a2 |
; Pre-chorus: Am Bm C B, climbing.
>e4. d8 c4 <a4 |
>f+4. e8 d4 <b4 |
>g4. e8 g4 >c4 |
<b2 a4 f+8 d+8 |
; Chorus: C D | Bm Em | C D | Em, then again to B.
g4. e8 f+4 g8 a8 |
b4. a8 g4 f+8 e8 |
e8 e8 g8 >c8< a4 f+8 a8 |
b2^8 a8 g8 f+8 |
g4. e8 f+4 g8 a8 |
b4. >d8 e4 d8 <b8 |
>c4< b8 g8 a4 f+8 a8 |
b4. a8 f+4 d+4 |
";

/// The chords' upper voice.
const HIGH: &str = "
@3 v15 y5 q3 o4
; Intro: brass on the hits, then a held B.
b4. b4. b4 | g4. g4. g4 | a4. a4. a4 |
q8 f+1 |
; Hook: strings.
@2 v13
b1 | g1 | a1 | f+1 |
; Verse
b1 | ^1 | g1 | a1 | b1 | ^1 | g1 | a1 |
; Pre-chorus: brass, climbing.
@3 v15 q3 o5
e4. e4. e4 | f+4. f+4. f+4 | g4. g4. g4 |
q8 f+1 |
; Chorus: strings, two chords a bar.
@2 v13 o4
g2 a2 | f+2 b2 | g2 a2 | b1 | g2 a2 | f+2 b2 | g2 a2 | f+1 |
";

/// The chords' lower voice.
const LOW: &str = "
@3 v15 y15 q3 o4
g4. g4. g4 | e4. e4. e4 | f+4. f+4. f+4 |
q8 d+1 |
@2 v13
g1 | e1 | f+1 | d+1 |
g1 | ^1 | e1 | f+1 | g1 | ^1 | e1 | f+1 |
@3 v15 q3 o5
c4. c4. c4 | d4. d4. d4 | e4. e4. e4 |
q8 d+1 |
@2 v13 o4
e2 f+2 | d2 g2 | e2 f+2 | g1 | e2 f+2 | d2 g2 | e2 f+2 | d+1 |
";

const BASS: &str = "
@1 v31 y10 q6
; Intro: with the hits, then eighths into the hook.
o2 e4. e4. e4 | c4. c4. c4 | d4. d4. d4 |
o1 [b8 >b8<]4 |
; Hook: octaves.
o2 [e8 >e8<]4 | [c8 >c8<]4 | [d8 >d8<]4 |
o1 [b8 >b8<]4 |
; Verse: the root, jumping up on the seventh eighth.
o2 [e8 e8 e8 e8 e8 e8 >e8< e8 |]2
c8 c8 c8 c8 c8 c8 >c8< c8 |
d8 d8 d8 d8 d8 d8 >d8< d8 |
[e8 e8 e8 e8 e8 e8 >e8< e8 |]2
c8 c8 c8 c8 c8 c8 >c8< c8 |
d8 d8 d8 d8 d8 d8 >d8< d8 |
; Pre-chorus: climbing.
o2 [a8 >a8<]4 | [b8 >b8<]4 | o3 [c8 >c8<]4 | o2 [b8 >b8<]4 |
; Chorus: two chords a bar.
o2 [c8 >c8<]2 [d8 >d8<]2 |
o1 [b8 >b8<]2 o2 [e8 >e8<]2 |
[c8 >c8<]2 [d8 >d8<]2 |
[e8 >e8<]4 |
[c8 >c8<]2 [d8 >d8<]2 |
o1 [b8 >b8<]2 o2 [e8 >e8<]2 |
[c8 >c8<]2 [d8 >d8<]2 |
o1 [b8 >b8<]4 |
";

/// Orchestra hits, arpeggios, then the chorus's harmony.
const COLOUR: &str = "
; Intro: orchestra hits.
@5 v33 y11 q8 o4
e4. e4. e4 | c4. c4. c4 | d4. d4. d4 |
<b1 |
; Hook: sixteenth arpeggios.
@4 v19 y13 l16 o4
[e g b > e < b g e g]2 |
[e g > c e c < g e g]2 |
[f+ a > d f+ d < a f+ a]2 |
[f+ b > d+ f+ d+ < b f+ b]2 |
; Verse: eighths.
l8
[e g b > e < b g e g |]2
e g > c e c < g e g |
f+ a > d f+ d < a f+ a |
[e g b > e < b g e g |]2
e g > c e c < g e g |
f+ a > d f+ d < a f+ a |
; Pre-chorus: sixteenths.
l16
[e a > c e c < a e a]2 |
[f+ b > d f+ d < b f+ b]2 |
[e g > c e c < g e g]2 |
[f+ b > d+ f+ d+ < b f+ b]2 |
; Chorus: a harmony under the lead.
@0 v19 y14 q7 l8 o5
e4. c8 d4 e8 f+8 |
f+4. f+8 e4 d8 <b8 |
>c8 c8 e8 g8 f+4 d8 f+8 |
g2^8 f+8 e8 d8 |
e4. c8 d4 e8 f+8 |
f+4. b8 b4 b8 g8 |
g4 g8 e8 f+4 d8 f+8 |
f+4. f+8 d+4 <b4 |
";

const KICKS: &str = "
; Intro
k.....k.....k... | k.....k.....k... | k.....k.....k... | k...k...k...k... |
; Hook
k...k...k...k... | k...k...k...k... | k...k...k...k... | k...k...k...k... |
; Verse
k.....k.k....... | k.....k.k....... | k.....k.k....... | k.....k.k....... |
k.....k.k....... | k.....k.k....... | k.....k.k....... | k.....k.k...k.k. |
; Pre-chorus
k...k...k...k... | k...k...k...k... | k...k...k...k... | k...k...k.k.k.k. |
; Chorus
k...k...k...k... | k...k...k...k... | k...k...k...k... | k...k...k...k... |
k...k...k...k... | k...k...k...k... | k...k...k...k... | k...k...k.k.k.k. |
";

const SNARES: &str = "
; Intro: on the hits, then a fill.
S.....S.....S... | S.....S.....S... | S.....S.....S... | ....s...ttmmffSS |
; Hook
....s.......s... | ....s.......s... | ....s.......s... | ....s.......s.ss |
; Verse
....s.......s... | ....s.......s... | ....s.......s... | ....s.......s... |
....s.......s... | ....s.......s... | ....s.......s... | ....s.......s.ss |
; Pre-chorus: a roll into the chorus.
....s.......s... | ....s.......s... | ....s.......s... | s.s.s.s.ssssSSSS |
; Chorus
....S.......S... | ....S.......S... | ....S.......S... | ....S.......S... |
....S.......S... | ....S.......S... | ....S.......S... | ....S...ttmmffSS |
";

const CYMBALS: &str = "
; Intro: a crash on each bar.
x............... | x............... | x............... | x............... |
; Hook: open hats on the off-beats.
x.......h.o.h.o. | h.o.h.o.h.o.h.o. | h.o.h.o.h.o.h.o. | h.o.h.o.h.o.h.o. |
; Verse
x.......h.h.h.h. | h.h.h.h.h.h.h.h. | h.h.h.h.h.h.h.h. | h.h.h.h.h.h.h.h. |
h.h.h.h.h.h.h.h. | h.h.h.h.h.h.h.h. | h.h.h.h.h.h.h.h. | h.h.h.h.h.h.h.h. |
; Pre-chorus
x.......hhhhhhhh | hhhhhhhhhhhhhhhh | hhhhhhhhhhhhhhhh | hhhhhhhhhhhhhhhh |
; Chorus
x.......h.o.h.o. | h.o.h.o.h.o.h.o. | h.o.h.o.h.o.h.o. | h.o.h.o.h.o.h.o. |
x.......h.o.h.o. | h.o.h.o.h.o.h.o. | h.o.h.o.h.o.h.o. | x............... |
";

/// The eight voices' parts, and whether each goes to the echo (the bass, kick and cymbals stay
/// dry).
fn score() -> [(Part, bool); spc::VOICES] {
    [
        (mml(LEAD), true),
        (mml(HIGH), true),
        (mml(LOW), true),
        (mml(BASS), false),
        (mml(COLOUR), true),
        (steps(KICKS), false),
        (steps(SNARES), true),
        (steps(CYMBALS), false),
    ]
}

/// MVOL.
const MASTER: i8 = 127;
/// The echo: 96 ms, fed back a little over half, a little darker each time round.
const ECHO: Echo = Echo { delay: 6, feedback: 72, volume: [36, 36], fir: [0, 0, 0, 0, 16, 32, 48, 32] };

/// VxVOL from a volume and a pan: the far side fades.
fn volumes(vol: u8, pan: u8) -> (i8, i8) {
    let v = i32::from(vol.min(127));
    let side = |d: i32| (v * d.clamp(0, 10) / 10) as i8;
    (side(20 - i32::from(pan)), side(i32::from(pan)))
}

/// The loop: stereo and planar (the left channel's samples, then the right's) at [`spc::RATE`].
pub fn title() -> Vec<f32> {
    render().0
}

/// The loop, and how many samples the chip's mix clamped.
fn render() -> (Vec<f32>, u32) {
    let bank = bank();
    let parts = score();
    let song = BARS * BAR;
    for (p, _) in &parts {
        assert_eq!(p.ticks, song, "every voice plays the whole song");
    }
    // Two seconds more for the echo and the last releases to die away, folded onto the top: the
    // end rings into the start as it would going round.
    let extra = (2.0 * f64::from(spc::RATE) / TICK).ceil() as u32;
    let total = at(song + extra);
    // Planar from the start: the left channel, then the right.
    let mut out = vec![0.0f32; 2 * total];
    let mut dsp = Dsp::new(&bank, MASTER, ECHO);
    let mut next = [0usize; spc::VOICES];
    let mut sounding: [Option<Note>; spc::VOICES] = [None; spc::VOICES];
    for tick in 0..song + extra {
        for (v, (part, echo)) in parts.iter().enumerate() {
            if sounding[v].is_some_and(|n| n.off == tick) {
                dsp.key_off(v);
            }
            if let Some(n) = part.notes.get(next[v]).filter(|n| n.on == tick) {
                let inst = &INSTRUMENTS[usize::from(n.inst)];
                let (l, r) = volumes(n.vol, n.pan);
                dsp.set_volume(v, l, r);
                dsp.set_echo(v, *echo);
                dsp.key_on(v, inst.sample, inst.adsr);
                sounding[v] = Some(*n);
                next[v] += 1;
            }
            // The pitch, with vibrato once a note has sounded a while.
            if let Some(n) = sounding[v] {
                let inst = &INSTRUMENTS[usize::from(n.inst)];
                let since = tick - n.on;
                let cents = if inst.vibrato > 0.0 && since >= VIBRATO_DELAY {
                    let t = (since - VIBRATO_DELAY) as f32 * TICK as f32 / spc::RATE as f32;
                    inst.vibrato * (TAU * VIBRATO_HZ * t).sin()
                } else {
                    0.0
                };
                let hz = midi(f32::from(n.key) + cents / 100.0);
                dsp.set_pitch(v, (4096.0 * hz / inst.unity).round() as u32);
            }
        }
        for i in at(tick)..at(tick + 1) {
            let [l, r] = dsp.frame();
            out[i] = f32::from(l) / 32_768.0;
            out[total + i] = f32::from(r) / 32_768.0;
        }
    }
    let len = len();
    for ch in out.chunks_exact_mut(total) {
        let (head, tail) = ch.split_at_mut(len);
        for (h, t) in head.iter_mut().zip(tail) {
            *h += *t;
        }
    }
    out.copy_within(total..total + len, len);
    out.truncate(2 * len);
    (out, dsp.clipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mml_reads_lengths_ties_repeats_and_octaves() {
        let p = mml("o4 l8 c d4. e16 ^16 r4. | [f+ >a-<]2 q4 b2 |");
        let keys: Vec<u8> = p.notes.iter().map(|n| n.key).collect();
        assert_eq!(keys, [60, 62, 64, 66, 80, 66, 80, 71]);
        let ons: Vec<u32> = p.notes.iter().map(|n| n.on).collect();
        assert_eq!(ons, [0, 24, 96, 192, 216, 240, 264, 288]);
        // A full gate lets go a tick early, the tie made the e an eighth, q4 holds half.
        assert_eq!(p.notes[0].off, 23);
        assert_eq!(p.notes[2].off, 96 + 23);
        assert_eq!(p.notes[7].off, 288 + 48);
        assert_eq!(p.ticks, 2 * BAR);
    }

    #[test]
    #[should_panic(expected = "a bar line")]
    fn a_short_bar_is_caught() {
        mml("c4 c4 c4 | c1 |");
    }

    #[test]
    fn every_voice_plays_every_bar() {
        for (p, _) in score() {
            assert_eq!(p.ticks, BARS * BAR);
            assert!(p.notes.windows(2).all(|w| w[0].off < w[1].on), "notes overlap");
        }
    }

    #[test]
    fn the_notes_are_in_e_minor() {
        // E natural minor, and D# for the B major chords (the drums are unpitched).
        let scale = [4, 6, 7, 9, 11, 0, 2, 3];
        for (p, _) in score().into_iter().take(5) {
            for n in &p.notes {
                assert!(scale.contains(&(n.key % 12)), "key {} at tick {}", n.key, n.on);
            }
        }
    }

    #[test]
    fn the_bank_and_the_echo_fit_the_chips_ram() {
        // Leave 8 KB of the 64 for a sound driver and the score.
        let samples: usize = bank().iter().map(|s| s.brr.len()).sum();
        assert!(samples + ECHO.ram() <= spc::ARAM - 8_192, "{samples} B of samples");
    }

    #[test]
    fn the_loop_is_stereo_clean_and_seamless() {
        let (buf, clipped) = render();
        assert_eq!(clipped, 0, "the mix clamps");
        let n = len();
        assert_eq!(buf.len(), 2 * n);
        let (l, r) = buf.split_at(n);
        for ch in [l, r] {
            assert!(ch.iter().all(|v| v.is_finite()));
            let rms = (ch.iter().map(|v| v * v).sum::<f32>() / n as f32).sqrt();
            assert!(rms > 0.02, "quiet: {rms}");
            // The seam: the last sample runs into the first no harder than the music moves elsewhere.
            let jump = (ch[n - 1] - ch[0]).abs();
            let step = ch.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
            assert!(jump <= step, "clicks at the loop point ({jump} > {step})");
        }
        assert!(l.iter().zip(r).any(|(a, b)| (a - b).abs() > 0.01), "it's stereo");
        // The song's length at 143 BPM.
        assert!((n as f64 / f64::from(spc::RATE) - f64::from(BARS) * 4.0 * 60.0 / BPM).abs() < 0.01);
    }
}
