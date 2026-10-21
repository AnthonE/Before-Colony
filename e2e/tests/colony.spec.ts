import { spawn } from "node:child_process";
import { chromium, expect, test, type Page } from "@playwright/test";
import { bc, built, calmPointer, collectConsole } from "./util";

// Each move whole: the pointer locks here (`util.ts`, `calmPointer`).
test.beforeEach(({ page }) => page.addInitScript(calmPointer));

// The colony inside (`scripts/e2e.sh colony`: a survival server with the colony open, no drones,
// and an agent strolling outside Hub Gate). The pilot goes out through their bay's airlock and
// rides the cap lift down to Hub Gate, finds the agent there, walks across the square into the
// Proving Ground (the Blast Hall) to its desk and its board, walks the city's streets to the
// Exchange floor and buys there, on to the Charter Board's desk (a delivery to a great work, a
// contract posted and withdrawn), finds a sight, sits at The Arrival and reads its Most Wanted,
// then walks back and rides up to the bay. Each desk shows only what it does, and the objectives
// lead from one to the next. The walking is the dev hook's (a guide walks the pilot's own legs);
// the terminals' panels are clicked.
// Then a tram, a car, a Markab (backed up Hub Gate's terminal on its fan, held there, dropped off
// it with the fan cut, and a leap on its lift jets), the Canal strip's cap lift (Q at the airlock),
// and two browsers: two pilots
// meet at Hub Gate, each seeing the other; one rides home, gone from the other's street, and a
// suit an agent flies in by the inner gate comes down onto the avenue by the other, each seeing
// the other.

// The agent in a suit inside the colony (`crates/bc-bot/examples/suit_inside.rs`).
const SUIT = built("examples/suit_inside");
const BC_URL = process.env.BC_URL ?? "http://127.0.0.1:8080";

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

// Clicks a terminal's button (or row), in the page itself, once it's there. The terminal's panel
// is drawn afresh whenever the hangar, the market or the board changes (every few seconds), and a
// page drawing in software has taken longer than that over Playwright's own click (scrolled into
// view, then stable over two frames): it kept finding the button gone, for minutes.
async function press(page: Page, selector: string) {
  await expect(page.locator(selector).first()).toBeVisible({ timeout: FRAMES });
  await page.evaluate((sel) => (document.querySelector(sel) as HTMLElement).click(), selector);
}

// Every toast the page has shown since it loaded: a later one (the same sight's name, reached
// again a step on) can replace one before a page drawing in software is looked at again.
const KEEP_TOASTS = () =>
  addEventListener("DOMContentLoaded", () => {
    const t = document.getElementById("toast");
    const seen: string[] = ((window as any).bcToasts = []);
    const keep = new MutationObserver(() => seen.push(t?.textContent ?? ""));
    if (t) keep.observe(t, { childList: true, characterData: true, subtree: true });
  });
const toasts = (page: Page) => page.evaluate(() => ((window as any).bcToasts ?? []).join("\n"));

async function mine(page: Page, name: string) {
  const status = await (await page.request.get("/status")).json();
  return { status, hangar: status.game.hangars.find((h: any) => h.name === name) };
}

// What a frame or two shows, waited on in the city: a page drawing it in software, its panels and
// its map over it, has taken from seconds to a minute over a frame in the suite's runs.
const FRAMES = 120_000;

