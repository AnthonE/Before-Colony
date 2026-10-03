import { expect, test, type Page } from "@playwright/test";
import { bc, collectConsole } from "./util";

// The colony inside (`scripts/e2e.sh colony`: a survival server with the colony open, no dolls,
// and an agent strolling outside Hub Gate). The pilot goes out through their bay's airlock and
// rides the cap lift down to Hub Gate, finds the agent there, walks the city's streets to the
// Exchange floor and buys there, finds a sight, then walks back and rides up to the bay. The
// walking is the dev hook's (a guide walks the pilot's own legs); the terminal's panel is clicked.
// Then a tram, a car, and two browsers: two pilots meet at Hub Gate, and one flies a suit in by
// the inner gate over the other, each seeing the other.

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

async function mine(page: Page, name: string) {
  const status = await (await page.request.get("/status")).json();
  return { status, hangar: status.game.hangars.find((h: any) => h.name === name) };
}

test("a pilot rides down into the colony, trades on its Exchange floor, and rides home", async ({ page }) => {
  test.setTimeout(1_500_000);
  const logs = collectConsole(page);
  await page.goto("/?autoplay=1&name=Relena&quality=low");
  await until(page, "the bay", (s) => s.place === "hangar" && s.seq === "walking", 180_000);
  expect((await mine(page, "Relena")).status.game.colony).toBe(true);
  // The objectives on foot run down the chain: first, the cap lift.
  await until(page, "the first objective on foot", (s) => s.objective === "RIDE THE CAP LIFT DOWN", 30_000);

  // The airlock leads to the cap lift.
  await push(page, { cmd: "walk_to", spot: "airlock" });
  await until(page, "at the airlock", (s) => s.focus === "airlock" && !s.walking_to, 120_000);
  await expect(page.locator("#use")).toContainText("CAP LIFT");
  await push(page, { cmd: "use" });
  await until(page, "the ride down", (s) => s.seq === "lift_down", 30_000);
  let s = await until(page, "the city", (s) => s.place === "city", 30_000);
  // The ride can be skipped.
  await push(page, { cmd: "skip" });
  s = await until(page, "Hub Gate", (s) => s.seq === "walking", 30_000);
  expect(s.strip).toBe(0);
  // Its square is named on arrival.
  await until(page, "Charter Square", (s) => s.district === "CHARTER SQUARE", 30_000);
  // Someone's there already: the agent strolling outside Hub Gate, in a flight suit of their own,
  // and the server has the two of them down here.
  s = await until(page, "the flaneur", (s) => s.people >= 1 && String(s.people_names).includes("Flaneur-01"), 30_000);
  expect(s.people_nearest).toBeLessThan(120);
  const city = (await mine(page, "Relena")).status.game.city;
  expect(city.people).toBe(2);
  expect(city.refused_poses).toBe(0);
  expect((await mine(page, "Relena")).hangar?.place).toBe("city");
  await until(page, "the next objective", (s) => s.objective === "FIND THE EXCHANGE FLOOR", 30_000);

  // The map (M): the strip, its districts, its places.
  await page.focus("#bc");
  await page.keyboard.press("m");
  await expect(page.locator("#map")).toBeVisible({ timeout: 30_000 });
  await expect(page.locator("#map-title")).toContainText("CHARTER SQUARE", { timeout: 30_000 });
  await page.keyboard.press("m");
  await expect(page.locator("#map")).toBeHidden({ timeout: 30_000 });

  // Down the avenue, round the corner and in through the Exchange floor's door to its counter: a
  // room, lit by its lamps (the eye adapts to EV 8). The panel is the bay's.
  await push(page, { cmd: "walk_to", spot: "exchange_floor" });
  await until(page, "the walk", (s) => s.city_walking_to, 30_000);
  await until(page, "at the Exchange floor", (s) => s.focus === "exchange_floor" && !s.city_walking_to, 420_000);
  await expect(page.locator("#use")).toContainText("EXCHANGE", { timeout: 30_000 });
  s = await until(page, "indoors", (s) => s.city_room === "exchange_floor" && s.city_ev < 8.5, 30_000);
  await page.screenshot({ path: "artifacts/colony-exchange-floor.png" });
  await until(page, "the last objective", (s) => s.objective === "SELL ON THE EXCHANGE", 30_000);
  await push(page, { cmd: "use" });
  await until(page, "the exchange terminal", (s) => s.terminal === "exchange", 60_000);
  await expect(page.locator("#terminal")).toBeVisible();
  await page.click('[data-act="ex-filter"][data-f="goods"]');
  await page.click('tr[data-item="mat.ti_alloy"]');
  await page.fill('[data-key="qty:buy:mat.ti_alloy"]', "10");
  await page.click('[data-act="order"]');
  await expect(page.locator("#term-log")).toContainText("BOUGHT", { timeout: 30_000 });
  await page.keyboard.press("Escape");
  await until(page, "the terminal closed", (s) => !s.terminal, 60_000);
  expect((await bc(page)).hangar_credits).toBeLessThan(2000);

  // On to the nearest sight, the clock tower over Charter Square: found, on the found-list, and
  // the map lists it.
  expect((await bc(page)).sights_found).toBe(0);
  await push(page, { cmd: "walk_to", spot: "sight_1" });
  await until(page, "the walk", (s) => s.city_walking_to, 30_000);
  await until(page, "the clock tower", (s) => s.sights_found === 1, 420_000);
  await expect(page.locator("#toast")).toContainText("SIGHT FOUND", { timeout: 30_000 });
  await until(page, "there", (s) => !s.city_walking_to, 420_000);
  await page.focus("#bc");
  await page.keyboard.press("m");
  await expect(page.locator("#map-sights")).toContainText("SIGHTS FOUND 1/10", { timeout: 30_000 });
  await expect(page.locator("#map-sights")).toContainText("THE CLOCK TOWER");
  await page.screenshot({ path: "artifacts/colony-map-sights.png" });
  await page.keyboard.press("m");
  await expect(page.locator("#map")).toBeHidden({ timeout: 30_000 });

  // The Arrival's seats: over to one, E to sit (the pilot's seen sitting), a step to stand.
  await push(page, { cmd: "walk_to", spot: "seat" });
  await until(page, "the walk", (s) => s.city_walking_to, 30_000);
  await until(page, "by the seats", (s) => !s.city_walking_to, 420_000);
  await expect(page.locator("#use")).toContainText("SIT", { timeout: 30_000 });
  await push(page, { cmd: "use" });
  await until(page, "seated", (s) => s.seated >= 0, 30_000);
  await expect(page.locator("#use")).toContainText("SEATED", { timeout: 30_000 });
  await page.screenshot({ path: "artifacts/colony-seated.png" });
  await page.focus("#bc");
  await page.keyboard.down("w");
  await until(page, "standing", (s) => s.seated < 0, 30_000);
  await page.keyboard.up("w");
  expect((await mine(page, "Relena")).status.game.city.refused_by?.seat ?? 0).toBe(0);

  // Back to Hub Gate, and up the lift to the bay.
  await push(page, { cmd: "walk_to", spot: "hub_gate_1" });
  await until(page, "at Hub Gate", (s) => s.focus === "hub_gate_1" && !s.city_walking_to, 420_000);
  await expect(page.locator("#use")).toContainText("UP TO YOUR BAY", { timeout: 30_000 });
  await push(page, { cmd: "use" });
  s = await until(page, "home", (s) => s.place === "hangar", 30_000);
  s = await until(page, "on foot in the bay", (s) => s.seq === "walking" && s.strip === -1, 30_000);
  expect(s.bay).toBe("docked");

  // The server agrees, and its hot path never allocated.
  const { status, hangar } = await mine(page, "Relena");
  expect(hangar?.place).toBe("hangar");
  expect(status.game.hot_path_allocations).toBe(0);
  const bad = logs.filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked/i.test(l));
  if (bad.length) console.log(bad.join("\n"));
  expect(bad).toEqual([]);
});

