//! The ZERO System's predicted futures: for each tracked threat, every maneuver hypothesis drawn as
//! a ghost trail 1.5 s ahead, opacity proportional to its probability. The client re-runs the
//! server's rollout code from its own view; the server only sends the probabilities.

use bevy::prelude::*;

use crate::dev_hooks::DevStatus;
use crate::net::{GameClient, now_s};

pub fn draw_ghosts(game: NonSend<GameClient>, mut gizmos: Gizmos, mut dev: ResMut<DevStatus>) {
    let game = game.borrow();
    let core = &game.core;
    let t = core.render_tick(now_s());
    let ghosts = core.world.zero_ghosts(t);
    let mut drawn = 0u32;
    for g in &ghosts {
        let Some(pose) = core.world.pose(g.slot, t) else { continue };
        let mut best = 0;
        for k in 1..g.probs.len() {
            if g.probs[k] > g.probs[best] {
                best = k;
            }
        }
        for (k, path) in g.paths.iter().enumerate() {
            let p = g.probs[k];
            if p < 0.04 {
                continue;
            }
            let color = if k == best {
                Color::srgba(1.0, 0.35, 0.8, 0.35 + 0.65 * p)
            } else {
                Color::srgba(0.45, 0.9, 1.0, 0.15 + 0.7 * p)
            };
            gizmos.linestrip(std::iter::once(pose.pos).chain(path.iter().copied()), color);
            if k == best {
                gizmos.sphere(Isometry3d::from_translation(path[path.len() - 1]), 6.0, color);
            }
            drawn += 1;
        }
    }
    // The firing solution: a line from the own suit along the ZERO aim.
    if let (Some(z), Some(own)) = (core.world.zero, core.world.own)
        && z.has_solution
        && own.alive
    {
        let from = core.predict.render_pos();
        gizmos.line(from, from + z.solution * 900.0, Color::srgba(1.0, 0.85, 0.3, 0.5));
    }
    dev.set("zero_ghosts", drawn);
}