test("a pilot rides down into the colony, trades on its Exchange floor, and rides home", async ({ page }) => {
  // (Long: a page drawing in software walks the city slowly, and this walk calls at the Proving
  // Ground, the Exchange floor, the Charter Board's desk, a sight and The Arrival. 45 minutes
  // weren't enough for a run at a few frames a minute.)
  test.setTimeout(3_300_000);
  const logs = collectConsole(page);
  // The flight certificate (objective bit 11) is taken as earned: a browser drawing in software
  // can't clear the drill in time, and the server's tests fly it (`crates/bc-server/tests/proving.rs`).
  await page.addInitScript(() => localStorage.setItem("bc.settings", "version = 1\nobjectives_done = 2048\n"));
  await page.addInitScript(KEEP_TOASTS);
  await page.goto("/?autoplay=1&name=Maren&quality=low");
  await until(page, "the bay", (s) => s.place === "hangar" && s.seq === "walking", 180_000);
  expect((await mine(page, "Maren")).status.game.colony).toBe(true);
  // The objectives on foot run down the chain: first, the cap lift.
  await until(page, "the first objective on foot", (s) => s.objective === "RIDE THE CAP LIFT DOWN", FRAMES);

  // The airlock leads to the cap lift.
  await push(page, { cmd: "walk_to", spot: "airlock" });
  await until(page, "at the airlock", (s) => s.focus === "airlock" && !s.walking_to, 120_000);
  await expect(page.locator("#use")).toContainText("CAP LIFT");
  await push(page, { cmd: "use" });
  await until(page, "the ride down", (s) => s.seq === "lift_down", FRAMES);
  let s = await until(page, "the city", (s) => s.place === "city", FRAMES);
  // The ride can be skipped.
  await push(page, { cmd: "skip" });
  s = await until(page, "Hub Gate", (s) => s.seq === "walking", FRAMES);
  expect(s.strip).toBe(0);
  // On foot here as in the bay: the bay's line and a dot to aim by, and a click walks.
  await expect(page.locator("#bay-line")).toBeVisible({ timeout: FRAMES });
  await expect(page.locator("#dot")).toBeVisible();
  await expect(page.locator("#prompt-main")).toHaveText(/TO WALK$/);
  // The menu says what leaving down here does: nothing's out, and the pilot wakes in their bay.
  await page.keyboard.press("Escape");
  await expect(page.locator("#pause")).toBeVisible({ timeout: FRAMES });
  await expect(page.locator("#disconnect-button")).toHaveText("LEAVE THE CITY");
  await page.locator('#pause [data-cmd="resume"]').click();
  await expect(page.locator("#pause")).toBeHidden({ timeout: FRAMES });
  // Its square is named on arrival.
  await until(page, "Charter Square", (s) => s.district === "CHARTER SQUARE", FRAMES);
  // Someone's there already: the agent strolling outside Hub Gate, in a flight suit of their own,
  // and the server has the two of them down here.
  s = await until(page, "the flaneur", (s) => s.people >= 1 && String(s.people_names).includes("Flaneur-01"), FRAMES);
  expect(s.people_nearest).toBeLessThan(120);
  const city = (await mine(page, "Maren")).status.game.city;
  expect(city.people).toBe(2);
  expect(city.refused_poses).toBe(0);
  expect((await mine(page, "Maren")).hangar?.place).toBe("city");

  // The next objective is the Proving Ground, off the square (`docs/TRAINING.md`): across it and in
  // through the Blast Hall's blast doors to its desk, a room at a suit's scale under its
  // floodlights. Its desk has the course.
  await until(page, "the next objective", (s) => s.objective === "REPORT TO THE PROVING GROUND", FRAMES);
  await push(page, { cmd: "walk_to", spot: "proving_ground" });
  await until(page, "the walk", (s) => s.city_walking_to, FRAMES);
  await until(
    page,
    "at the Proving Ground's desk",
    (s) => s.focus === "proving_ground" && !s.city_walking_to,
    420_000,
  );
  await expect(page.locator("#use")).toContainText("PROVING GROUND", { timeout: FRAMES });
  await until(page, "in the Blast Hall", (s) => s.city_room === "proving_ground", FRAMES);
  await page.screenshot({ path: "artifacts/colony-proving-ground.png" });
  // Its desk has the Proving Ground's board: the day's best round the course and through the drill,
  // the pars, and how to board a trainer.
  await push(page, { cmd: "use" });
  // (A panel over the city drawn in software: a frame there has taken a minute.)
  await until(page, "the board", (s) => s.terminal === "proving", 180_000);
  await expect(page.locator("#term-body")).toContainText("THE DRILL", { timeout: FRAMES });
  await expect(page.locator("#term-body")).toContainText("gantry", { timeout: FRAMES });
  await page.screenshot({ path: "artifacts/colony-proving-board.png" });
  await push(page, { cmd: "back" });
  await until(page, "the board closed", (s) => s.panel === "none", 180_000);
  await until(page, "the next objective", (s) => s.objective === "FIND THE EXCHANGE FLOOR", FRAMES);

  // The map (M): the strip, its districts, its places (over the dot). M closes it, and so does Esc
  // (not opening the menu).
  await page.focus("#bc");
  await page.keyboard.press("m");
  await expect(page.locator("#map")).toBeVisible({ timeout: FRAMES });
  await expect(page.locator("#map-title")).toContainText("CHARTER SQUARE", { timeout: FRAMES });
  await expect(page.locator("#dot")).toBeHidden();
  await page.keyboard.press("m");
  await expect(page.locator("#map")).toBeHidden({ timeout: FRAMES });
  await page.keyboard.press("m");
  await expect(page.locator("#map")).toBeVisible({ timeout: FRAMES });
  await page.keyboard.press("Escape");
  await expect(page.locator("#map")).toBeHidden({ timeout: FRAMES });
  await expect(page.locator("#pause")).toBeHidden();

  // Down the avenue, round the corner and in through the Exchange floor's door to its counter: a
  // room, lit by its lamps (the eye adapts to EV 8). The panel is the bay's exchange, and here an
  // order can rest on the book (the bay's can't).
  await push(page, { cmd: "walk_to", spot: "exchange_floor" });
  await until(page, "the walk", (s) => s.city_walking_to, FRAMES);
  await until(page, "at the Exchange floor", (s) => s.focus === "exchange_floor" && !s.city_walking_to, 420_000);
  await expect(page.locator("#use")).toContainText("EXCHANGE", { timeout: FRAMES });
  s = await until(page, "indoors", (s) => s.city_room === "exchange_floor" && s.city_ev < 8.5, FRAMES);
  await page.screenshot({ path: "artifacts/colony-exchange-floor.png" });
  // Its door in focus is the floor found: next, the Charter Board.
  await until(page, "the next objective", (s) => s.objective === "REPORT TO THE CHARTER BOARD", FRAMES);
  await push(page, { cmd: "use" });
  await until(page, "the exchange terminal", (s) => s.terminal === "exchange", 180_000);
  await expect(page.locator("#terminal")).toBeVisible();
  await press(page, '[data-act="ex-filter"][data-f="goods"]');
  await press(page, 'tr[data-item="mat.ti_alloy"]');
  await expect(page.locator('[data-key="rest"]')).toBeVisible({ timeout: FRAMES });
  await page.fill('[data-key="qty:buy:mat.ti_alloy"]', "10");
  await press(page, '[data-act="order"]');
  await expect(page.locator("#term-log")).toContainText("BOUGHT", { timeout: FRAMES });
  await page.keyboard.press("Escape");
  await until(page, "the terminal closed", (s) => !s.terminal, 180_000);
  expect((await bc(page)).hangar_credits).toBeLessThan(2000);

  // On along the square to the Charter Board's hall, and its desk: only the board's tab, with what
  // only it does (posting a contract, taking a patrol, the charter). The 200 kg of steel the pilot
  // came with go to the second foundry (a fresh server's is short of steel), for standing; a
  // contract posted here is withdrawn again.
  await push(page, { cmd: "walk_to", spot: "charter_board" });
  await until(page, "the walk", (s) => s.city_walking_to, FRAMES);
  await until(page, "at the Charter Board's desk", (s) => s.focus === "charter_board" && !s.city_walking_to, 420_000);
  await expect(page.locator("#use")).toContainText("CHARTER BOARD", { timeout: FRAMES });
  await push(page, { cmd: "use" });
  await until(page, "the Charter Board", (s) => s.terminal === "charter", 180_000);
  await expect(page.locator('[data-act="post"]')).toBeVisible({ timeout: FRAMES });
  await expect(page.locator('[data-tab="fabricator"]')).toBeHidden();
  await page.screenshot({ path: "artifacts/colony-charter-desk.png" });
  await press(page, '[data-act="contribute"][data-work="second_foundry"][data-item="mat.steel"]');
  await expect(page.locator("#term-log")).toContainText("A SECOND FOUNDRY: DELIVERED", { timeout: FRAMES });
  await page.fill('[data-key="post:qty"]', "10");
  await page.fill('[data-key="post:reward"]', "50");
  await page.fill('[data-key="post:hours"]', "1");
  await press(page, '[data-act="post"]');
  await expect(page.locator("#term-log")).toContainText("POSTED", { timeout: FRAMES });
  await press(page, '[data-act="withdraw"]');
  await expect(page.locator("#term-log")).toContainText("WITHDRAWN", { timeout: FRAMES });
  await page.keyboard.press("Escape");
  await until(page, "the terminal closed", (s) => !s.terminal, 180_000);
  // The desk found and standing earned: next, the rifle (from the stores, at the bay's console).
  await until(page, "the next objective", (s) => s.objective === "ARM YOUR SUIT", FRAMES);

  // On to the nearest sight, the clock tower over Charter Square: found, on the found-list, and
  // the map lists it.
  expect((await bc(page)).sights_found).toBe(0);
  await push(page, { cmd: "walk_to", spot: "sight_1" });
  await until(page, "the walk", (s) => s.city_walking_to, FRAMES);
  await until(page, "the clock tower", (s) => s.sights_found === 1, 420_000);
  await expect.poll(() => toasts(page), { timeout: FRAMES }).toContain("SIGHT FOUND");
  s = await until(page, "there", (s) => !s.city_walking_to, 420_000);
  // The city's traffic and people round the square (logged till a run has seen them).
  console.log(`life by the clock tower: ${s.ambient_people} people, ${s.ambient_cars} cars, ${s.life_ms} ms`);
  await page.focus("#bc");
  await page.keyboard.press("m");
  await expect(page.locator("#map-sights")).toContainText("SIGHTS FOUND 1/10", { timeout: FRAMES });
  await expect(page.locator("#map-sights")).toContainText("THE CLOCK TOWER");
  await page.screenshot({ path: "artifacts/colony-map-sights.png" });
  await page.keyboard.press("m");
  await expect(page.locator("#map")).toBeHidden({ timeout: FRAMES });

  // The Arrival's seats: over to one, E to sit (the pilot's seen sitting), a step to stand.
  await push(page, { cmd: "walk_to", spot: "seat" });
  await until(page, "the walk", (s) => s.city_walking_to, FRAMES);
  await until(page, "by the seats", (s) => !s.city_walking_to, 420_000);
  await expect(page.locator("#use")).toContainText("SIT", { timeout: FRAMES });
  await push(page, { cmd: "use" });
  await until(page, "seated", (s) => s.seated >= 0, FRAMES);
  await expect(page.locator("#use")).toContainText("SEATED", { timeout: FRAMES });
  await page.screenshot({ path: "artifacts/colony-seated.png" });
  await page.focus("#bc");
  await page.keyboard.down("w");
  await until(page, "standing", (s) => s.seated < 0, FRAMES);
  await page.keyboard.up("w");
  expect((await mine(page, "Maren")).status.game.city.refused_by?.seat ?? 0).toBe(0);

  // In to The Arrival's counter: the Most Wanted pinned up behind the bar, and the pilot's terms
  // (with standing, its wreck can be theirs).
  await push(page, { cmd: "walk_to", spot: "bar" });
  await until(page, "the walk", (s) => s.city_walking_to, FRAMES);
  await until(page, "at The Arrival's counter", (s) => s.focus === "bar" && !s.city_walking_to, 420_000);
  await expect(page.locator("#use")).toContainText("THE ARRIVAL", { timeout: FRAMES });
  await push(page, { cmd: "use" });
  await until(page, "the Most Wanted", (s) => s.terminal === "wanted", 180_000);
  await expect(page.locator("#term-body")).toContainText("MOST WANTED", { timeout: FRAMES });
  await expect(page.locator("#term-body")).toContainText("ORION");
  await page.screenshot({ path: "artifacts/colony-arrival.png" });
  await press(page, '[data-act="ace-terms"][data-salvage="1"]');
  await expect(page.locator("#term-log")).toContainText("TERMS: AN ACE'S WRECK", { timeout: FRAMES });
  await page.keyboard.press("Escape");
  await until(page, "the terminal closed", (s) => !s.terminal, 180_000);
  // The Exchange floor, the Charter Board's desk, standing and the Most Wanted read: each its
  // objective's bit (8, 12, 13 and 15).
  s = await until(page, "the desks' objectives", (s) => (s.objectives_done & 45312) === 45312, FRAMES);
  expect(s.objectives_done & 45312).toBe(45312);

  // Back to Hub Gate, and up the lift to the bay.
  await push(page, { cmd: "walk_to", spot: "hub_gate_1" });
  s = await until(page, "at Hub Gate", (s) => s.focus === "hub_gate_1" && !s.city_walking_to, 420_000);
  console.log(`life at Hub Gate: ${s.ambient_people} people, ${s.ambient_cars} cars, ${s.life_ms} ms`);
  await expect(page.locator("#use")).toContainText("UP TO YOUR BAY", { timeout: FRAMES });
  await push(page, { cmd: "use" });
  s = await until(page, "home", (s) => s.place === "hangar", FRAMES);
  s = await until(page, "on foot in the bay", (s) => s.seq === "walking" && s.strip === -1, FRAMES);
  expect(s.bay).toBe("docked");

  // The server agrees, and its hot path never allocated.
  const { status, hangar } = await mine(page, "Maren");
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

test("a pilot takes a Markab from Hub Gate's motor pool, backs it up the terminal on its fan, and drops off", async ({
  page,
}) => {
  test.setTimeout(600_000);
  const logs = collectConsole(page);
  await page.goto("/?autoplay=1&name=Lars&quality=low");
  await until(page, "the bay", (s) => s.place === "hangar" && s.seq === "walking", 180_000);
  await push(page, { cmd: "walk_to", spot: "airlock" });
  await until(page, "at the airlock", (s) => s.focus === "airlock" && !s.walking_to, 120_000);
  await push(page, { cmd: "use" });
  await until(page, "the city", (s) => s.place === "city", 30_000);
  await push(page, { cmd: "skip" });
  await until(page, "Hub Gate", (s) => s.seq === "walking", 30_000);

  // To the motor pool beside Hub Gate's door, and a Markab from it (R).
  await push(page, { cmd: "walk_to", spot: "pool" });
  await until(page, "at the pool", (s) => !s.city_walking_to, 120_000);
  await expect(page.locator("#use")).toContainText("R  A MARKAB", { timeout: 30_000 });
  await page.focus("#bc");
  await page.keyboard.press("r");
  let s = await until(page, "at the wheel", (s) => s.driving === "markab" && s.markab === "floor", 30_000);
  expect(s.markab_fan).toBe(true);
  await expect(page.locator("#use")).toContainText("FAN ON", { timeout: 30_000 });

  // Hub Gate's terminal stands behind it: backed into it, the fan takes it up its face, tail first.
  await page.keyboard.down("s");
  await until(page, "on the terminal's face", (s) => s.markab === "wall", 60_000);
  s = await until(page, "up it", (s) => s.markab === "wall" && s.markab_h > 8, 90_000);
  await page.keyboard.up("s");
  // Let go, it holds there.
  s = await until(page, "held on the wall", (s) => s.markab === "wall" && s.drive_speed < 0.5, 30_000);
  await expect(page.locator("#use")).toContainText("ON THE WALL", { timeout: 30_000 });

  // The fan off: nothing holds it, it falls, and comes down on its wheels (a fall of a second or
  // so, which a slow page may draw in one frame: what's watched for is where it ends).
  await page.keyboard.press("f");
  s = await until(
    page,
    "fallen",
    (s) => !s.markab_fan && s.markab === "floor" && s.markab_h < 1 && s.drive_speed < 1,
    60_000,
  );

  // The fan on again, and a leap on the lift jets.
  await page.keyboard.press("f");
  await page.keyboard.down("Space");
  await until(page, "in the air", (s) => s.markab === "air", 30_000);
  await page.keyboard.up("Space");
  s = await until(page, "down again", (s) => s.markab === "floor" && s.drive_speed < 1, 60_000);

  // Out beside it, on foot again; the server took every pose, up the wall and in the air too.
  await push(page, { cmd: "use" });
  s = await until(page, "on foot", (s) => s.driving === "", 30_000);
  const status = await (await page.request.get("/status")).json();
  console.log(`refused: ${JSON.stringify(status.game.city.refused_by)}`);
  expect(status.game.city.refused_poses).toBe(0);
  const bad = logs.filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked/i.test(l));
  if (bad.length) console.log(bad.join("\n"));
  expect(bad).toEqual([]);
});

test("a pilot chooses the Canal strip's cap lift at the airlock, and rides up again from its Hub Gate", async ({
  page,
}) => {
  test.setTimeout(600_000);
  const logs = collectConsole(page);
  await page.goto("/?autoplay=1&name=Sally&quality=low");
  await until(page, "the bay", (s) => s.place === "hangar" && s.seq === "walking", 180_000);
  await push(page, { cmd: "walk_to", spot: "airlock" });
  await until(page, "at the airlock", (s) => s.focus === "airlock" && !s.walking_to, 120_000);
  // E is the Charter strip's lift; Q chooses the next strip's, and the prompt names it.
  await expect(page.locator("#use")).toContainText("CAP LIFT 1, DOWN TO THE CHARTER STRIP", { timeout: 30_000 });
  await page.focus("#bc");
  await page.keyboard.press("q");
  await expect(page.locator("#use")).toContainText("CAP LIFT 2, DOWN TO THE CANAL STRIP", { timeout: 30_000 });
  await push(page, { cmd: "use" });
  await until(page, "the ride down", (s) => s.seq === "lift_down", 30_000);
  await until(page, "the city", (s) => s.place === "city", 30_000);
  await push(page, { cmd: "skip" });
  let s = await until(page, "the Canal strip's Hub Gate", (s) => s.seq === "walking", 30_000);
  expect(s.strip).toBe(1);
  await until(page, "the Depots", (s) => s.district === "THE DEPOTS", 30_000);
  const status = await (await page.request.get("/status")).json();
  expect(status.game.city.by_strip[1]).toBeGreaterThanOrEqual(1);

  // Up again from the Canal strip's own Hub Gate.
  await push(page, { cmd: "walk_to", spot: "hub_gate_2" });
  await until(page, "at Hub Gate", (s) => s.focus === "hub_gate_2" && !s.city_walking_to, 120_000);
  await expect(page.locator("#use")).toContainText("UP TO YOUR BAY", { timeout: 30_000 });
  await push(page, { cmd: "use" });
  await until(page, "home", (s) => s.place === "hangar", 30_000);
  s = await until(page, "on foot in the bay", (s) => s.seq === "walking" && s.strip === -1, 30_000);
  // Back at the airlock, E is the Charter strip's lift again.
  await push(page, { cmd: "walk_to", spot: "airlock" });
  await until(page, "at the airlock", (s) => s.focus === "airlock" && !s.walking_to, 120_000);
  await expect(page.locator("#use")).toContainText("CAP LIFT 1, DOWN TO THE CHARTER STRIP", { timeout: 30_000 });
  const after = (await (await page.request.get("/status")).json()).game;
  expect(after.city.refused_poses).toBe(0);
  expect(after.hot_path_allocations).toBe(0);
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

test("two pilots meet at Hub Gate; one rides home, and a suit comes down by the other", async ({ browser }) => {
  test.setTimeout(1_200_000);
  // Two browsers, two pilots: small windows, as two software renderers share the machine. The
  // second is a browser of its own: two pages of one share its GPU process, and the one already
  // drawing the city leaves the other a frame a minute.
  const small = { viewport: { width: 640, height: 360 } };
  const second = await chromium.launch(test.info().project.use.launchOptions ?? {});
  const [ca, cb] = [await browser.newContext(small), await second.newContext(small)];
  const [a, b] = [await ca.newPage(), await cb.newPage()];
  const logs = [collectConsole(a), collectConsole(b)];
  // One after the other: the client is a big download and compile, and a page rendering in
  // software beside another can take longer than its dial's 10 s to see its connection open (a
  // first connection that fails waits for the player, who would load the page again).
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
  const both = async (f: (p: Page) => Promise<unknown>) => Promise.all([f(a), f(b)]);

  // Both down the cap lift to Hub Gate.
  await both((p) => push(p, { cmd: "walk_to", spot: "airlock" }));
  await both((p) => until(p, "at the airlock", (s) => s.focus === "airlock" && !s.walking_to, 180_000));
  await both((p) => push(p, { cmd: "use" }));
  await both((p) => until(p, "the city", (s) => s.place === "city", 120_000));
  await both((p) => push(p, { cmd: "skip" }));
  await both((p) => until(p, "Hub Gate", (s) => s.seq === "walking" && s.strip === 0, 120_000));
  // Kai steps over to the motor pool beside the door; Nico stays at it. (Over there and stopped:
  // a page drawing in software can walk the few seconds of it in one or two long frames, too soon
  // for a look to catch the walk under way.)
  const from = String((await bc(a)).city_feet).split(",").map(Number);
  await push(a, { cmd: "walk_to", spot: "pool" });
  await until(
    a,
    "at the pool",
    (s) => {
      const [x, y] = String(s.city_feet).split(",").map(Number);
      return !s.city_walking_to && Math.hypot(x - from[0], y - from[1]) > 10;
    },
    240_000,
  );

  // Each sees the other, by name, where the other stands (within 2 m), and the server has both.
  // (Seen a moment late, as everyone is: on a page drawing in software a walk's last frame, and the
  // pose it sends, can be seconds and metres long, so the sight is waited on to catch up.)
  for (const [me, other, them] of [
    [a, b, "Nico"],
    [b, a, "Kai"],
  ] as const) {
    await until(me, `${them} in view`, (s) => seenAt(s, them) !== undefined, 120_000);
    const feet = String((await bc(other)).city_feet).split(",").map(Number);
    const near = (s: Record<string, any>) => {
      const seen = seenAt(s, them);
      return seen !== undefined && Math.hypot(seen[0] - feet[0], seen[1] - feet[1]) < 2;
    };
    const s = await until(me, `${them} seen where they stand, ${feet}`, near, 60_000);
    console.log(`${them}: seen at ${seenAt(s, them)}, stands at ${feet} (${s.people} in view)`);
  }
  let status = await (await a.request.get("/status")).json();
  expect(status.game.city.people).toBeGreaterThanOrEqual(3);
  expect(status.game.city.by_strip[0]).toBeGreaterThanOrEqual(3);

  // Nico rides back up to the bay, and is gone from Kai's street.
  await push(b, { cmd: "walk_to", spot: "hub_gate_1" });
  await until(b, "at Hub Gate's door", (s) => s.focus === "hub_gate_1" && !s.city_walking_to, 240_000);
  await push(b, { cmd: "use" });
  await until(b, "the bay", (s) => s.place === "hangar" && s.seq === "walking" && s.strip === -1, 120_000);
  await until(a, "Nico gone", (s) => seenAt(s, "Nico") === undefined, 60_000);
  status = await (await a.request.get("/status")).json();
  expect(status.game.city.people).toBe(2);
  // (Kai's page has the machine to itself for the rest.)
  await cb.close();
  await second.close();

  // A suit comes in by the inner gate and down onto the avenue 60 m up from Hub Gate's door: an
  // agent flies it (a page drawing in software beside another can't fly a suit down 2.9 km), and
  // says every few seconds who it sees below. Kai, on foot by the door, watches it come and
  // stand there, and the suit's pilot sees Kai.
  let said = "";
  const agent = spawn(SUIT, ["--server", BC_URL, "--name", "Lin", "--land"]);
  for (const out of [agent.stdout, agent.stderr]) out.on("data", (d: Buffer) => (said += d.toString()));
  const lines = () => said.replace(/\x1b\[[0-9;]*m/g, "").split("\n");
  try {
    let s = await until(a, "the suit coming", (s) => s.watched >= 1 && s.watched_nearest >= 0, 300_000);
    console.log(`a suit in view, ${Math.round(s.watched_nearest)} m off`);
    s = await until(a, "the suit standing on the avenue", (s) => s.watched_standing >= 1, 300_000);
    console.log(`it stands ${Math.round(s.watched_nearest)} m off`);
    expect(s.watched_nearest).toBeLessThan(150);
    await a.screenshot({ path: "artifacts/colony-suit-on-the-avenue.png" });
    const t0 = Date.now();
    while (!lines().some((l) => /sees \[.*\bKai\b.*\] standing=true/.test(l))) {
      if (Date.now() - t0 > 120_000) throw new Error(`the suit never saw Kai:\n${lines().slice(-20).join("\n")}`);
      await a.waitForTimeout(1_000);
    }
    console.log(lines().filter((l) => l.includes("sees [")).slice(-1)[0]);

    // The server: Kai (and the flaneur) watching, one suit inside, nobody's pose refused, and the
    // hot path clean.
    status = await (await a.request.get("/status")).json();
    expect(status.game.inside.watchers).toBeGreaterThanOrEqual(1);
    expect(status.game.inside.suits).toBe(1);
    expect(status.game.city.refused_poses).toBe(0);
    expect(status.game.hot_path_allocations).toBe(0);
  } finally {
    agent.kill();
  }
  const bad = logs.flat().filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked/i.test(l));
  if (bad.length) console.log(bad.join("\n"));
  expect(bad).toEqual([]);
  await ca.close();
});