test("a pilot takes the tram from Hub Gate one stop up the line", async ({ page }) => {
  test.setTimeout(900_000);
  const logs = collectConsole(page);
  await page.goto("/?autoplay=1&name=Catherine&quality=low");
  await until(page, "the bay", (s) => s.place === "hangar" && s.seq === "walking", 180_000);
  await push(page, { cmd: "walk_to", spot: "airlock" });
  await until(page, "at the airlock", (s) => s.focus === "airlock" && !s.walking_to, 120_000);
  await push(page, { cmd: "use" });
  await until(page, "the city", (s) => s.place === "city", 30_000);
  await push(page, { cmd: "skip" });
  await until(page, "Hub Gate", (s) => s.seq === "walking", 30_000);

  // Onto Hub Gate's platform, and in through the doors of the next train out (one leaves every
  // 2.8 minutes): asked again while it waits, the guide takes the pilot to the nearest open door.
  const t0 = Date.now();
  let s = await bc(page);
  while (s.riding < 0) {
    // (A guide that finds the doors shut on it gives up; then it's asked again.)
    if (!s.city_walking_to) await push(page, { cmd: "walk_to", spot: "tram" });
    if (Date.now() - t0 > 420_000) throw new Error(`never got on: ${JSON.stringify(s)}`);
    await page.waitForTimeout(1_000);
    s = await bc(page);
  }
  await expect(page.locator("#toast")).toContainText("CHARTER LINE", { timeout: 30_000 });

  // It pulls out, and stops at the next station with its doors open: off onto the platform.
  s = await until(page, "the next station", (s) => s.station === 1, 240_000);
  await page.waitForTimeout(2_500);
  await push(page, { cmd: "walk_to", spot: "tram" });
  s = await until(page, "off the tram", (s) => s.riding < 0, 30_000);
  const [x] = String(s.city_feet).split(",").map(Number);
  expect(Math.abs(x - -13_050)).toBeLessThan(45);

  // The server followed every step of it: on foot, aboard and off again.
  const status = await (await page.request.get("/status")).json();
  console.log(`refused: ${JSON.stringify(status.game.city.refused_by)}`);
  expect(status.game.city.refused_poses).toBe(0);
  expect(status.game.hot_path_allocations).toBe(0);
  const bad = logs.filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked/i.test(l));
  if (bad.length) console.log(bad.join("\n"));
  expect(bad).toEqual([]);
});

