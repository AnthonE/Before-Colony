//! The Super Famicom's sound chip in software: the S-DSP beside the SPC700, as nocash's fullsnes
//! documents the hardware. It's the sound of the title theme ([`crate::title`]):
//!
//! - **BRR samples.** Instruments sit in the chip's 64 KB of RAM as 9-byte blocks: a header (a
//!   shift and one of four predictors) and sixteen 4-bit residuals, about 3.5:1. [`Sample::new`]
//!   encodes a waveform as a converter would (the shift and predictor that decode closest, block by
//!   block) and voices play what the chip's own integer decoder makes of it, grit and all.
//! - **Gaussian interpolation.** A voice steps through its sample at `pitch / 0x1000` of 32 kHz and
//!   reads between samples with a 4-point filter from the chip's table: the soft, dark top end.
//! - **ADSR.** A linear attack, exponential decay and sustain and a fixed 8 ms release, stepped at
//!   the chip's rates off one shared counter.
//! - **Echo.** A delay line in that same RAM (16 ms per step), through an 8-tap FIR filter and fed
//!   back, with the chip's wraps and clamps.
//! - Eight voices, mixed in 16 bits, stereo, at 32 kHz.
//!
//! All integer, so a render is the same everywhere. Not here: the SPC700 itself (the score drives
//! the voices a tick at a time, as a sound driver would), noise, pitch modulation, GAIN envelopes.

/// The output rate, Hz.
pub const RATE: u32 = 32_000;
/// The chip's RAM, bytes: the samples, the echo buffer, the driver and the score all share it.
pub const ARAM: usize = 0x1_0000;
/// Voices.
pub const VOICES: usize = 8;

/// A sample in the chip's RAM.
pub struct Sample {
    /// As stored: 9-byte BRR blocks.
    pub brr: Vec<u8>,
    /// As the decoder reads it back (15-bit).
    pcm: Vec<i16>,
    /// Where it loops back to at its end, or `None` to stop there.
    looping: Option<usize>,
}

impl Sample {
    /// Encodes `wave` (15-bit) as BRR. A loop starts on a block (a multiple of 16 samples) and a
    /// looping sample is whole blocks long.
    pub fn new(wave: &[i16], looping: Option<usize>) -> Self {
        if let Some(from) = looping {
            assert!(
                from.is_multiple_of(16) && wave.len().is_multiple_of(16) && from < wave.len(),
                "a loop is whole blocks"
            );
        }
        let brr = encode(wave, looping);
        let pcm = decode(&brr);
        Self { brr, pcm, looping }
    }

    /// Sample `k`: past the end, around the loop, or silence.
    #[inline]
    fn at(&self, k: usize) -> i32 {
        match (self.pcm.get(k), self.looping) {
            (Some(&v), _) => i32::from(v),
            (None, Some(from)) => {
                let len = self.pcm.len();
                i32::from(self.pcm[from + (k - len) % (len - from)])
            }
            (None, None) => 0,
        }
    }
}

/// A BRR predictor's guess from the last two samples: the chip's integer formulas.
fn predict(filter: u8, old: i32, older: i32) -> i32 {
    match filter {
        0 => 0,
        1 => old + ((-old) >> 4),
        2 => 2 * old + ((-3 * old) >> 5) - older + (older >> 4),
        _ => 2 * old + ((-13 * old) >> 6) - older + ((3 * older) >> 4),
    }
}

/// One sample from a 4-bit residual, as the chip decodes it.
fn brr_sample(nibble: i32, shift: u8, filter: u8, old: i32, older: i32) -> i32 {
    // Shifts 13-15 are reserved: they decode as 12 with the residual's sign alone.
    let s = if shift <= 12 { (nibble << shift) >> 1 } else { (nibble >> 3) << 11 };
    // Clamped to 16 bits, then cut to 15: past +-0x4000 a sample loses its sign (the encoder
    // steers clear, since it scores what this returns).
    let v = (s + predict(filter, old, older)).clamp(-0x8000, 0x7FFF);
    i32::from((v << 1) as i16 >> 1)
}

