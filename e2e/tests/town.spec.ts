import { writeFileSync } from "node:fs";
import { chromium, expect, test, type Page } from "@playwright/test";
import { bc, collectConsole } from "./util";
import { quantiles, townSampler, townSampling, type TownSample } from "./perf";

// The ring's town, in the browser (`scripts/e2e.sh town`: survival as the game runs it, the retired
// cylinder's inside shut, no drones; `docs/FREE_ROAM.md`). A new pilot wakes on the street at Hub
// Gate's lift: the town is drawn round them, its traffic and its people about; they walk out to the
// boulevard, take a car from a kerb and drive it off, get out, walk back to the lift and ride up to
// their bay, and come back down by Q at its airlock. Two pilots in two browsers wake there and one
// sees the other in the square. The walking is the dev hook's (a guide walks the pilot's legs).

const push = (page: Page, cmd: Record<string, unknown>) =>
  page.evaluate((c) => ((window as any).bcInbox ||= []).push(c), cmd);

async function until(
  page: Page,
  what: string,
  pred: (s: Record<string, any>) => boolean,
  ms: number,
): Promise<Record<string, any>> {
  const t0 = Date.now();
  for (;;) {
    const s = await bc(page);
    if (pred(s)) return s;
    if (Date.now() - t0 > ms) throw new Error(`timed out waiting for ${what}: ${JSON.stringify(s)}`);
    await page.waitForTimeout(250);
  }
}

/** Where in the town the pilot is: across the ring, round it, up. */
const townFeet = (s: Record<string, any>) => String(s.town_feet ?? "").split(",").map(Number);

/** Where a pilot named `name` is seen, if they are: `[x, s, h]`. */
function seenAt(s: Record<string, any>, name: string): number[] | undefined {
  const hit = String(s.people_at ?? "")
    .split(";")
    .find((p) => p.startsWith(`${name}@`));
  return hit?.slice(name.length + 1).split(",").map(Number);
}

/** Awake (or come down) in the town, on foot. */
const inTheTown = (page: Page, ms: number) =>
  until(page, "the town", (s) => s.place === "town" && s.seq === "walking" && s.town_feet !== "", ms);

/** From the bay: to its airlock, and down the spoke's lift to the town. */
async function downToTheTown(page: Page) {
  await push(page, { cmd: "walk_to", spot: "airlock" });
  await until(page, "at the airlock", (s) => s.focus === "airlock" && !s.walking_to, 180_000);
  await expect(page.locator("#use")).toContainText("THE TOWN");
  await push(page, { cmd: "hangar", req: { t: "enter_town" } });
  return inTheTown(page, 120_000);
}

const errors = (logs: string[]) => logs.filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked/i.test(l));

/** What the town cost to draw over each stretch of the walk (`marks`: when each ended), to
 *  artifacts/perf-town.json: the build's and the life's ms, the frame rate, the cells and draws. */
function townReport(samples: TownSample[], marks: [string, number][], from: number) {
  const out: Record<string, unknown> = {};
  let t0 = from;
  for (const [name, t1] of marks) {
    const xs = samples.filter((x) => x.at >= t0 && x.at < t1 && x.place === "town");
    out[name] = {
      secs: (t1 - t0) / 1000,
      frames: xs.length ? xs[xs.length - 1].frames - xs[0].frames : 0,
      town_ms: quantiles(xs.map((x) => x.town_ms)),
      life_ms: quantiles(xs.map((x) => x.town_life_ms)),
      fps: quantiles(xs.map((x) => x.fps)),
      cells: quantiles(xs.map((x) => x.town_cells)),
      missing: quantiles(xs.map((x) => x.town_missing)),
      cached: quantiles(xs.map((x) => x.town_cached)),
      draws: quantiles(xs.map((x) => x.town_draws)),
    };
    t0 = t1;
  }
  return out;
}

