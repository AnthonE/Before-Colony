import { chromium, expect, test, type Page } from "@playwright/test";
import { bc, collectConsole } from "./util";

// The ring's town, in the browser (`scripts/e2e.sh town`: survival, the retired cylinder's inside
// shut as it is by default, no drones; `docs/FREE_ROAM.md`). A pilot walks to their bay's airlock
// and rides a spoke's lift down to Hub Gate's square: the town is drawn round them, its traffic
// and its people about; they walk out to the boulevard, take a car from a kerb and drive it off,
// get out, walk back to the lift and ride up to their bay. Two pilots in two browsers ride down and
// each sees the other in the square. The walking is the dev hook's (a guide walks the pilot's legs).

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

/** To the bay's airlock, and down the spoke's lift to the town. */
async function downToTheTown(page: Page) {
  await push(page, { cmd: "walk_to", spot: "airlock" });
  await until(page, "at the airlock", (s) => s.focus === "airlock" && !s.walking_to, 180_000);
  await expect(page.locator("#use")).toContainText("THE TOWN");
  await push(page, { cmd: "hangar", req: { t: "enter_town" } });
  return until(page, "the town", (s) => s.place === "town" && s.seq === "walking" && s.town_feet !== "", 120_000);
}

const errors = (logs: string[]) => logs.filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked/i.test(l));

test("a pilot rides down to the town, walks it, drives a car off and rides back up", async ({ page }) => {
  test.setTimeout(1_500_000);
  const logs = collectConsole(page);
  await page.goto("/?autoplay=1&name=Tester&quality=low");
  await until(page, "the bay", (s) => s.place === "hangar" && s.seq === "walking", 240_000);

  // Out of Hub Gate's lift's door, on the floor of its square, in Charter Square; the town drawn
  // round them, its people and its traffic about.
  let s = await downToTheTown(page);
  const [x0, , h0] = townFeet(s);
  expect(x0).toBeLessThan(-40);
  expect(x0).toBeGreaterThan(-46);
  expect(Math.abs(h0)).toBeLessThan(0.2);
  expect(s.town_district).toBe("CHARTER SQUARE");
  s = await until(page, "the town drawn", (s) => s.town_cells > 10 && s.town_tris > 20_000, 240_000);
  s = await until(page, "its life", (s) => s.town_people > 3 && s.town_cars > 3, 120_000);
  console.log(`the town: ${s.town_cells} cells, ${s.town_tris} triangles, ${s.town_people} people, ${s.town_cars} cars`);
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

  // Back to the lift, and up to the bay.
  await push(page, { cmd: "walk_to", spot: "lift" });
  await until(page, "back at the lift", (s) => s.focus === "lift" && !s.town_walking_to, 900_000);
  await expect(page.locator("#use")).toContainText("UP TO YOUR BAY");
  await push(page, { cmd: "use" });
  await until(page, "the bay again", (s) => s.place === "hangar" && s.seq === "walking", 120_000);
  status = await (await page.request.get("/status")).json();
  expect(status.game.town.people).toBe(0);
  expect(status.game.hot_path_allocations).toBe(0);
  const bad = errors(logs);
  if (bad.length) console.log(bad.join("\n"));
  expect(bad).toEqual([]);
});

test("two pilots ride down to the town and see each other in Hub Gate's square", async ({ browser }) => {
  test.setTimeout(1_200_000);
  const small = { viewport: { width: 640, height: 360 } };
  const second = await chromium.launch(test.info().project.use.launchOptions ?? {});
  const [ca, cb] = [await browser.newContext(small), await second.newContext(small)];
  const [a, b] = [await ca.newPage(), await cb.newPage()];
  const logs = [collectConsole(a), collectConsole(b)];
  const inBay = async (p: Page, name: string) => {
    for (let k = 0; k < 3; k++) {
      await p.goto(`/?autoplay=1&name=${name}&quality=low`);
      const s = await until(
        p,
        "the bay",
        (s) => (s.place === "hangar" && s.seq === "walking") || s.link === "failed",
        240_000,
      );
      if (s.link !== "failed") return;
      console.log(`${name}: the link failed (${s.last_error}); loading again`);
    }
    throw new Error(`${name} never got in`);
  };
  await inBay(a, "Kai");
  await inBay(b, "Nico");
  await Promise.all([downToTheTown(a), downToTheTown(b)]);
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