/// One block's best encoding under a shift and predictor.
struct Block {
    err: i64,
    shift: u8,
    filter: u8,
    data: [u8; 8],
    old: i32,
    older: i32,
}

fn encode_block(want: &[i32; 16], shift: u8, filter: u8, mut old: i32, mut older: i32) -> Block {
    let mut data = [0u8; 8];
    let mut err = 0;
    for (k, &w) in want.iter().enumerate() {
        // The residual nearest what the predictor leaves, checked against its neighbours since the
        // clamp and the wrap aren't linear.
        let guess = (2.0 * (w - predict(filter, old, older)) as f32 / (1 << shift) as f32).round() as i32;
        let (mut best, mut nibble, mut got) = (i64::MAX, 0, 0);
        for n in (guess - 1).clamp(-8, 7)..=(guess + 1).clamp(-8, 7) {
            let v = brr_sample(n, shift, filter, old, older);
            let e = i64::from(v - w).pow(2);
            if e < best {
                (best, nibble, got) = (e, n, v);
            }
        }
        err += best;
        data[k / 2] |= ((nibble & 0xF) as u8) << if k % 2 == 0 { 4 } else { 0 };
        (old, older) = (got, old);
    }
    Block { err, shift, filter, data, old, older }
}

/// Encodes as a converter does: every block takes the shift and predictor that decode closest.
/// The first block and the loop's can't lean on the samples before them (the decoder may arrive
/// from anywhere), so they get no predictor.
fn encode(wave: &[i16], looping: Option<usize>) -> Vec<u8> {
    let blocks = wave.len().div_ceil(16);
    let mut brr = Vec::with_capacity(blocks * 9);
    let (mut old, mut older) = (0, 0);
    for b in 0..blocks {
        let want = std::array::from_fn(|k| wave.get(b * 16 + k).map_or(0, |&v| i32::from(v)));
        let filters = if b == 0 || looping == Some(b * 16) { 1 } else { 4 };
        let mut best: Option<Block> = None;
        for filter in 0..filters {
            for shift in 0..=12 {
                let c = encode_block(&want, shift, filter, old, older);
                if best.as_ref().is_none_or(|b| c.err < b.err) {
                    best = Some(c);
                }
            }
        }
        let best = best.expect("a block has candidates");
        let mut header = best.shift << 4 | best.filter << 2;
        if b + 1 == blocks {
            // End, and loop (or mute).
            header |= if looping.is_some() { 0b11 } else { 0b01 };
        }
        brr.push(header);
        brr.extend_from_slice(&best.data);
        (old, older) = (best.old, best.older);
    }
    brr
}

/// What the chip's decoder reads back from `brr`. A loop's first block has no predictor, so the
/// samples decode the same on every pass.
fn decode(brr: &[u8]) -> Vec<i16> {
    let mut pcm = Vec::with_capacity(brr.len() / 9 * 16);
    let (mut old, mut older) = (0, 0);
    for block in brr.as_chunks::<9>().0 {
        let (shift, filter) = (block[0] >> 4, block[0] >> 2 & 3);
        for k in 0..16 {
            let byte = block[1 + k / 2];
            let nibble = if k % 2 == 0 { byte >> 4 } else { byte & 0xF };
            let v = brr_sample(i32::from((nibble << 4) as i8 >> 4), shift, filter, old, older);
            pcm.push(v as i16);
            (old, older) = (v, old);
        }
    }
    pcm
}

