//! Making things takes time. Each station runs its jobs one at a time, in order, on the wall clock
//! (Unix seconds), so a job keeps running while its pilot flies, or sleeps. A job's inputs (and
//! the foundry's fees) are taken when it's queued; each batch's output lands in the stores as it
//! finishes. Cancelling gives back what the batches not yet made would have used.

use serde::{Deserialize, Serialize};

use crate::catalogue::{Recipe, Station, recipe};
use crate::item::Item;

/// The most jobs a station holds.
pub const MAX_JOBS: usize = 8;

/// A job: `batches` of a recipe (named by what it makes).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub recipe: Item,
    pub batches: u32,
    /// Batches finished (and delivered).
    pub done: u32,
    /// Seconds a batch takes (the recipe's, at the server's craft speed when it was queued).
    pub secs: u32,
    /// When the batch under way started (the job at the head of the queue only).
    pub started: u64,
    /// The share of the recipe's fee paid for it, percent (the second foundry's discount).
    #[serde(default = "full_fee")]
    pub fee_pct: u64,
}

fn full_fee() -> u64 {
    100
}

impl Job {
    pub fn recipe(&self) -> Option<&'static Recipe> {
        recipe(self.recipe)
    }

    /// Seconds until the whole job is done, if it's at the head of the queue at `now`.
    pub fn secs_left(&self, now: u64) -> u64 {
        let batch_left = (self.started + u64::from(self.secs)).saturating_sub(now);
        batch_left + u64::from(self.batches.saturating_sub(self.done + 1)) * u64::from(self.secs)
    }
}

/// A station's jobs, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queue {
    pub jobs: Vec<Job>,
}

impl Queue {
    pub fn is_full(&self) -> bool {
        self.jobs.len() >= MAX_JOBS
    }

    /// Adds a job (its inputs already taken). It starts at `now` if nothing's ahead of it.
    pub fn push(&mut self, recipe: Item, batches: u32, secs: u32, now: u64, fee_pct: u64) {
        let started = if self.jobs.is_empty() { now } else { 0 };
        self.jobs.push(Job { recipe, batches, done: 0, secs: secs.max(1), started, fee_pct });
    }

    /// Runs the clock to `now`: every batch finished by then, as (what, how much), in order.
    pub fn settle(&mut self, now: u64) -> Vec<(Item, u64)> {
        let mut out = Vec::new();
        while let Some(job) = self.jobs.first_mut() {
            let Some(r) = recipe(job.recipe) else {
                self.jobs.remove(0);
                continue;
            };
            while job.done < job.batches && job.started + u64::from(job.secs) <= now {
                job.started += u64::from(job.secs);
                job.done += 1;
                match out.last_mut() {
                    Some((item, qty)) if *item == r.output => *qty += r.makes,
                    _ => out.push((r.output, r.makes)),
                }
            }
            if job.done < job.batches {
                break;
            }
            // The next job starts when this one finished.
            let finished = job.started;
            self.jobs.remove(0);
            if let Some(next) = self.jobs.first_mut() {
                next.started = finished;
            }
        }
        out
    }

    /// Takes job `index` off the queue (settle first): what its unmade batches would have used,
    /// and their fees.
    pub fn cancel(&mut self, index: usize, now: u64) -> Option<(Vec<(Item, u64)>, u64)> {
        if index >= self.jobs.len() {
            return None;
        }
        let job = self.jobs.remove(index);
        if index == 0
            && let Some(next) = self.jobs.first_mut()
        {
            next.started = now;
        }
        let left = u64::from(job.batches.saturating_sub(job.done));
        let r = job.recipe()?;
        let refund = r.inputs.iter().map(|(i, q)| (*i, q * left)).collect();
        Some((refund, r.fee * left * job.fee_pct / 100))
    }

    /// Seconds until everything queued is done.
    pub fn secs_left(&self, now: u64) -> u64 {
        let mut total = 0;
        for (k, job) in self.jobs.iter().enumerate() {
            total += if k == 0 {
                job.secs_left(now)
            } else {
                u64::from(job.batches.saturating_sub(job.done)) * u64::from(job.secs)
            };
        }
        total
    }
}

/// A pilot's two stations.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Works {
    pub fabricator: Queue,
    pub foundry: Queue,
}

impl Works {
    pub fn queue(&mut self, station: Station) -> &mut Queue {
        match station {
            Station::Fabricator => &mut self.fabricator,
            Station::Foundry => &mut self.foundry,
        }
    }

    pub fn get(&self, station: Station) -> &Queue {
        match station {
            Station::Fabricator => &self.fabricator,
            Station::Foundry => &self.foundry,
        }
    }

    /// Both stations' deliveries up to `now`.
    pub fn settle(&mut self, now: u64) -> Vec<(Item, u64)> {
        let mut out = self.fabricator.settle(now);
        out.extend(self.foundry.settle(now));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Material, Ore};

    const STEEL: Item = Item::Material(Material::Steel);
    const PROPELLANT: Item = Item::Material(Material::Propellant);

    #[test]
    fn jobs_run_in_order_on_the_clock_and_deliver_batch_by_batch() {
        let mut q = Queue::default();
        q.push(STEEL, 3, 20, 1_000, 100);
        q.push(PROPELLANT, 2, 15, 1_005, 100);
        assert_eq!(q.secs_left(1_000), 3 * 20 + 2 * 15);
        assert_eq!(q.settle(1_019), []);
        assert_eq!(q.settle(1_020), [(STEEL, 80)]);
        // Two more batches of steel, then the propellant starts at 1060 (not when it was queued).
        assert_eq!(q.settle(1_074), [(STEEL, 160)]);
        assert_eq!(q.jobs.len(), 1);
        assert_eq!(q.jobs[0].started, 1_060);
        assert_eq!(q.settle(1_075), [(PROPELLANT, 100)]);
        // Long after: everything, once.
        assert_eq!(q.settle(9_999), [(PROPELLANT, 100)]);
        assert!(q.jobs.is_empty());
        assert_eq!(q.settle(99_999), []);
    }

    #[test]
    fn cancelling_refunds_the_batches_not_made() {
        let mut q = Queue::default();
        q.push(STEEL, 4, 20, 0, 100);
        q.push(PROPELLANT, 1, 15, 0, 100);
        assert_eq!(q.settle(45), [(STEEL, 160)]);
        let (refund, fee) = q.cancel(0, 45).unwrap();
        assert_eq!(refund, [(Item::Ore(Ore::NickelIron), 200)]);
        assert_eq!(fee, 0);
        // The propellant starts now.
        assert_eq!(q.jobs[0].started, 45);
        assert_eq!(q.settle(60), [(PROPELLANT, 100)]);
        assert!(q.cancel(0, 60).is_none());
    }

    #[test]
    fn the_foundry_refunds_its_fees() {
        let mut q = Queue::default();
        let gundanium = Item::Material(Material::Gundanium);
        q.push(gundanium, 3, 180, 0, 100);
        assert_eq!(q.settle(200), [(gundanium, 100)]);
        let (_, fee) = q.cancel(0, 200).unwrap();
        assert_eq!(fee, 2 * recipe(gundanium).unwrap().fee);
    }
}
