import { expect, test, type Page } from "@playwright/test";
import { bc, collectConsole } from "./util";

// Suits inside the colony (`scripts/e2e.sh inside`: a survival server with the colony open, no
// dolls). The pilot walks to the cockpit in their bay and presses Q: the suit launches into the
// colony by the inner gate, its own sector (the client is welcomed to it). There it flies among
// the city's buildings with its weapons safe, comes down over the avenue by Hub Gate, lands there
// with its grip armed (L) and walks up it (W), lets go, and flies back up to dock at the inner
// gate into the bay. The flying is the dev hook's (`fly_to`); the grip and the walk are keys.

const push = (page: Page, cmd: Record<string, unknown>) =>
  page.evaluate((c) => ((window as any).bcInbox ||= []).push(c), cmd);

// The own suit's place (`__bc.pos`, "x,y,z" in the sector's frame).
const posOf = (s: Record<string, any>) => String(s.pos ?? "").split(",").map(Number);

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

  // Down to 22 m over the avenue, 60 m up it from Hub Gate's door; there, the grip armed: the
  // city catches the suit, and it comes down onto its feet.
  await push(page, { cmd: "fly_to", spot: "hub_gate_1", up: 22, ahead: 60 });
  await until(page, "the errand", (s) => s.flying_to, 30_000);
  await until(page, "over the avenue", (s) => !s.flying_to, 300_000);
  await page.keyboard.press("l");
  s = await until(page, "on its feet", (s) => s.footing === "grounded" && s.surface_body === "city", 60_000);
  await page.screenshot({ path: "artifacts/inside-landed.png" });
  // Up the avenue at a walk (W): on the ground all the way.
  const from = posOf(s);
  await page.keyboard.down("w");
  await page.waitForTimeout(3_000);
  await page.keyboard.up("w");
  s = await until(page, "stopped", (s) => s.speed < 0.5, 30_000);
  const to = posOf(s);
  const walked = Math.hypot(to[0] - from[0], to[1] - from[1], to[2] - from[2]);
  console.log(`walked ${walked.toFixed(1)} m up the avenue`);
  expect(walked).toBeGreaterThan(8);
  expect(s.footing).toBe("grounded");
  expect(s.prediction_error_m).toBeLessThan(0.5);
  await page.screenshot({ path: "artifacts/inside-walked.png" });

  // Letting go (L), and back up to the inner gate: at rest in its ring, dock.
  await page.keyboard.press("l");
  await until(page, "flying", (s) => s.footing === "free", 30_000);
  await push(page, { cmd: "fly_to", spot: "inner_gate" });
  await until(page, "the errand", (s) => s.flying_to, 30_000);
  await until(page, "at the inner gate", (s) => !s.flying_to, 300_000);
  await page.keyboard.down("x");
  await page.waitForTimeout(4_000);
  await page.keyboard.up("x");
  await page.keyboard.press("Enter");
  await until(page, "home in the bay", (s) => s.place === "hangar" && s.interior === false, 120_000);
  expect(logs.filter((l) => l.startsWith("[pageerror]"))).toEqual([]);
});