/// The chip's interpolation weights (as fullsnes lists them). Each four the filter uses together
/// sum to 0x7FF..0x801: very nearly unity.
#[rustfmt::skip]
const GAUSS: [i32; 512] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2,
    2, 2, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 5, 5, 5, 5,
    6, 6, 6, 6, 7, 7, 7, 8, 8, 8, 9, 9, 9, 10, 10, 10,
    11, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15, 15, 16, 16, 17, 17,
    18, 19, 19, 20, 20, 21, 21, 22, 23, 23, 24, 24, 25, 26, 27, 27,
    28, 29, 29, 30, 31, 32, 32, 33, 34, 35, 36, 36, 37, 38, 39, 40,
    41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56,
    58, 59, 60, 61, 62, 64, 65, 66, 67, 69, 70, 71, 73, 74, 76, 77,
    78, 80, 81, 83, 84, 86, 87, 89, 90, 92, 94, 95, 97, 99, 100, 102,
    104, 106, 107, 109, 111, 113, 115, 117, 118, 120, 122, 124, 126, 128, 130, 132,
    134, 137, 139, 141, 143, 145, 147, 150, 152, 154, 156, 159, 161, 163, 166, 168,
    171, 173, 175, 178, 180, 183, 186, 188, 191, 193, 196, 199, 201, 204, 207, 210,
    212, 215, 218, 221, 224, 227, 230, 233, 236, 239, 242, 245, 248, 251, 254, 257,
    260, 263, 267, 270, 273, 276, 280, 283, 286, 290, 293, 297, 300, 304, 307, 311,
    314, 318, 321, 325, 328, 332, 336, 339, 343, 347, 351, 354, 358, 362, 366, 370,
    374, 378, 381, 385, 389, 393, 397, 401, 405, 410, 414, 418, 422, 426, 430, 434,
    439, 443, 447, 451, 456, 460, 464, 469, 473, 477, 482, 486, 491, 495, 499, 504,
    508, 513, 517, 522, 527, 531, 536, 540, 545, 550, 554, 559, 563, 568, 573, 577,
    582, 587, 592, 596, 601, 606, 611, 615, 620, 625, 630, 635, 640, 644, 649, 654,
    659, 664, 669, 674, 678, 683, 688, 693, 698, 703, 708, 713, 718, 723, 728, 732,
    737, 742, 747, 752, 757, 762, 767, 772, 777, 782, 787, 792, 797, 802, 806, 811,
    816, 821, 826, 831, 836, 841, 846, 851, 855, 860, 865, 870, 875, 880, 884, 889,
    894, 899, 904, 908, 913, 918, 923, 927, 932, 937, 941, 946, 951, 955, 960, 965,
    969, 974, 978, 983, 988, 992, 997, 1001, 1005, 1010, 1014, 1019, 1023, 1027, 1032, 1036,
    1040, 1045, 1049, 1053, 1057, 1061, 1066, 1070, 1074, 1078, 1082, 1086, 1090, 1094, 1098, 1102,
    1106, 1109, 1113, 1117, 1121, 1125, 1128, 1132, 1136, 1139, 1143, 1146, 1150, 1153, 1157, 1160,
    1164, 1167, 1170, 1174, 1177, 1180, 1183, 1186, 1190, 1193, 1196, 1199, 1202, 1205, 1207, 1210,
    1213, 1216, 1219, 1221, 1224, 1227, 1229, 1232, 1234, 1237, 1239, 1241, 1244, 1246, 1248, 1251,
    1253, 1255, 1257, 1259, 1261, 1263, 1265, 1267, 1269, 1270, 1272, 1274, 1275, 1277, 1279, 1280,
    1282, 1283, 1284, 1286, 1287, 1288, 1290, 1291, 1292, 1293, 1294, 1295, 1296, 1297, 1297, 1298,
    1299, 1300, 1300, 1301, 1302, 1302, 1303, 1303, 1303, 1304, 1304, 1304, 1304, 1304, 1305, 1305,
];

/// Reads between `s[1]` and `s[2]` (oldest to newest), `i`/256 of the way: the first three
/// products summed with a 16-bit wrap and the last with a clamp, as the chip does.
#[inline]
fn gauss(i: usize, s: [i32; 4]) -> i32 {
    let mut out = (GAUSS[0xFF - i] * s[0]) >> 10;
    out += (GAUSS[0x1FF - i] * s[1]) >> 10;
    out += (GAUSS[0x100 + i] * s[2]) >> 10;
    out = i32::from(out as i16);
    out += (GAUSS[i] * s[3]) >> 10;
    out.clamp(-0x8000, 0x7FFF) >> 1
}

