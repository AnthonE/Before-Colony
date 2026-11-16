import { mkdirSync, writeFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { bc, collectConsole } from "./util";
import { frameClock, quantiles, townSampler, townSampling } from "./perf";

// What the ring's town costs to draw (`scripts/e2e.sh perf`; docs/FREE_ROAM.md, "A client that draws
// a town"): the showcase's crowded street (camera 9, the boulevard by Hub Gate's square at noon) at
// High, live, so the cells stream in within the frame's budget as the game streams them; then held a
// while. The frame times, the build's and the life's, and the counts go to artifacts/perf.json.
// SwiftShader draws in software, so the frame times are for comparing runs on one machine; what's
// asserted is what it can't distort: the build kept to its budget, the counts to their caps.
//
//   BC_PERF_SECS=20 (how long the street is held, s; at least 12 frames)
const secs = Number(process.env.BC_PERF_SECS ?? 20);
// High's caps (`town.rs`'s `reach`, `life::TIERS[2]`): the cells in sight, the people and the cars
// near and far.
const CELLS = 2 * 26 + 1;
const PEOPLE = 96 + 192;
const CARS = 40 + 128;
// The most a frame spends building cells (`town.rs`'s budget, 2 to 5 ms; a step of a cell's build
// past it). The time is the wall clock's, and on a busy machine a frame's few ms of building are now
// and then stretched by the browser's thread waiting its turn (twice the budget, seen here on four
// shared cores), so the bound is four budgets, and the fill is checked to have taken frames.
const BUDGET_MS = 5;

test("the town's crowded street, streamed in and held", async ({ page }, info) => {
  test.setTimeout(900_000);
  const logs = collectConsole(page);
  await page.addInitScript(frameClock);
  await page.addInitScript(townSampling);
  await page.goto(`/?showcase=town&cam=9&t=480&quality=high&gfx=${info.project.name}`);
  const town = townSampler(page);

  // In: every cell in sight built, streamed a few at a time (sampled from the page's load).
  await page.waitForFunction(
    (cells) => {
      const s = (window as any).__bc ?? {};
      return s.town_cells >= cells && s.town_missing === 0;
    },
    CELLS,
    { timeout: 600_000, polling: 250 },
  );
  const loaded = (await town.stop()).filter((x) => x.town_cells > 0);
  const done = loaded.findIndex((x) => x.town_cells >= CELLS && x.town_missing === 0);
  const fill = loaded.slice(0, done < 0 ? loaded.length : done + 1);
  const span = (xs: typeof fill) => (xs.length ? xs[xs.length - 1] : undefined);

  // Held: the street as it stands, its life going by.
  await page.evaluate(() => ((window as any).__frameMs = []));
  await town.start();
  await page.waitForFunction(
    ([ms, n]) => Date.now() - (window as any).__sampledSince >= ms && (window as any).__frameMs.length >= n,
    [secs * 1000, 12],
    { timeout: 600_000, polling: 500 },
  );
  const held = await town.stop();
  const frameMs: number[] = await page.evaluate(() => (window as any).__frameMs);
  const s = await bc(page);

  const report = {
    scene: "town",
    cam: 9,
    t: 480,
    quality: s.gfx_tier,
    backend: s.backend,
    fill: {
      secs: fill.length ? (span(fill)!.at - fill[0].at) / 1000 : 0,
      frames: fill.length ? span(fill)!.frames - fill[0].frames : 0,
      samples: fill.length,
      town_ms: quantiles(fill.map((x) => x.town_ms)),
      life_ms: quantiles(fill.map((x) => x.town_life_ms)),
      missing: quantiles(fill.map((x) => x.town_missing)),
      // Each sample's frame, slowest build of the last frame or two, and cells still to build.
      each: fill.map((x) => [x.frames, x.town_ms, x.town_missing]),
    },
    held: {
      secs: held.length ? (span(held)!.at - held[0].at) / 1000 : 0,
      frames: frameMs.length,
      frame_ms: quantiles(frameMs),
      town_ms: quantiles(held.map((x) => x.town_ms)),
      life_ms: quantiles(held.map((x) => x.town_life_ms)),
    },
    counts: {
      cells: s.town_cells,
      tris: s.town_tris,
      people: s.town_people,
      cars: s.town_cars,
      draws: s.town_draws,
      cached: s.town_cached,
    },
  };
  console.log(JSON.stringify(report));
  mkdirSync("artifacts", { recursive: true });
  writeFileSync("artifacts/perf.json", JSON.stringify(report, null, 2));
  await page.screenshot({ path: "artifacts/perf-town-9.png" });

  expect(s.gfx_tier).toBe("high");
  for (const x of [...loaded, ...held]) {
    expect(x.town_cells).toBeLessThanOrEqual(CELLS);
    expect(x.town_people).toBeLessThanOrEqual(PEOPLE);
    expect(x.town_cars).toBeLessThanOrEqual(CARS);
  }
  expect(s.town_people).toBeGreaterThan(20);
  expect(s.town_cars).toBeGreaterThan(5);
  // Streamed: over frames, cells still to build after the first.
  expect(report.fill.frames).toBeGreaterThanOrEqual(2);
  expect(fill.some((x) => x.town_missing > 0)).toBe(true);
  for (const x of [...loaded, ...held]) expect(x.town_ms).toBeLessThanOrEqual(4 * BUDGET_MS);
  const bad = logs.filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked|wgpu error|validation error/i.test(l));
  if (bad.length) console.log(bad.join("\n"));
  expect(bad).toEqual([]);
});
