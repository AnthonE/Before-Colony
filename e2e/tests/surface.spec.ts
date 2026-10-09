import { execFileSync } from "node:child_process";
import { expect, test, type Page } from "@playwright/test";
import { bc, built, collectConsole } from "./util";

// Suits on the bodies, end to end: signed in with a stub wallet, the lander autopilot flies from
// the Colonies' base to MO-II, lands in its Aft Well with its grip, crouches and lies still until
// it's hidden. Leaving there parks the suit (asleep, in the hide spot); coming back wakes in it,
// still on the ground and still hidden. Then, flown by hand, it wakes there again and lifts off
// on the thrusters, free past 40 m. All the while the server allocates nothing in its tick, every
// snapshot fits a datagram, and every rider names a body the client knows.
const SIGNER = built("examples/sign");
// Test key 2's address.
const ADDRESS = "0x2b5ad5c4795c026514f8317c7a215e218dccd6cf";
const NAME = "E2E-Hider";

test("land in MO-II's Aft Well, hide there, park on leaving, wake hidden and lift off", async ({ context, page, request }, info) => {
  test.setTimeout(600_000);
  await context.exposeFunction("__signHex", (hex: string) => {
    const message = Buffer.from(hex.slice(2), "hex");
    return { signature: execFileSync(SIGNER, ["2"], { input: message }).toString().trim() };
  });
  await context.addInitScript((address) => {
    (window as any).ethereum = {
      async request({ method, params }: { method: string; params?: any[] }) {
        if (method === "eth_accounts" || method === "eth_requestAccounts") return [address];
        if (method === "personal_sign") return (await (window as any).__signHex(params![0])).signature;
        throw { code: 4200, message: `unsupported: ${method}` };
      },
      on() {},
    };
  }, ADDRESS);

  const logs = collectConsole(page);
  const status = async () => (await (await request.get("/status")).json()).game;
  // The rules that hold throughout, checked at every poll.
  const healthy = async (p: Page) => {
    const b = await bc(p);
    const g = await status();
    expect(g.hot_path_allocations).toBe(0);
    expect(b.max_snapshot ?? 0).toBeLessThanOrEqual(1100);
    expect(b.unresolved_bodies ?? 0).toBe(0);
    return b;
  };
  const waitOn = async (p: Page, what: string, pred: (b: Record<string, any>) => boolean, timeout = 60_000) => {
    const until = Date.now() + timeout;
    for (;;) {
      const b = await healthy(p);
      if (pred(b)) return b;
      if (Date.now() > until) {
        console.log(`--- waiting for ${what} failed; status: ${JSON.stringify(b)}`);
        console.log(logs.slice(-30).join("\n"));
        throw new Error(`timed out waiting for ${what}`);
      }
      await p.waitForTimeout(500);
    }
  };

  // The lander flies; the title waits for the wallet (`autoplay=0`) so the pilot signs in.
  await page.goto(`/?quality=low&gfx=${info.project.name}&frame=leo&autopilot=lander:hide:0:0&autoplay=0`);
  await expect(page.locator("#title")).toBeVisible({ timeout: 60_000 });
  await page.locator("#wallet-button").click();
  await expect(page.locator("#wallet-account")).toContainText(ADDRESS.slice(0, 6));
  await page.locator("#callsign").fill(NAME);
  await page.locator("#launch-button").click();
  await waitOn(page, "signed in", (b) => b.link === "ingame" && b.signed_in && b.alive);
  expect((await bc(page)).autopilot).toBe(true);

  // About 15 km to MO-II at the lander's cruise, then down into the Aft Well.
  const flown = Date.now();
  let landed = await waitOn(
    page,
    "standing in the Aft Well",
    (b) => b.footing === "grounded" && b.surface_body === "landmark:0" && b.hide_spot === "AFT WELL",
    360_000,
  );
  console.log(`landed in ${((Date.now() - flown) / 1000).toFixed(0)} s: ${JSON.stringify(landed)}`);
  expect(landed.grip).toBe(true);
  // Crouched and still for 3 s, it goes dark to enemies.
  await waitOn(page, "hidden", (b) => b.cover === "hidden" && b.footing === "grounded", 30_000);
  // For review: the suit crouched on the well's floor, and the HUD saying so.
  await page.screenshot({ path: `artifacts/surface-${info.project.name}-hidden.png` });

  // Leaving here parks the suit: in arcade there's no restart to outlast, so the button says so.
  await page.evaluate(() => (window as any).bcInbox.push({ cmd: "pause" }));
  await expect(page.locator("#pause")).toBeVisible();
  await expect(page.locator("#disconnect-button")).toHaveText("PARK & DISCONNECT");
  await expect(page.locator("#pause-who")).toContainText("Hidden in AFT WELL");
  await page.locator("#disconnect-button").click();
  await waitOn(page, "the title", (b) => b.link === "idle");
  const asleep = async () => ((await status()).sleepers ?? []).find((p: any) => p.name === NAME);
  await expect.poll(asleep, { timeout: 10_000 }).toBeTruthy();
  await expect.poll(async () => (await status()).sleepers_parked, { timeout: 10_000 }).toBeGreaterThanOrEqual(1);
  // After the power-down it's off enemies' sensors.
  await expect.poll(async () => (await status()).sleepers_hidden, { timeout: 20_000 }).toBeGreaterThanOrEqual(1);

  // Back: awake on the ground where it was left, gripping, and still hidden.
  await page.locator("#launch-button").click();
  const woke = await waitOn(page, "awake", (b) => b.link === "ingame" && b.woke === true && b.footing !== "free" && b.cover !== "exposed");
  console.log(`woke: ${JSON.stringify(woke)}`);
  expect(woke.footing).toBe("grounded");
  expect(woke.surface_body).toBe("landmark:0");
  expect(woke.hide_spot).toBe("AFT WELL");
  expect(woke.cover).toBe("hidden");
  expect(await asleep()).toBeUndefined();
  // It stays put: a few more seconds on the ground, hidden.
  await page.waitForTimeout(3_000);
  const later = await healthy(page);
  expect([later.footing, later.cover, later.hide_spot]).toEqual(["grounded", "hidden", "AFT WELL"]);

  // Parked again, and back without the autopilot: the pilot's own controls hold the suit on the
  // ground from the first command.
  await page.evaluate(() => (window as any).bcInbox.push({ cmd: "pause" }));
  await page.locator("#disconnect-button").click();
  await waitOn(page, "the title again", (b) => b.link === "idle");
  await expect.poll(asleep, { timeout: 10_000 }).toBeTruthy();
  await page.goto(`/?quality=low&gfx=${info.project.name}&frame=leo&autoplay=0`);
  await expect(page.locator("#wallet-account")).toContainText(ADDRESS.slice(0, 6), { timeout: 60_000 });
  await page.locator("#callsign").fill(NAME);
  await page.locator("#launch-button").click();
  const manual = await waitOn(page, "awake by hand", (b) => b.link === "ingame" && b.woke === true && b.footing !== "free");
  expect([manual.autopilot, manual.footing, manual.hide_spot, manual.grip]).toEqual([false, "grounded", "AFT WELL", true]);
  await page.waitForTimeout(2_000);
  expect((await healthy(page)).footing).toBe("grounded");
  // Space held: it stands, hops, and the thrusters climb until the grip lets go, which is flying,
  // not losing it.
  await page.keyboard.down("Space");
  await waitOn(page, "lifted off", (b) => b.footing === "free", 30_000);
  await page.keyboard.up("Space");
  // The HUD says so from the suit as drawn, which a software renderer's few frames a second bring
  // in a moment after the server's word.
  await expect.poll(async () => page.locator("#toast").textContent(), { timeout: 10_000 }).toBe("FLYING");

  const errors = logs.filter((l) => /%cERROR|\[pageerror\]|panicked/.test(l));
  if (errors.length) console.log(errors.join("\n"));
  expect(errors).toEqual([]);
});
