import type { Page } from "@playwright/test";

// What the perf recorders share (perf.spec.ts, town.spec.ts): the page's frame times, and the town's
// numbers from `window.__bc` (`town.rs`'s `publish_stats`) sampled in the page.

/** An init script: every frame's time, ms, from one `requestAnimationFrame` to the next (the game
 *  draws once a callback), into `window.__frameMs`. */
export const frameClock = () => {
  const w = window as any;
  w.__frameMs = [];
  let last: number | undefined;
  const tick = (now: number) => {
    if (last !== undefined) w.__frameMs.push(now - last);
    last = now;
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
};

/** The town's numbers as the page has them, at one moment. */
export type TownSample = {
  at: number;
  frames: number;
  place: string;
  fps: number;
  town_cells: number;
  town_tris: number;
  town_ms: number;
  town_life_ms: number;
  town_people: number;
  town_cars: number;
  town_draws: number;
  town_missing: number;
  town_cached: number;
  town_speed: number;
};

/** In the page (an init script, or evaluated): samples `window.__bc`'s town numbers four times a
 *  second into `window.__townSamples`, afresh. On a page drawing seconds a frame the timer only
 *  gets in between frames, so that's a sample a frame. */
export const townSampling = () => {
  const w = window as any;
  clearInterval(w.__townTimer);
  w.__townSamples = [];
  w.__sampledSince = Date.now();
  w.__townTimer = setInterval(() => {
    const s = w.__bc ?? {};
    w.__townSamples.push({
      at: Date.now(),
      frames: s.frames ?? 0,
      place: s.place ?? "",
      fps: s.fps ?? 0,
      town_cells: s.town_cells ?? 0,
      town_tris: s.town_tris ?? 0,
      town_ms: s.town_ms ?? 0,
      town_life_ms: s.town_life_ms ?? 0,
      town_people: s.town_people ?? 0,
      town_cars: s.town_cars ?? 0,
      town_draws: s.town_draws ?? 0,
      town_missing: s.town_missing ?? 0,
      town_cached: s.town_cached ?? 0,
      town_speed: s.town_speed ?? 0,
    });
  }, 250);
};

/** Samples the town's numbers in the page from `start` (or from the page's load, with
 *  `page.addInitScript(townSampling)`) to `stop`, which hands them over. */
export function townSampler(page: Page) {
  return {
    start: () => page.evaluate(townSampling),
    stop: (): Promise<TownSample[]> =>
      page.evaluate(() => {
        const w = window as any;
        clearInterval(w.__townTimer);
        return w.__townSamples ?? [];
      }),
  };
}

/** The middle, the 95th percentile and the most of some numbers (nearest rank; 0s if none). */
export function quantiles(xs: number[]) {
  const v = xs.filter((x) => Number.isFinite(x)).sort((a, b) => a - b);
  const at = (q: number) => (v.length ? v[Math.max(0, Math.ceil(q * v.length) - 1)] : 0);
  const round = (x: number) => Math.round(x * 100) / 100;
  return { n: v.length, p50: round(at(0.5)), p95: round(at(0.95)), max: round(v.length ? v[v.length - 1] : 0) };
}