test("a pilot takes a car from Hub Gate's motor pool and drives up the avenue", async ({ page }) => {
  test.setTimeout(600_000);
  const logs = collectConsole(page);
  await page.goto("/?autoplay=1&name=Hilde&quality=low");
  await until(page, "the bay", (s) => s.place === "hangar" && s.seq === "walking", 180_000);
  await push(page, { cmd: "walk_to", spot: "airlock" });
  await until(page, "at the airlock", (s) => s.focus === "airlock" && !s.walking_to, 120_000);
  await push(page, { cmd: "use" });
  await until(page, "the city", (s) => s.place === "city", 30_000);
  await push(page, { cmd: "skip" });
  await until(page, "Hub Gate", (s) => s.seq === "walking", 30_000);

  // To the motor pool beside Hub Gate's door, and a car from it.
  await push(page, { cmd: "walk_to", spot: "pool" });
  await until(page, "at the pool", (s) => !s.city_walking_to, 120_000);
  await expect(page.locator("#use")).toContainText("TAKE A CAR", { timeout: 30_000 });
  await push(page, { cmd: "use" });
  let s = await until(page, "at the wheel", (s) => s.driving === "car", 30_000);
  const start = Number(String(s.city_feet).split(",")[0]);

  // Up the avenue for 30 m or so, then the brakes.
  const along = (s: Record<string, any>) => Number(String(s.city_feet).split(",")[0]);
  await page.focus("#bc");
  await page.keyboard.down("w");
  s = await until(page, "under way", (s) => s.drive_speed > 8 && along(s) - start > 30, 90_000);
  // The handbrake stops it dead (held S, a stop runs straight on into reversing).
  await page.keyboard.up("w");
  await page.keyboard.down("Space");
  s = await until(page, "stopped", (s) => Math.abs(s.drive_speed) < 1, 60_000);
  await page.keyboard.up("Space");
  const end = Number(String(s.city_feet).split(",")[0]);
  expect(end - start).toBeGreaterThan(20);

  // Out beside it, on foot again.
  await push(page, { cmd: "use" });
  s = await until(page, "on foot", (s) => s.driving === "", 30_000);
  const status = await (await page.request.get("/status")).json();
  console.log(`refused: ${JSON.stringify(status.game.city.refused_by)}`);
  expect(status.game.city.refused_poses).toBe(0);
  const bad = logs.filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked/i.test(l));
  if (bad.length) console.log(bad.join("\n"));
  expect(bad).toEqual([]);
});

// Where `name` is in `s.people_at` ("name@x,s,h;…"): along, across, up.
function seenAt(s: Record<string, any>, name: string): number[] | undefined {
  const hit = String(s.people_at ?? "")
    .split(";")
    .find((p) => p.startsWith(`${name}@`));
  return hit?.slice(name.length + 1).split(",").map(Number);
}

