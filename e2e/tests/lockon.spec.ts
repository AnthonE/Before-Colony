import { expect, test } from "@playwright/test";
import { bc, collectConsole } from "./util";

// Lock-on (docs/LOCK.md) in a real browser against the server's Mobile Dolls, flown with the keys
// (headless Chromium can't lock the pointer, so a dev hook keeps the aim on the nearest Doll): Y
// locks it, holding W carries the suit in to it, flying locked on, its prediction keeping to the
// server's; holding Y lets go.
test("lock on to a Doll, close in on it, and let go", async ({ page, request }, info) => {
  test.setTimeout(420_000);
  const logs = collectConsole(page);
  const wait = async (what: string, pred: string, timeout = 60_000) => {
    try {
      await page.waitForFunction(pred, null, { timeout, polling: 250 });
    } catch (e) {
      console.log(`--- waiting for ${what} failed; status: ${JSON.stringify(await bc(page))}`);
      console.log(logs.slice(-30).join("\n"));
      throw e;
    }
  };
  await page.goto(`/?quality=low&gfx=${info.project.name}`);
  await expect(page.locator("#title")).toBeVisible({ timeout: 60_000 });
  await page.locator("#callsign").fill("E2E-Lock");
  // Wing Zero: a Leo flying in on eight Dolls is often shot down before it's halfway there.
  await page.locator('#frames .frame[data-slug="wingzero"]').click();
  await page.locator("#launch-button").click();
  await wait("in the world", "window.__bc?.link === 'ingame' && window.__bc?.alive && window.__bc?.entities > 0");
  await page.focus("#bc");

  // On to the nearest Doll (a dev hook keeps the aim on it, as a pilot's mouse would: headless
  // Chromium can't lock the pointer), flying in until one is within a lock's reach.
  await page.evaluate(() => (window as any).bcInbox.push({ cmd: "aim_hostile", on: true }));
  await page.keyboard.down("w");
  await wait("a Doll within reach", "window.__bc?.hostile_range > 0 && window.__bc?.hostile_range < 3000", 120_000);
  await page.keyboard.up("w");

  // Y: the Doll under the crosshair.
  await page.keyboard.press("y");
  await wait("a lock", "window.__bc?.lock_slot >= 0 && window.__bc?.lock_range > 0", 20_000);
  const start = (await bc(page)).lock_range as number;

  // W and boost, held: in to it (a Doll keeps its distance, so boost), flying locked on, the
  // prediction keeping to the server's.
  await page.keyboard.down("Shift");
  await page.keyboard.down("w");
  let closest = start;
  let lockedOn = false;
  let worstError = 0;
  let ended = "the time ran out";
  const until = Date.now() + 90_000;
  while (Date.now() < until) {
    await page.waitForTimeout(500);
    const s = await bc(page);
    if (s.lock_slot < 0 || !s.alive) {
      ended = !s.alive ? "shot down" : "the lock let go";
      break;
    }
    lockedOn ||= s.lockon === true;
    if (s.lock_range > 0) closest = Math.min(closest, s.lock_range);
    worstError = Math.max(worstError, s.prediction_error_m ?? 0);
    if (closest < 60) {
      ended = "in reach";
      break;
    }
  }
  await page.keyboard.up("w");
  await page.keyboard.up("Shift");
  console.log(`locked at ${start.toFixed(0)} m, closest ${closest.toFixed(0)} m (${ended}), prediction ≤ ${worstError.toFixed(3)} m`);
  expect(lockedOn).toBe(true);
  expect(closest).toBeLessThan(Math.max(60, start * 0.5));
  expect(worstError).toBeLessThan(0.5);
  await page.screenshot({ path: `artifacts/lockon-${info.project.name}.png` });

  // Y held: let go.
  const s = await bc(page);
  if (s.lock_slot >= 0) {
    await page.keyboard.down("y");
    await page.waitForTimeout(700);
    await page.keyboard.up("y");
    await expect.poll(async () => (await bc(page)).lock_slot).toBe(-1);
  }

  // The tick stayed allocation-free throughout.
  const status = await (await request.get("/status")).json();
  expect(status.game?.hot_path_allocations ?? 0).toBe(0);
  const errors = logs.filter((l) => /%cERROR|\[pageerror\]|panicked/.test(l));
  expect(errors).toEqual([]);
});
