//! Spatial hash for broad-phase queries, rebuilt from scratch every tick with a counting sort
//! (O(n + buckets), no allocation).

use alloc::boxed::Box;
use glam::Vec3;

use crate::storage::boxed;

const BUCKETS: usize = 4096;
/// Cell edge, m.
pub const CELL: f32 = 128.0;

pub struct SpatialHash {
    bucket_start: Box<[u32]>,
    bucket_fill: Box<[u32]>,
    items: Box<[u16]>,
    item_bucket: Box<[u32]>,
    stamp: Box<[u32]>,
    stamp_id: u32,
    n: usize,
}

#[inline]
fn cell_of(p: Vec3) -> (i32, i32, i32) {
    let inv = 1.0 / CELL;
    (
        crate::math::floor(p.x * inv) as i32,
        crate::math::floor(p.y * inv) as i32,
        crate::math::floor(p.z * inv) as i32,
    )
}

#[inline]
fn bucket(x: i32, y: i32, z: i32) -> usize {
    let h = (x as u32).wrapping_mul(73_856_093)
        ^ (y as u32).wrapping_mul(19_349_663)
        ^ (z as u32).wrapping_mul(83_492_791);
    h as usize & (BUCKETS - 1)
}

impl SpatialHash {
    pub fn new(max_items: usize) -> Self {
        Self {
            bucket_start: boxed(BUCKETS + 1, 0u32),
            bucket_fill: boxed(BUCKETS, 0u32),
            items: boxed(max_items, 0u16),
            item_bucket: boxed(max_items, u32::MAX),
            stamp: boxed(max_items, 0u32),
            stamp_id: 0,
            n: 0,
        }
    }

    /// Rebuilds from `(index, position)` pairs.
    pub fn build(&mut self, entries: impl Iterator<Item = (usize, Vec3)> + Clone) {
        self.bucket_fill.fill(0);
        self.item_bucket.fill(u32::MAX);
        for (i, p) in entries.clone() {
            let (x, y, z) = cell_of(p);
            let b = bucket(x, y, z);
            self.item_bucket[i] = b as u32;
            self.bucket_fill[b] += 1;
        }
        let mut acc = 0u32;
        for b in 0..BUCKETS {
            self.bucket_start[b] = acc;
            acc += self.bucket_fill[b];
            self.bucket_fill[b] = self.bucket_start[b];
        }
        self.bucket_start[BUCKETS] = acc;
        self.n = acc as usize;
        for (i, _) in entries {
            let b = self.item_bucket[i] as usize;
            let slot = self.bucket_fill[b] as usize;
            self.items[slot] = i as u16;
            self.bucket_fill[b] += 1;
        }
    }

    /// Calls `f` once for every item whose cell overlaps the box `[min, max]`.
    pub fn query_box(&mut self, min: Vec3, max: Vec3, mut f: impl FnMut(usize)) {
        self.stamp_id = self.stamp_id.wrapping_add(1);
        if self.stamp_id == 0 {
            self.stamp.fill(0);
            self.stamp_id = 1;
        }
        let (x0, y0, z0) = cell_of(min);
        let (x1, y1, z1) = cell_of(max);
        // Huge boxes: every bucket is visited once instead.
        let cells = (x1 - x0 + 1) as i64 * (y1 - y0 + 1) as i64 * (z1 - z0 + 1) as i64;
        if cells > BUCKETS as i64 {
            for k in 0..self.n {
                let i = self.items[k] as usize;
                if self.stamp[i] != self.stamp_id {
                    self.stamp[i] = self.stamp_id;
                    f(i);
                }
            }
            return;
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    let b = bucket(x, y, z);
                    for k in self.bucket_start[b]..self.bucket_start[b + 1] {
                        let i = self.items[k as usize] as usize;
                        if self.stamp[i] != self.stamp_id {
                            self.stamp[i] = self.stamp_id;
                            f(i);
                        }
                    }
                }
            }
        }
    }

    /// Items whose cells overlap a sphere's bounding box (callers do the exact test).
    pub fn query_sphere(&mut self, c: Vec3, r: f32, f: impl FnMut(usize)) {
        let e = Vec3::splat(r);
        self.query_box(c - e, c + e, f);
    }
}
