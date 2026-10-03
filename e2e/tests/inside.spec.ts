import { expect, test, type Page } from "@playwright/test";
import { bc, collectConsole } from "./util";

// Suits inside the colony (`scripts/e2e.sh inside`: a survival server with the colony open, no
// dolls, and an agent strolling outside Hub Gate). A pilot walks to the cockpit in their bay and
// presses Q: the suit launches into the colony by the inner gate, its own sector (the client is
// welcomed to it). There it flies with its weapons safe, a little way down the colony and back
// into the gate's ring, and docks into the bay. Another comes down over the avenue by Hub Gate,
// lands there with its grip armed (L), sees the agent strolling outside Hub Gate, walks up the
// avenue (W) and lets go. The flying is the dev hook's (`fly_to`); the grip and the walk are keys.
// (Nobody flies back up from the avenue here: at a frame every few seconds the page sends only its
// newest commands, the server fills the rest with stand-ins, and the 2.9 km climb against the
// colony's pull goes at a couple of metres a second. `bc-server/tests/inside.rs` flies it.)

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

// From the bay to the cockpit, and Q: into the colony by the inner gate, the launch skipped.
async function launchInside(page: Page, name: string) {
  await page.goto(`/?autoplay=1&name=${name}&quality=low`);
  await until(page, "the bay", (s) => s.place === "hangar" && s.seq === "walking", 180_000);
  await push(page, { cmd: "walk_to", spot: "cockpit" });
  await until(page, "at the cockpit", (s) => s.focus === "cockpit" && !s.walking_to, 120_000);
  await page.locator("canvas").first().click();
  await page.keyboard.press("q");
  await until(page, "inside the colony", (s) => s.place === "space" && s.interior === true && s.alive, 120_000);
  await expect(page.locator("#news")).toContainText("INTO THE COLONY", { timeout: 30_000 });
  await push(page, { cmd: "skip" });
  await until(page, "flying", (s) => s.seq === "walking" || s.seq === undefined, 30_000).catch(() => {});
}

test("a pilot launches into the colony by the inner gate, flies down it a way, and docks back", async ({ page }) => {
  test.setTimeout(900_000);
  const logs = collectConsole(page);
  await launchInside(page, "Trowa");
  // Look: the city around the suit.
  await page.waitForTimeout(4_000);
  let s = await bc(page);
  expect(s.city_chunks).toBeGreaterThan(0);
  await page.screenshot({ path: "artifacts/inside.png" });

  // Weapons safe: the trigger does nothing.
  await page.mouse.down();
  await page.waitForTimeout(1_500);
  await page.mouse.up();
  s = await bc(page);
  expect(s.beams ?? 0).toBe(0);

  // 200 m down the colony from the gate, out of its ring (120 m), and back into it.
  const gate = posOf(s);
  await push(page, { cmd: "fly_to", spot: "inner_gate", ahead: 200 });
  await until(page, "the errand", (s) => s.flying_to, 30_000);
  s = await until(page, "down the colony", (s) => !s.flying_to, 300_000);
  expect(posOf(s)[0] - gate[0]).toBeGreaterThan(120);
  await push(page, { cmd: "fly_to", spot: "inner_gate" });
  await until(page, "the errand", (s) => s.flying_to, 30_000);
  await until(page, "at the inner gate", (s) => !s.flying_to, 300_000);
  // At rest in its ring, dock.
  await page.keyboard.down("x");
  await page.waitForTimeout(4_000);
  await page.keyboard.up("x");
  await page.keyboard.press("Enter");
  await until(page, "home in the bay", (s) => s.place === "hangar" && s.interior === false, 120_000);
  expect(logs.filter((l) => l.startsWith("[pageerror]"))).toEqual([]);
});

test("a suit inside the colony lands on the avenue by Hub Gate, sees the people there, and walks it", async ({
  page,
}) => {
  test.setTimeout(1_200_000);
  const logs = collectConsole(page);
  await launchInside(page, "Quatre");

  // Down to 22 m over the avenue, 60 m up it from Hub Gate's door; there, the grip armed: the
  // city catches the suit, and it comes down onto its feet.
  await push(page, { cmd: "fly_to", spot: "hub_gate_1", up: 22, ahead: 60 });
  await until(page, "the errand", (s) => s.flying_to, 30_000);
  await until(page, "over the avenue", (s) => !s.flying_to, 480_000);
  await page.keyboard.press("l");
  let s = await until(page, "on its feet", (s) => s.footing === "grounded" && s.surface_body === "city", 60_000);
  await page.screenshot({ path: "artifacts/inside-landed.png" });
  // From the suit, its pilot sees the people below: the agent strolling outside Hub Gate.
  await until(page, "the flaneur, from the suit", (s) => String(s.people_names).includes("Flaneur-01"), 60_000);
  // Up the avenue at a walk (W, held till it has gone 8 m, however slow the page's frames):
  // on the ground all the way.
  const from = posOf(s);
  const away = (s: Record<string, any>) => {
    const at = posOf(s);
    return Math.hypot(at[0] - from[0], at[1] - from[1], at[2] - from[2]);
  };
  // (At a frame every second or two the page sends its commands in bursts, and between them the
  // server flies the suit hands-off: what the page predicts runs ahead of what the server walks,
  // and settles back to it once the suit stops. So: walk, stop, and look again.)
  for (let k = 0; k < 5 && away(s) < 8; k++) {
    await page.keyboard.down("w");
    await until(page, "walking up the avenue", (s) => away(s) > 12 || s.footing !== "grounded", 120_000);
    await page.keyboard.up("w");
    s = await until(page, "stopped", (s) => s.speed < 0.5, 60_000);
    await page.waitForTimeout(2_000);
    s = await bc(page);
  }
  const walked = away(s);
  console.log(`walked ${walked.toFixed(1)} m up the avenue`);
  expect(walked).toBeGreaterThan(8);
  expect(s.footing).toBe("grounded");
  expect(s.prediction_error_m).toBeLessThan(0.5);
  await page.screenshot({ path: "artifacts/inside-walked.png" });

  // Letting go (L): flying again.
  await page.keyboard.press("l");
  await until(page, "flying", (s) => s.footing === "free", 30_000);
  expect(logs.filter((l) => l.startsWith("[pageerror]"))).toEqual([]);
});
