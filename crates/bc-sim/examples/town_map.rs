//! A plan of the ring's town (`colony::town`), drawn from above as a PPM image on stdout: the ring
//! unrolled, `t` (round from Hub Gate) left to right and `x` (fore to aft) top to bottom. Roads dark,
//! pavements pale, the canal blue, parks green, the squares paler, the key places' sites in their
//! colours, and buildings shaded by height in their quarter's colour.
//!
//! `cargo run -q -p bc-sim --example town_map -- [m_per_px] [t0] [t1] > town.ppm`
//! (defaults: 8 m a pixel, the whole ring).

#![allow(clippy::disallowed_types, clippy::disallowed_methods, clippy::disallowed_macros)]

use bc_sim::colony::town::{BlockUse, Ground, MAX_HEIGHT, ROUND, TownBox, block_use, each_box, ground_at};
use bc_sim::content::town::{PlaceKind, Quarter};
use bc_sim::world::COLONY_HALF_WIDTH;
use std::io::Write;

fn main() {
    let args: Vec<f32> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();
    let step = args.first().copied().unwrap_or(8.0);
    let t0 = args.get(1).copied().unwrap_or(0.0);
    let t1 = args.get(2).copied().unwrap_or(ROUND);
    let (w, h) = (((t1 - t0) / step) as usize, (2.0 * COLONY_HALF_WIDTH / step) as usize);
    let mut boxes: Vec<TownBox> = Vec::new();
    each_box(-COLONY_HALF_WIDTH, COLONY_HALF_WIDTH, t0, t1, &mut |b| boxes.push(b));
    let mut img = vec![[0u8; 3]; w * h];
    for j in 0..h {
        let x = -COLONY_HALF_WIDTH + (j as f32 + 0.5) * step;
        for i in 0..w {
            let t = t0 + (i as f32 + 0.5) * step;
            let g = ground_at(x, t);
            let mut c = match g {
                Ground::Road => [70, 72, 78],
                Ground::Median => [120, 110, 70],
                Ground::Pavement => [175, 178, 185],
                Ground::Water => [60, 120, 170],
                Ground::RimWalk => [110, 150, 100],
                Ground::Outside => [0, 0, 0],
                Ground::Block(id) => match block_use(id) {
                    BlockUse::Park => [90, 150, 80],
                    BlockUse::Square => [215, 215, 205],
                    BlockUse::Place(PlaceKind::WatchStation) => [40, 60, 160],
                    BlockUse::Place(_) => [220, 170, 40],
                    BlockUse::Frames => [190, 175, 120],
                    BlockUse::Built => [150, 152, 158],
                },
            };
            let top = boxes
                .iter()
                .filter(|b| b.x0 <= x && x < b.x1 && b.t0 <= t && t < b.t1)
                .map(|b| b.h1)
                .fold(0.0f32, f32::max);
            if top > 0.0 {
                let tint = match g {
                    Ground::Block(id) => match bc_sim::colony::town::district(id.district()).quarter {
                        Quarter::Charter => [0.55, 0.65, 0.95],
                        Quarter::Canal => [0.45, 0.85, 0.85],
                        Quarter::Gardens => [0.6, 0.9, 0.55],
                        Quarter::Site => [0.95, 0.85, 0.4],
                    },
                    _ => [0.9, 0.9, 0.9],
                };
                let v = 0.35 + 0.65 * (top / MAX_HEIGHT).min(1.0);
                c = tint.map(|k| (255.0 * k * v) as u8);
            }
            img[j * w + i] = c;
        }
    }
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    write!(out, "P6\n{w} {h}\n255\n").unwrap();
    for p in img {
        out.write_all(&p).unwrap();
    }
}
