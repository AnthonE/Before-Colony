import { expect, test, type Page } from "@playwright/test";
import { bc, collectConsole } from "./util";

// The colony inside (`scripts/e2e.sh colony`: a survival server with the colony open, and no
// dolls). The pilot goes out through their bay's airlock and rides the cap lift down to Hub Gate,
// walks the city's streets to the Exchange floor and buys there, then walks back and rides up to
// the bay. The walking is the dev hook's (a guide walks the pilot's own legs); the terminal's panel
// is clicked.

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
  test.setTimeout(900_000);
  const logs = collectConsole(page);
  await page.goto("/?autoplay=1&name=Flaneur&quality=low");
  await until(page, "the bay", (s) => s.place === "hangar" && s.seq === "walking", 180_000);
  expect((await mine(page, "Flaneur")).status.game.colony).toBe(true);

  // The airlock leads to the cap lift.
  await push(page, { cmd: "walk_to", spot: "airlock" });
  await until(page, "at the airlock", (s) => s.focus === "airlock" && !s.walking_to, 120_000);
  await expect(page.locator("#use")).toContainText("CAP LIFT");
  await push(page, { cmd: "use" });
  await until(page, "the ride down", (s) => s.seq === "lift_down", 10_000);
  let s = await until(page, "the city", (s) => s.place === "city", 30_000);
  // The ride can be skipped.
  await push(page, { cmd: "skip" });
  s = await until(page, "Hub Gate", (s) => s.seq === "walking", 30_000);
  expect(s.strip).toBe(0);
  // Its square is named on arrival.
  await until(page, "Charter Square", (s) => s.district === "CHARTER SQUARE", 10_000);
  expect((await mine(page, "Flaneur")).hangar?.place).toBe("city");

  // The map (M): the strip, its districts, its places.
  await page.focus("#bc");
  await page.keyboard.press("m");
  await expect(page.locator("#map")).toBeVisible({ timeout: 10_000 });
  await expect(page.locator("#map-title")).toContainText("CHARTER SQUARE");
  await page.keyboard.press("m");
  await expect(page.locator("#map")).toBeHidden({ timeout: 10_000 });

  // Down the avenue and round the corner to the Exchange floor; the panel is the bay's.
  await push(page, { cmd: "walk_to", spot: "exchange_floor" });
  await until(page, "the walk", (s) => s.city_walking_to, 10_000);
  await until(page, "at the Exchange floor", (s) => s.focus === "exchange_floor" && !s.city_walking_to, 420_000);
  await expect(page.locator("#use")).toContainText("EXCHANGE");
  await push(page, { cmd: "use" });
  await until(page, "the exchange terminal", (s) => s.terminal === "exchange", 10_000);
  await expect(page.locator("#terminal")).toBeVisible();
  await page.click('[data-act="ex-filter"][data-f="goods"]');
  await page.click('tr[data-item="mat.ti_alloy"]');
  await page.fill('[data-key="qty:buy:mat.ti_alloy"]', "10");
  await page.click('[data-act="order"]');
  await expect(page.locator("#term-log")).toContainText("BOUGHT", { timeout: 30_000 });
  await page.keyboard.press("Escape");
  await until(page, "the terminal closed", (s) => !s.terminal, 10_000);
  expect((await bc(page)).hangar_credits).toBeLessThan(2000);

  // Back to Hub Gate, and up the lift to the bay.
  await push(page, { cmd: "walk_to", spot: "hub_gate_1" });
  await until(page, "at Hub Gate", (s) => s.focus === "hub_gate_1" && !s.city_walking_to, 420_000);
  await expect(page.locator("#use")).toContainText("UP TO YOUR BAY");
  await push(page, { cmd: "use" });
  s = await until(page, "home", (s) => s.place === "hangar", 30_000);
  s = await until(page, "on foot in the bay", (s) => s.seq === "walking" && s.strip === -1, 30_000);
  expect(s.bay).toBe("docked");

  // The server agrees, and its hot path never allocated.
  const { status, hangar } = await mine(page, "Flaneur");
  expect(hangar?.place).toBe("hangar");
  expect(status.game.hot_path_allocations).toBe(0);
  const bad = logs.filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked/i.test(l));
  if (bad.length) console.log(bad.join("\n"));
  expect(bad).toEqual([]);
});
