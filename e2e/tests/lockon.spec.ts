import { expect, test } from "@playwright/test";
import { bc, collectConsole } from "./util";

// Lock-on (docs/LOCK.md) in a real browser against the server's Mobile Dolls, flown with the keys
// alone (headless Chromium can't lock the pointer): Y locks the nearest Doll, holding W carries the
// suit in to it, flying locked on, its prediction keeping to the server's; holding Y lets go.
test("lock on to a Doll, close in on it, and let go", async ({ page, request }, info) => {
  test.setTimeout(240_000);
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
  await page.locator('#frames .frame[data-slug="leo"]').click();
  await page.locator("#launch-button").click();
  await wait("in the world", "window.__bc?.link === 'ingame' && window.__bc?.alive && window.__bc?.entities > 0");
  await page.focus("#bc");

  // Y: the nearest Doll, wherever the crosshair is.
  await page.keyboard.press("y");
  await wait("a lock", "window.__bc?.lock_slot >= 0 && window.__bc?.lock_range > 0", 20_000);
  const start = (await bc(page)).lock_range as number;

  // W, held: in to it, flying locked on, the prediction keeping to the server's.
  await page.keyboard.down("w");
  let closest = start;
  let lockedOn = false;
  let worstError = 0;
  const until = Date.now() + 60_000;
  while (Date.now() < until) {
    await page.waitForTimeout(500);
    const s = await bc(page);
    if (s.lock_slot < 0 || !s.alive) break;
    lockedOn ||= s.lockon === true;
    if (s.lock_range > 0) closest = Math.min(closest, s.lock_range);
    worstError = Math.max(worstError, s.prediction_error_m ?? 0);
    if (closest < 60) break;
  }
  await page.keyboard.up("w");
  console.log(`locked at ${start.toFixed(0)} m, closest ${closest.toFixed(0)} m, prediction ≤ ${worstError.toFixed(3)} m`);
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
