import { expect, test, type Page } from "@playwright/test";
import { bc, collectConsole } from "./util";

// Survival in the browser (`scripts/e2e.sh hangar`: a survival server whose fabricator works 60×
// faster, and no dolls). The pilot comes in through their bay's airlock, walks to the exchange
// terminal and buys titanium alloy from the colony, makes a combat knife with it at the
// fabricator, boards at the cockpit hatch and launches: thrown out of its bay's door on the ring,
// it flies home (a dev hook's errand) to the dock off the hub's mouth and docks. The walking is the dev hook's (a guide walks the pilot's own legs, as an agent's are
// walked); the terminals' panels are clicked.

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

/** Walks to a place in the bay and uses it. */
async function use(page: Page, spot: string) {
  await push(page, { cmd: "walk_to", spot });
  await until(page, `at the ${spot}`, (s) => s.focus === spot && !s.walking_to, 120_000);
  await push(page, { cmd: "use" });
}

test("a pilot works their bay, launches through its doors, and docks home", async ({ page }) => {
  test.setTimeout(600_000);
  const logs = collectConsole(page);
  await page.goto("/?autoplay=1&name=Tester&quality=low");
  let s = await until(page, "the bay", (s) => s.place === "hangar" && s.seq === "walking", 180_000);
  // The starter kit: a worn Leo in the gantry, and 2,000 credits.
  expect(s.bay).toBe("docked");
  expect(s.bay_line).toBe("leo");
  expect(s.hangar_credits).toBe(2000);
  expect(s.alive).toBe(false);
  // Second-hand inside too: its radiators are tired. A new arrival is told where they are.
  expect(s.bay_faults).toBe(1);
  await expect(page.locator("#news")).toContainText("ARRIVAL", { timeout: 30_000 });

  // The exchange: 100 kg of titanium alloy from the colony, at its ask.
  await use(page, "exchange");
  await until(page, "the exchange terminal", (s) => s.terminal === "exchange", 10_000);
  await expect(page.locator("#terminal")).toBeVisible();
  await page.click('[data-act="ex-filter"][data-f="goods"]');
  await page.click('tr[data-item="mat.ti_alloy"]');
  await page.fill('[data-key="qty:buy:mat.ti_alloy"]', "100");
  await page.click('[data-act="order"]');
  await expect(page.locator("#term-log")).toContainText("BOUGHT", { timeout: 30_000 });
  // The Charter Board, a tab away: the colony's contracts and its great works.
  await page.click('[data-tab="charter"]');
  await expect(page.locator("#term-body")).toContainText("THE COLONY", { timeout: 30_000 });
  await expect(page.locator("#term-body")).toContainText("A SECOND FOUNDRY");
  await expect(page.locator("#term-body")).toContainText("BEFORE COLONY");
  await page.screenshot({ path: "artifacts/charter.png" });
  await page.keyboard.press("Escape");
  await until(page, "the terminal closed", (s) => !s.terminal, 10_000);

  // The fabricator: a knife from 60 kg of steel and 20 of titanium alloy.
  await use(page, "fabricator");
  await until(page, "the fabricator", (s) => s.terminal === "fabricator", 10_000);
  await page.click('[data-act="fab-filter"][data-f="weapons"]');
  await page.click('[data-act="make"][data-item="weapon.army_knife"]');
  await expect(page.locator("#term-log")).toContainText("QUEUED", { timeout: 30_000 });
  await expect(page.locator("#term-log")).toContainText("MADE", { timeout: 60_000 });
  await page.click('[data-tab="stores"]');
  await expect(page.locator("#term-body")).toContainText(/Knife/);
  await page.keyboard.press("Escape");
  await until(page, "the terminal closed", (s) => !s.terminal, 10_000);

  // The suit's console: overhaul the radiators with components and electronics bought from the
  // colony, and fit a G-seat off its shelf. The stat sheet shows what it would launch as.
  await use(page, "exchange");
  await until(page, "the exchange terminal", (s) => s.terminal === "exchange", 10_000);
  for (const [slug, qty] of [["mat.components", "20"], ["mat.electronics", "5"]]) {
    await page.click('[data-act="ex-filter"][data-f="goods"]');
    await page.click(`tr[data-item="${slug}"]`);
    await page.fill(`[data-key="qty:buy:${slug}"]`, qty);
    await page.click('[data-act="order"]');
    await expect(page.locator("#term-log")).toContainText(/BOUGHT/, { timeout: 30_000 });
  }
  await page.click('[data-act="ex-filter"][data-f="modules"]');
  await page.click('tr[data-item="module.g_seat"]');
  await page.fill('[data-key="qty:buy:module.g_seat"]', "1");
  await page.click('[data-act="order"]');
  await expect(page.locator("#term-log")).toContainText(/G-seat/i, { timeout: 30_000 });
  await page.keyboard.press("Escape");
  await until(page, "the terminal closed", (s) => !s.terminal, 10_000);
  await use(page, "suit");
  await until(page, "the suit's console", (s) => s.terminal === "suit", 10_000);
  await expect(page.locator("#term-body")).toContainText("AS IT WOULD LAUNCH");
  await page.click('[data-act="overhaul"][data-part="torso"]');
  await expect(page.locator("#term-log")).toContainText("OVERHAULED RADIATORS", { timeout: 30_000 });
  await page.click('[data-act="fit"][data-item="module.g_seat"]');
  s = await until(page, "the G-seat fitted", (s) => s.bay_modules === 1 && s.bay_faults === 0, 30_000);
  await page.keyboard.press("Escape");
  await until(page, "the terminal closed", (s) => !s.terminal, 10_000);
  const before = (await bc(page)).hangar_credits;
  expect(before).toBeLessThan(2000);

  // Up the stairs to the hatch, and out through the bay doors: the suit rides its bay's cradle
  // in the door on the spinning ring until the catapult fires, and is thrown out of it.
  await use(page, "cockpit");
  s = await until(page, "the launch", (s) => s.place === "space", 60_000);
  expect(s.bay).toBe("out");
  s = await until(page, "in its bay's cradle", (s) => String(s.surface_body).startsWith("bay:"), 30_000);
  expect(s.surface_body).toBe(`bay:${s.bay_no}`);
  s = await until(page, "flying", (s) => s.seq === "walking" && s.alive, 60_000);
  expect(s.frame).toBe("leo");
  // The launch shot: outside the ring by the open door, the suit going.
  await page.screenshot({ path: "artifacts/hangar-launch.png" });
  s = await until(page, "thrown out of the door", (s) => s.footing === "free" && s.launch_shots >= 1, 10_000);
  const [x, y, z] = String(s.pos).split(",").map(Number);
  // Out by the bay ring (2.25 km off the colony's axis, past its −X face), far from the dock.
  expect(Math.hypot(y + 4200, z)).toBeGreaterThan(1_800);
  expect(x).toBeLessThan(-16_400);

  // Home: flown to the dock off the hub's mouth, at rest there, Enter takes it in.
  await push(page, { cmd: "fly_to", spot: "dock" });
  const atDock = (s: Record<string, any>) => {
    const [x, y, z] = String(s.pos).split(",").map(Number);
    return Math.hypot(x + 17_250, y + 4_200, z) < 150 && s.speed < 10;
  };
  await until(page, "at rest in the dock", atDock, 240_000);
  await page.focus("#bc");
  await page.keyboard.press("Enter");
  s = await until(page, "home", (s) => s.place === "hangar", 30_000);
  s = await until(page, "on foot again", (s) => s.seq === "walking", 30_000);
  expect(s.bay).toBe("docked");

  // The server agrees.
  const status = await (await page.request.get("/status")).json();
  const mine = status.game.hangars.find((h: any) => h.name === "Tester");
  expect(mine?.place).toBe("hangar");
  expect(mine?.bay).toBe("docked");
  expect(status.game.hot_path_allocations).toBe(0);
  const bad = logs.filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked/i.test(l));
  if (bad.length) console.log(bad.join("\n"));
  expect(bad).toEqual([]);
});
