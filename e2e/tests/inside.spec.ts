import { expect, test, type Page } from "@playwright/test";
import { bc, collectConsole } from "./util";

// Suits inside the colony (`scripts/e2e.sh inside`: a survival server with the colony open, no
// dolls). The pilot walks to the cockpit in their bay and presses Q: the suit launches into the
// colony by the inner gate, its own sector (the client is welcomed to it). There it flies among
// the city's buildings with its weapons safe, and docks back at the inner gate into the bay.

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

test("a pilot launches into the colony by the inner gate, flies there, and docks back", async ({ page }) => {
  test.setTimeout(600_000);
  const logs = collectConsole(page);
  await page.goto("/?autoplay=1&name=Quatre&quality=low");
  await until(page, "the bay", (s) => s.place === "hangar" && s.seq === "walking", 180_000);

  // To the cockpit, and Q: into the colony.
  await push(page, { cmd: "walk_to", spot: "cockpit" });
  await until(page, "at the cockpit", (s) => s.focus === "cockpit" && !s.walking_to, 120_000);
  await page.locator("canvas").first().click();
  await page.keyboard.press("q");
  let s = await until(page, "inside the colony", (s) => s.place === "space" && s.interior === true && s.alive, 120_000);
  await expect(page.locator("#news")).toContainText("INTO THE COLONY", { timeout: 30_000 });
  // Skip the launch sequence, and look: the city around the suit.
  await push(page, { cmd: "skip" });
  await until(page, "flying", (s) => s.seq === "walking" || s.seq === undefined, 30_000).catch(() => {});
  await page.waitForTimeout(4_000);
  s = await bc(page);
  expect(s.city_chunks).toBeGreaterThan(0);
  await page.screenshot({ path: "artifacts/inside.png" });

  // Weapons safe: the trigger does nothing.
  await page.mouse.down();
  await page.waitForTimeout(1_500);
  await page.mouse.up();
  s = await bc(page);
  expect(s.beams ?? 0).toBe(0);

  // At rest at the inner gate (the launch leaves it there, flight assist holding): dock.
  await page.keyboard.down("x");
  await page.waitForTimeout(4_000);
  await page.keyboard.up("x");
  await page.keyboard.press("Enter");
  await until(page, "home in the bay", (s) => s.place === "hangar" && s.interior === false, 120_000);
  expect(logs.filter((l) => l.startsWith("[pageerror]"))).toEqual([]);
});