test("a new pilot wakes in the town, walks it, drives a car off, rides up to their bay and back down", async ({
  page,
}) => {
  test.setTimeout(1_500_000);
  const logs = collectConsole(page);
  // (What the town costs to draw is sampled from the page's load to the lift again:
  // artifacts/perf-town.json.)
  await page.addInitScript(townSampling);
  const sampler = townSampler(page);
  const loaded = Date.now();
  const marks: [string, number][] = [];
  await page.goto("/?autoplay=1&name=Tester&quality=low");

  // Awake at Hub Gate's lift's door, on the floor of its square, in Charter Square, told where
  // their suit is; the town drawn round them, its people and its traffic about.
  let s = await inTheTown(page, 240_000);
  await expect(page.locator("#news")).toContainText("UP THE SPOKE'S LIFT", { timeout: 30_000 });
  const [x0, , h0] = townFeet(s);
  expect(x0).toBeLessThan(-40);
  expect(x0).toBeGreaterThan(-46);
  expect(Math.abs(h0)).toBeLessThan(0.2);
  expect(s.town_district).toBe("CHARTER SQUARE");
  s = await until(page, "the town drawn", (s) => s.town_cells > 10 && s.town_tris > 20_000, 240_000);
  s = await until(page, "its life", (s) => s.town_people > 3 && s.town_cars > 3, 120_000);
  console.log(`the town: ${s.town_cells} cells, ${s.town_tris} triangles, ${s.town_people} people, ${s.town_cars} cars`);
  marks.push(["drawn", Date.now()]);
  let status = await (await page.request.get("/status")).json();
  expect(status.game.town.people).toBe(1);
  expect(status.game.hangars.find((h: any) => h.name === "Tester")?.place).toBe("town");
  await page.screenshot({ path: "artifacts/town-hub-gate.png" });

  // Out to the boulevard's pavement between the lift's piers.
  await push(page, { cmd: "walk_to", spot: "boulevard" });
  s = await until(page, "the boulevard", (s) => !s.town_walking_to && Math.abs(townFeet(s)[0] + 36) < 1.5, 240_000);
  await page.screenshot({ path: "artifacts/town-boulevard.png" });

  // To the nearest car at a kerb: taken, pulled out of its bay and stopped again (W the throttle,
  // Space the handbrake), and left where it stands.
  await push(page, { cmd: "walk_to", spot: "car" });
  s = await until(page, "a car at hand", (s) => s.focus === "car" && !s.town_walking_to, 600_000);
  marks.push(["walk", Date.now()]);
  await expect(page.locator("#use")).toContainText("TAKE THE CAR");
  const [cx, cs] = townFeet(s);
  await push(page, { cmd: "use" });
  s = await until(page, "at the wheel", (s) => s.town_driving === true, 60_000);
  await page.keyboard.down("KeyW");
  s = await until(page, "pulling away", (s) => s.town_speed > 1.5 || s.town_driving === false, 240_000);
  await page.keyboard.up("KeyW");
  await page.screenshot({ path: "artifacts/town-driving.png" });
  await page.keyboard.down("Space");
  s = await until(page, "stopped", (s) => Math.abs(s.town_speed) < 0.5, 120_000);
  await page.keyboard.up("Space");
  const [dx, ds] = townFeet(s);
  expect(Math.hypot(dx - cx, ds - cs)).toBeGreaterThan(1);
  if (s.town_driving) await push(page, { cmd: "use" });
  s = await until(page, "out of the car", (s) => s.town_driving === false, 60_000);
  marks.push(["drive", Date.now()]);

  // Back to the lift, and up to the bay.
  await push(page, { cmd: "walk_to", spot: "lift" });
  await until(page, "back at the lift", (s) => s.focus === "lift" && !s.town_walking_to, 900_000);
  marks.push(["back", Date.now()]);
  // Across the hub: Q at its door rides up the spoke and down the next, Tower Hill's, 3 km round;
  // a walk to a lift there finds that one's door.
  await expect(page.locator("#use")).toContainText("Q  ACROSS THE HUB TO TOWER HILL");
  await page.keyboard.press("KeyQ");
  s = await until(page, "across the hub", (s) => s.town_district === "TOWER HILL", 120_000);
  await push(page, { cmd: "walk_to", spot: "lift" });
  await until(page, "at its lift", (s) => s.focus === "lift" && !s.town_walking_to, 300_000);
  const perf = townReport(await sampler.stop(), marks, loaded);
  console.log(`the town's cost: ${JSON.stringify(perf)}`);
  writeFileSync("artifacts/perf-town.json", JSON.stringify(perf, null, 2));
  await expect(page.locator("#use")).toContainText("UP TO YOUR BAY");
  await push(page, { cmd: "use" });
  await until(page, "their bay", (s) => s.place === "hangar" && s.seq === "walking", 120_000);
  status = await (await page.request.get("/status")).json();
  expect(status.game.town.people).toBe(0);
  // And down again: Q at the bay's airlock is the spoke's lift.
  s = await downToTheTown(page);
  expect(s.town_district).toBe("CHARTER SQUARE");
  status = await (await page.request.get("/status")).json();
  expect(status.game.town.people).toBe(1);
  expect(status.game.hot_path_allocations).toBe(0);
  const bad = errors(logs);
  if (bad.length) console.log(bad.join("\n"));
  expect(bad).toEqual([]);
});

test("two new pilots wake in the town and see each other in Hub Gate's square", async ({ browser }) => {
  test.setTimeout(1_200_000);
  const small = { viewport: { width: 640, height: 360 } };
  const second = await chromium.launch(test.info().project.use.launchOptions ?? {});
  const [ca, cb] = [await browser.newContext(small), await second.newContext(small)];
  const [a, b] = [await ca.newPage(), await cb.newPage()];
  const logs = [collectConsole(a), collectConsole(b)];
  const awake = async (p: Page, name: string) => {
    for (let k = 0; k < 3; k++) {
      await p.goto(`/?autoplay=1&name=${name}&quality=low`);
      const s = await until(
        p,
        "the town",
        (s) => (s.place === "town" && s.seq === "walking" && s.town_feet !== "") || s.link === "failed",
        240_000,
      );
      if (s.link !== "failed") return;
      console.log(`${name}: the link failed (${s.last_error}); loading again`);
    }
    throw new Error(`${name} never got in`);
  };
  await awake(a, "Kai");
  await awake(b, "Nico");
  // Nico steps out to the boulevard; Kai sees him there (within 2 m of where he stands).
  await push(b, { cmd: "walk_to", spot: "boulevard" });
  await until(b, "the boulevard", (s) => !s.town_walking_to && Math.abs(townFeet(s)[0] + 36) < 1.5, 240_000);
  const feet = townFeet(await bc(b));
  const near = (s: Record<string, any>) => {
    const seen = seenAt(s, "Nico");
    return seen !== undefined && Math.hypot(seen[0] - feet[0], seen[1] - feet[1]) < 2;
  };
  const s = await until(a, `Nico seen at ${feet}`, near, 120_000);
  console.log(`Nico: seen at ${seenAt(s, "Nico")}, stands at ${feet}`);
  await a.screenshot({ path: "artifacts/town-two-pilots.png" });
  const status = await (await a.request.get("/status")).json();
  expect(status.game.town.people).toBe(2);
  for (const l of logs) {
    const bad = errors(l);
    if (bad.length) console.log(bad.join("\n"));
    expect(bad).toEqual([]);
  }
  await second.close();
});
