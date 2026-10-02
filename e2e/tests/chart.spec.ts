import { expect, test } from "@playwright/test";
import { bc, collectConsole, luminanceStdDev } from "./util";

// The chart (M) as a pilot uses it, end to end: open it over the sector, zoom out to the Earth
// Sphere and back, step through its places to MO-II's Aft Well, set a course there (the way goes
// round the station: the well faces away), engage the auto-nav, close the chart and let it fly.
// The suit comes to rest over the well, and the stick is the pilot's again. All the while the
// server's tick allocates nothing.
const MO_II = [-17_500, 1_500, 3_000];

test("set a course on the chart and let the auto-nav fly it to MO-II's Aft Well", async ({ page, request }, info) => {
  test.setTimeout(420_000);
  const logs = collectConsole(page);
  const status = async () => (await (await request.get("/status")).json()).game;
  const waitOn = async (what: string, pred: (b: Record<string, any>) => boolean, timeout = 60_000) => {
    const until = Date.now() + timeout;
    for (;;) {
      const b = await bc(page);
      expect((await status()).hot_path_allocations).toBe(0);
      if (pred(b)) return b;
      if (Date.now() > until) {
        console.log(`--- waiting for ${what} failed; status: ${JSON.stringify(b)}`);
        console.log(logs.slice(-30).join("\n"));
        throw new Error(`timed out waiting for ${what}`);
      }
      await page.waitForTimeout(500);
    }
  };
  const shot = async (name: string) => {
    const png = await page.screenshot({ path: `artifacts/chart-${info.project.name}-${name}.png` });
    // Something is drawn: not a blank or single-colour frame.
    expect(luminanceStdDev(png)).toBeGreaterThan(4);
  };

  await page.goto(`/?quality=low&gfx=${info.project.name}&frame=leo`);
  await expect(page.locator("#title")).toBeVisible({ timeout: 60_000 });
  await page.locator("#callsign").fill("E2E-Navigator");
  await page.locator("#launch-button").click();
  await waitOn("in the world", (b) => b.link === "ingame" && b.alive && b.snapshots >= 30);
  await page.focus("#bc");

  // M: the chart, the view on the pilot's suit.
  await page.keyboard.press("m");
  await waitOn("the chart", (b) => b.map_open);
  await page.waitForTimeout(2_000);
  await shot("sector");

  // 3: out to the Earth Sphere; 2: back to the sector.
  await page.keyboard.press("3");
  await waitOn("the Earth Sphere", (b) => b.chart_view_m > 1e9, 20_000);
  await page.waitForTimeout(1_500);
  await shot("sphere");
  await page.keyboard.press("2");
  await waitOn("the sector", (b) => b.chart_view_m < 1e5, 20_000);

  // ] steps through the places to the Aft Well; Enter sets a course there.
  for (let i = 0; i < 20 && (await bc(page)).chart_selected !== "AFT WELL"; i++) {
    await page.keyboard.press("]");
    await page.waitForTimeout(300);
  }
  expect((await bc(page)).chart_selected).toBe("AFT WELL");
  await page.waitForTimeout(1_500);
  await shot("aft-well");
  await page.keyboard.press("Enter");
  const set = await waitOn("the course", (b) => b.course === "AFT WELL" && b.course_m > 1_000);
  console.log(`course: ${(set.course_m / 1000).toFixed(1)} km, ${set.course_turns} turns`);
  await page.keyboard.press("2");
  await page.waitForTimeout(1_500);
  await shot("course");

  // N: the auto-nav. M closes the chart; the suit flies itself.
  await page.keyboard.press("n");
  await waitOn("the auto-nav", (b) => b.auto_nav);
  await page.keyboard.press("m");
  await waitOn("the chart closed", (b) => !b.map_open);
  await waitOn("under way", (b) => b.speed > 50, 30_000);
  await page.waitForTimeout(3_000);
  await shot("flying");

  // It arrives over the Aft Well, at rest, and hands the stick back.
  const flown = Date.now();
  const there = await waitOn("arrived", (b) => !b.auto_nav, 300_000);
  console.log(`arrived in ${((Date.now() - flown) / 1000).toFixed(0)} s: ${JSON.stringify({ pos: there.pos, speed: there.speed })}`);
  const [x, y, z] = String(there.pos).split(",").map(Number);
  const off = Math.hypot(x - MO_II[0], y - MO_II[1], z - MO_II[2]);
  expect(off).toBeLessThan(700);
  expect(there.speed).toBeLessThan(5);
  expect(there.alive).toBe(true);
  // The course is done with.
  expect(there.course).toBe("");
  await shot("arrived");

  const errors = logs.filter((l) => /%cERROR|\[pageerror\]|panicked/.test(l));
  if (errors.length) console.log(errors.join("\n"));
  expect(errors).toEqual([]);
});