test("two pilots meet at Hub Gate, and one flies a suit in over the other", async ({ browser }) => {
  test.setTimeout(1_200_000);
  // Two browsers, two pilots: small windows, as two software renderers share the machine.
  const small = { viewport: { width: 640, height: 360 } };
  const [ca, cb] = [await browser.newContext(small), await browser.newContext(small)];
  const [a, b] = [await ca.newPage(), await cb.newPage()];
  const logs = [collectConsole(a), collectConsole(b)];
  // One after the other: the client is a big download and compile, and two at once on one
  // machine can keep a page from answering its connection in time.
  const inBay = (p: Page) => until(p, "the bay", (s) => s.place === "hangar" && s.seq === "walking", 240_000);
  await a.goto("/?autoplay=1&name=Heero&quality=low");
  await inBay(a);
  await b.goto("/?autoplay=1&name=Duo&quality=low");
  await inBay(b);
  const both = async (f: (p: Page) => Promise<unknown>) => Promise.all([f(a), f(b)]);

  // Both down the cap lift to Hub Gate.
  await both((p) => push(p, { cmd: "walk_to", spot: "airlock" }));
  await both((p) => until(p, "at the airlock", (s) => s.focus === "airlock" && !s.walking_to, 180_000));
  await both((p) => push(p, { cmd: "use" }));
  await both((p) => until(p, "the city", (s) => s.place === "city", 120_000));
  await both((p) => push(p, { cmd: "skip" }));
  await both((p) => until(p, "Hub Gate", (s) => s.seq === "walking" && s.strip === 0, 120_000));
  // Heero steps over to the motor pool beside the door; Duo stays at it.
  await push(a, { cmd: "walk_to", spot: "pool" });
  await until(a, "the walk", (s) => s.city_walking_to, 60_000);
  await until(a, "at the pool", (s) => !s.city_walking_to, 240_000);

  // Each sees the other, by name, where the other stands (within 2 m), and the server has both.
  for (const [me, other, them] of [
    [a, b, "Duo"],
    [b, a, "Heero"],
  ] as const) {
    const s = await until(me, `${them} in view`, (s) => seenAt(s, them) !== undefined, 120_000);
    await me.waitForTimeout(1_000);
    const seen = seenAt(await bc(me), them)!;
    const feet = String((await bc(other)).city_feet).split(",").map(Number);
    console.log(`${them}: seen at ${seen}, stands at ${feet} (${s.people} in view)`);
    expect(Math.hypot(seen[0] - feet[0], seen[1] - feet[1])).toBeLessThan(2);
  }
  let status = await (await a.request.get("/status")).json();
  expect(status.game.city.people).toBeGreaterThanOrEqual(3);
  expect(status.game.city.by_strip[0]).toBeGreaterThanOrEqual(3);

  // Duo rides back up to the bay and launches into the colony by the inner gate.
  await push(b, { cmd: "walk_to", spot: "hub_gate_1" });
  await until(b, "at Hub Gate's door", (s) => s.focus === "hub_gate_1" && !s.city_walking_to, 240_000);
  await push(b, { cmd: "use" });
  await until(b, "the bay", (s) => s.place === "hangar" && s.seq === "walking" && s.strip === -1, 120_000);
  await push(b, { cmd: "walk_to", spot: "cockpit" });
  await until(b, "at the cockpit", (s) => s.focus === "cockpit" && !s.walking_to, 240_000);
  await b.locator("canvas").first().click();
  await b.keyboard.press("q");
  await until(b, "inside the colony", (s) => s.place === "space" && s.interior === true && s.alive, 180_000);
  await push(b, { cmd: "skip" });

  // Down from the inner gate to 250 m over Hub Gate's door: Heero, on foot below, watches it come,
  // and Duo, in it, sees Heero.
  await push(b, { cmd: "fly_to", spot: "hub_gate_1", up: 250 });
  await until(b, "the errand", (s) => s.flying_to, 60_000);
  const s = await until(
    a,
    "the suit over Hub Gate",
    (s) => s.watched >= 1 && s.watched_nearest >= 0 && s.watched_nearest < 600,
    420_000,
  );
  console.log(`watched: ${s.watched} suit(s), the nearest ${Math.round(s.watched_nearest)} m off`);
  await until(b, "Heero, from the suit", (s) => seenAt(s, "Heero") !== undefined, 180_000);
  await a.screenshot({ path: "artifacts/colony-suit-overhead.png" });
  await b.screenshot({ path: "artifacts/colony-from-the-suit.png" });

  // The server: Heero watching, Duo's suit inside, nobody's pose refused, and the hot path clean.
  status = await (await a.request.get("/status")).json();
  expect(status.game.inside.watchers).toBeGreaterThanOrEqual(1);
  expect(status.game.inside.suits).toBe(1);
  expect(status.game.city.refused_poses).toBe(0);
  expect(status.game.hot_path_allocations).toBe(0);
  const bad = logs.flat().filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked/i.test(l));
  if (bad.length) console.log(bad.join("\n"));
  expect(bad).toEqual([]);
  await ca.close();
  await cb.close();
});