/// A voice's ADSR registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Adsr {
    /// 0..=15: the linear rise, 4 s (0) to instant (15).
    pub attack: u8,
    /// 0..=7: the exponential fall to the sustain level.
    pub decay: u8,
    /// 0..=7: the decay stops at (n + 1) / 8.
    pub level: u8,
    /// 0..=31: the exponential fall while held (0 holds).
    pub sustain: u8,
}

pub const fn adsr(attack: u8, decay: u8, level: u8, sustain: u8) -> Adsr {
    Adsr { attack, decay, level, sustain }
}

/// Samples between envelope steps, by rate (0 never steps).
const PERIODS: [u32; 32] = [
    0, 2048, 1536, 1280, 1024, 768, 640, 512, 384, 320, 256, 192, 160, 128, 96, 80, 64, 48, 40, 32, 24, 20,
    16, 12, 10, 8, 6, 5, 4, 3, 2, 1,
];
/// Where each rate's steps fall on the shared counter.
const OFFSETS: [u32; 32] = [
    0, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0, 1040, 536, 0,
    1040, 536, 0, 1040, 536, 0, 1040, 0, 0,
];
/// The shared counter's period (every rate's divides it).
const COUNTER: u32 = 30_720;

/// Whether an envelope at `rate` steps on this sample.
#[inline]
fn steps(rate: usize, counter: u32) -> bool {
    rate != 0 && (counter + OFFSETS[rate]).is_multiple_of(PERIODS[rate])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy)]
struct Voice {
    sample: usize,
    /// The oldest of the four samples being read, and how far past it (12 bits).
    pos: usize,
    frac: u32,
    /// 0x1000 plays the sample at 32 kHz.
    pitch: u32,
    /// 0..=0x7FF.
    env: i32,
    phase: Phase,
    adsr: Adsr,
    /// VxVOLL, VxVOLR.
    vol: [i32; 2],
    /// Sent to the echo too (EON).
    echo: bool,
    /// Sounding: keyed on, and not released to silence or past a sample's end.
    on: bool,
}

impl Voice {
    const IDLE: Voice = Voice {
        sample: 0,
        pos: 0,
        frac: 0,
        pitch: 0x1000,
        env: 0,
        phase: Phase::Release,
        adsr: adsr(15, 0, 7, 0),
        vol: [0; 2],
        echo: false,
        on: false,
    };

    /// This sample's output (15-bit, enveloped), then a step on through the sample.
    #[inline]
    fn run(&mut self, bank: &[Sample], counter: u32) -> i32 {
        if !self.on {
            return 0;
        }
        let s = &bank[self.sample];
        let taps = [s.at(self.pos), s.at(self.pos + 1), s.at(self.pos + 2), s.at(self.pos + 3)];
        let out = gauss((self.frac >> 4) as usize, taps);
        self.envelope(counter);
        let out = (out * self.env) >> 11;
        self.frac += self.pitch;
        self.pos += (self.frac >> 12) as usize;
        self.frac &= 0xFFF;
        let len = s.pcm.len();
        match s.looping {
            Some(from) if self.pos >= len => self.pos -= len - from,
            // A sample's end block without a loop mutes the voice.
            None if self.pos + 3 >= len => self.on = false,
            _ => {}
        }
        if self.phase == Phase::Release && self.env == 0 {
            self.on = false;
        }
        out
    }

    /// One envelope step, as the chip takes it: the phase moves on the level it would reach even
    /// when its rate doesn't step this sample.
    #[inline]
    fn envelope(&mut self, counter: u32) {
        if self.phase == Phase::Release {
            self.env = (self.env - 8).max(0);
            return;
        }
        let mut env = self.env;
        let rate = if self.phase == Phase::Attack {
            let rate = usize::from(self.adsr.attack) * 2 + 1;
            env += if rate < 31 { 0x20 } else { 0x400 };
            rate
        } else {
            env -= 1;
            env -= env >> 8;
            if self.phase == Phase::Decay {
                usize::from(self.adsr.decay) * 2 + 16
            } else {
                usize::from(self.adsr.sustain)
            }
        };
        if self.phase == Phase::Decay && env >> 8 == i32::from(self.adsr.level) {
            self.phase = Phase::Sustain;
        }
        if !(0..=0x7FF).contains(&env) {
            env = env.clamp(0, 0x7FF);
            if self.phase == Phase::Attack {
                self.phase = Phase::Decay;
            }
        }
        if steps(rate, counter) {
            self.env = env;
        }
    }
}

/// The echo's registers.
#[derive(Clone, Copy, Debug)]
pub struct Echo {
    /// EDL: the delay, 16 ms a step (0..=15).
    pub delay: u8,
    /// EFB: how much of the echo feeds back (/128).
    pub feedback: i8,
    /// EVOLL, EVOLR: its level in the output (/128).
    pub volume: [i8; 2],
    /// FIR0..FIR7, the oldest sample's first (/128, summing to about 128 for unity).
    pub fir: [i8; 8],
}

impl Echo {
    /// Its buffer's share of the chip's RAM, bytes.
    pub fn ram(&self) -> usize {
        (usize::from(self.delay) * 2048).max(4)
    }
}

/// The chip: eight voices, the mixer and the echo.
pub struct Dsp<'a> {
    bank: &'a [Sample],
    voices: [Voice; VOICES],
    /// MVOLL, MVOLR.
    master: [i32; 2],
    echo: Echo,
    counter: u32,
    /// The echo buffer, and where it's read and written this sample.
    ring: Vec<[i32; 2]>,
    ring_at: usize,
    /// The FIR's last eight inputs, and the newest's place.
    fir: [[i32; 2]; 8],
    fir_at: usize,
    /// Samples where the voices' 16-bit mix clamped (a score mixed too hot).
    pub clipped: u32,
}

impl<'a> Dsp<'a> {
    pub fn new(bank: &'a [Sample], master: i8, echo: Echo) -> Self {
        Self {
            bank,
            voices: [Voice::IDLE; VOICES],
            master: [i32::from(master); 2],
            echo,
            counter: 0,
            ring: vec![[0; 2]; echo.ram() / 4],
            ring_at: 0,
            fir: [[0; 2]; 8],
            fir_at: 0,
            clipped: 0,
        }
    }

    /// KON: plays `sample` from the top, the envelope from silence.
    pub fn key_on(&mut self, v: usize, sample: usize, adsr: Adsr) {
        let voice = &mut self.voices[v];
        *voice = Voice { sample, pos: 0, frac: 0, env: 0, phase: Phase::Attack, adsr, on: true, ..*voice };
    }

    /// KOFF: the release.
    pub fn key_off(&mut self, v: usize) {
        self.voices[v].phase = Phase::Release;
    }

    /// 0x1000 plays a sample at 32 kHz (the register holds 14 bits).
    pub fn set_pitch(&mut self, v: usize, pitch: u32) {
        self.voices[v].pitch = pitch.min(0x3FFF);
    }

    pub fn set_volume(&mut self, v: usize, left: i8, right: i8) {
        self.voices[v].vol = [i32::from(left), i32::from(right)];
    }

    pub fn set_echo(&mut self, v: usize, on: bool) {
        self.voices[v].echo = on;
    }

    /// The next sample, left and right.
    pub fn frame(&mut self) -> [i16; 2] {
        self.counter = self.counter.checked_sub(1).unwrap_or(COUNTER - 1);
        let (mut main, mut wet) = ([0i32; 2], [0i32; 2]);
        for v in &mut self.voices {
            let out = v.run(self.bank, self.counter);
            for ch in 0..2 {
                let amp = (out * v.vol[ch]) >> 6;
                let sum = main[ch] + amp;
                main[ch] = sum.clamp(-0x8000, 0x7FFF);
                self.clipped += u32::from(main[ch] != sum);
                if v.echo {
                    wet[ch] = (wet[ch] + amp).clamp(-0x8000, 0x7FFF);
                }
            }
        }
        // The echo: the oldest entry leaves the ring for the FIR...
        self.fir_at = (self.fir_at + 1) % 8;
        for ch in 0..2 {
            self.fir[self.fir_at][ch] = self.ring[self.ring_at][ch] >> 1;
        }
        let mut out = [0i16; 2];
        for ch in 0..2 {
            let tap = |t: usize| (self.fir[(self.fir_at + 1 + t) % 8][ch] * i32::from(self.echo.fir[t])) >> 6;
            let sum = i32::from((0..7).map(tap).sum::<i32>() as i16) + tap(7);
            let echo = sum.clamp(-0x8000, 0x7FFF) & !1;
            // ...joins the voices on the way out...
            let dry = i32::from(((main[ch] * self.master[ch]) >> 7) as i16);
            let echoed = i32::from(((echo * i32::from(self.echo.volume[ch])) >> 7) as i16);
            out[ch] = (dry + echoed).clamp(-0x8000, 0x7FFF) as i16;
            // ...and goes back into the ring with the voices sent to it.
            let fed = i32::from(((echo * i32::from(self.echo.feedback)) >> 7) as i16);
            self.ring[self.ring_at][ch] = (wet[ch] + fed).clamp(-0x8000, 0x7FFF) & !1;
        }
        self.ring_at = (self.ring_at + 1) % self.ring.len();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRY: Echo = Echo { delay: 0, feedback: 0, volume: [0; 2], fir: [0; 8] };

    #[test]
    fn brr_decodes_as_the_chip_does() {
        // Shift 12, no predictor: the residual scaled to 15 bits.
        assert_eq!(brr_sample(7, 12, 0, 0, 0), 7 << 11);
        assert_eq!(brr_sample(-8, 12, 0, 0, 0), -8 << 11);
        // Shift 0 drops the low bit.
        assert_eq!(brr_sample(3, 0, 0, 0, 0), 1);
        // Reserved shifts keep only the residual's sign.
        assert_eq!(brr_sample(7, 13, 0, 0, 0), 0);
        assert_eq!(brr_sample(-1, 15, 0, 0, 0), -2048);
        // Predictor 1 is 15/16 of the last sample, rounding down.
        assert_eq!(brr_sample(0, 0, 1, 1000, 0), 1000 - 63);
        // Past 15 bits a sample loses its sign: the documented glitch.
        assert!(brr_sample(7, 12, 1, 0x3000, 0) < 0);
    }

    #[test]
    fn brr_keeps_a_waveform_and_its_loop() {
        // A tone sweeping up: something the predictors have to work for.
        let wave: Vec<i16> =
            (0..1_024).map(|i| ((i as f32 * (0.02 + i as f32 * 1e-4)).sin() * 12_000.0) as i16).collect();
        let s = Sample::new(&wave, Some(512));
        assert_eq!(s.brr.len(), 64 * 9);
        let (mut signal, mut noise) = (0.0f64, 0.0f64);
        for (w, d) in wave.iter().zip(&s.pcm) {
            signal += f64::from(*w).powi(2);
            noise += f64::from(w - d).powi(2);
        }
        let snr = 10.0 * (signal / noise).log10();
        assert!(snr > 30.0, "BRR keeps {snr:.1} dB");
        // The first block and the loop's use no predictor; the last carries end and loop.
        assert_eq!(s.brr[0] >> 2 & 3, 0);
        assert_eq!(s.brr[32 * 9] >> 2 & 3, 0);
        assert_eq!(s.brr[63 * 9] & 3, 0b11);
        // Past the end, the loop.
        assert_eq!(s.at(1_024), s.at(512));
        assert_eq!(s.at(1_024 + 511), s.at(1_023));
    }

    #[test]
    fn the_gaussian_table_is_nearly_unity_everywhere() {
        for i in 0..256 {
            let sum = GAUSS[i] + GAUSS[0xFF - i] + GAUSS[0x100 + i] + GAUSS[0x1FF - i];
            assert!((0x7FF..=0x801).contains(&sum), "{i}: {sum:#x}");
        }
        assert!(GAUSS.windows(2).all(|w| w[0] <= w[1]));
        // A flat signal reads back flat.
        assert!((gauss(0, [8_000; 4]) - 8_000).abs() <= 8);
        assert!((gauss(128, [8_000; 4]) - 8_000).abs() <= 8);
    }

    /// A voice on a constant sample: its output is its envelope.
    fn envelope_of(a: Adsr, off_at: usize, n: usize) -> Vec<i32> {
        let bank = [Sample::new(&[0x3FF0; 64], Some(0))];
        let mut dsp = Dsp::new(&bank, 127, DRY);
        dsp.key_on(0, 0, a);
        (0..n)
            .map(|k| {
                if k == off_at {
                    dsp.key_off(0);
                }
                let _ = dsp.frame();
                dsp.voices[0].env
            })
            .collect()
    }

    #[test]
    fn the_envelope_keeps_the_chips_time() {
        // Attack 10: 32 a step every 20 samples, 64 steps to the top.
        let e = envelope_of(adsr(10, 0, 7, 0), usize::MAX, 2_000);
        let top = e.iter().position(|&v| v >= 0x7E0).expect("reaches the top");
        assert!((1_240..=1_300).contains(&top), "{top}");
        // Held at level 7 with no sustain rate: it stays up.
        assert!(e[1_999] >= 0x700);
        // Instant attack, then a decay to level 3's boundary (a half) that holds there.
        let e = envelope_of(adsr(15, 7, 3, 0), usize::MAX, 3_000);
        assert!(e[4] >= 0x7E0);
        assert!((0x3F0..=0x410).contains(&e[2_999]), "{:#x}", e[2_999]);
        // Release: 8 a sample, from the top to silence in 256.
        let e = envelope_of(adsr(15, 0, 7, 0), 100, 400);
        assert_eq!(e[99] - e[100], 8);
        assert_eq!(e[100 + 256], 0);
    }

    #[test]
    fn pitch_0x1000_plays_a_sample_at_32_khz() {
        // A 32-sample cycle: 1 kHz at 0x1000, 1.5 kHz at 0x1800.
        let cycle: Vec<i16> =
            (0..32).map(|i| ((i as f32 / 32.0 * std::f32::consts::TAU).sin() * 8_000.0) as i16).collect();
        let bank = [Sample::new(&cycle, Some(0))];
        for (pitch, hz) in [(0x1000, 1_000), (0x1800, 1_500)] {
            let mut dsp = Dsp::new(&bank, 127, DRY);
            dsp.set_volume(0, 127, 127);
            dsp.set_pitch(0, pitch);
            dsp.key_on(0, 0, adsr(15, 0, 7, 0));
            let out: Vec<i16> = (0..RATE).map(|_| dsp.frame()[0]).collect();
            let rises = out.windows(2).filter(|w| w[0] < 0 && w[1] >= 0).count();
            assert!(rises.abs_diff(hz) <= 2, "{pitch:#x}: {rises} Hz");
        }
    }

    #[test]
    fn the_echo_comes_back_after_its_delay() {
        // A click, echoed: EDL 4 is 64 ms (2048 samples), and FIR0 reads the oldest of the eight.
        let mut click = vec![0i16; 64];
        click[8] = 0x3000;
        let bank = [Sample::new(&click, None)];
        let echo = Echo { delay: 4, feedback: 0, volume: [127, 127], fir: [127, 0, 0, 0, 0, 0, 0, 0] };
        let mut dsp = Dsp::new(&bank, 0, echo);
        dsp.set_volume(0, 127, 127);
        dsp.set_echo(0, true);
        dsp.key_on(0, 0, adsr(15, 0, 7, 0));
        let out: Vec<i32> = (0..4_000).map(|_| i32::from(dsp.frame()[0]).abs()).collect();
        let loudest = (0..out.len()).max_by_key(|&k| out[k]).unwrap();
        assert!((2_048 + 7..2_048 + 7 + 16).contains(&loudest), "{loudest}");
        // Master volume 0: nothing but the echo.
        assert!(out[..2_048].iter().all(|&v| v == 0));
    }
}
