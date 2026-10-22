import { execFileSync } from "node:child_process";
import { expect, test, type Browser, type Page, type TestInfo } from "@playwright/test";
import { bc, built, collectConsole } from "./util";

// Homes (`docs/OPEN_CITY.md`; `scripts/e2e.sh home`: a survival server with the colony open and no
// drones, its register of homes seeded by `examples/seed_homes` with test keys 1 and 2 asleep in
// the beds of two neighbouring flats). Two pilots sign in with stub wallets and wake at home, beside
// their beds. Ada sets her flat's code at its panel and puts a sofa down in build mode. Bo walks
// along the corridor to her door and is stopped there, shut and locked; knocks (Ada hears it); is
// shocked for a wrong code; enters hers, opens the door and comes in, and sees the sofa and the bed.
// Ada logs out at home: Bo sees her asleep in her bed. She signs in again and wakes beside it.
// The walking is the dev hook's (a guide walks the pilot's own legs); the panels are clicked.

const SIGNER = built("examples/sign");
const SEED: { a: { id: number; address: string }; b: { id: number; address: string } } = JSON.parse(
  process.env.BC_HOME_SEED ?? '{"a":{"id":0},"b":{"id":0}}',
);
// What a frame or two shows, waited on in the city: a page drawing in software has taken from
// seconds to a minute over a frame.
const FRAMES = 120_000;

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

// Clicks a terminal's button in the page itself, once it's there (the panel is drawn afresh
// whenever the hangar changes: `colony.spec.ts`'s `press`).
async function press(page: Page, selector: string) {
  await expect(page.locator(selector).first()).toBeVisible({ timeout: FRAMES });
  await page.evaluate((sel) => (document.querySelector(sel) as HTMLElement).click(), selector);
}

// Types into a terminal's field (its draft is kept as typed, through redraws).
async function type(page: Page, key: string, text: string) {
  const field = page.locator(`[data-key="${key}"]`).first();
  await expect(field).toBeVisible({ timeout: FRAMES });
  await field.fill(text);
}

// A browser of its own, signed in with test key `key`'s stub wallet (`login.spec.ts`), as `name`.
async function signIn(browser: Browser, key: number, name: string, info: TestInfo) {
  const address = execFileSync(SIGNER, [String(key), "address"]).toString().trim();
  const context = await browser.newContext();
  await context.exposeFunction("__signHex", (hex: string) => {
    const message = Buffer.from(hex.slice(2), "hex");
    return { signature: execFileSync(SIGNER, [String(key)], { input: message }).toString().trim() };
  });
  await context.addInitScript((addr) => {
    (window as any).ethereum = {
      async request({ method, params }: { method: string; params?: any[] }) {
        if (method === "eth_accounts" || method === "eth_requestAccounts") return [addr];
        if (method === "personal_sign") return (await (window as any).__signHex(params![0])).signature;
        throw { code: 4200, message: `unsupported: ${method}` };
      },
      on() {},
    };
  }, address);
  // The flight certificate is taken as earned (`colony.spec.ts`): the objectives stay quiet.
  await context.addInitScript(() => localStorage.setItem("bc.settings", "version = 1\nobjectives_done = 2048\n"));
  const page = await context.newPage();
  const logs = collectConsole(page);
  await page.goto(`/?quality=low&gfx=${info.project.name}`);
  await expect(page.locator("#title")).toBeVisible({ timeout: 60_000 });
  await page.locator("#wallet-button").click();
  await expect(page.locator("#wallet-account")).toContainText(address.slice(0, 6));
  await page.locator("#callsign").fill(name);
  await page.locator("#launch-button").click();
  return { page, logs, context };
}

test("two pilots wake at home; one locks and furnishes hers, the other knocks, gets the code and comes in", async ({
  browser,
}, info) => {
  // (Long: two pages drawing the city in software.)
  test.setTimeout(1_800_000);
  expect(SEED.a.id, "the seeded register (scripts/e2e.sh home)").toBeGreaterThan(0);
  const ada = await signIn(browser, 1, "E2E-Ada", info);
  const bo = await signIn(browser, 2, "E2E-Bo", info);
  try {
    // Both wake at home, on foot in the city: no bay, no cap lift.
    await until(ada.page, "Ada awake at home", (s) => s.place === "city" && s.home === SEED.a.id, 180_000);
    await until(bo.page, "Bo awake at home", (s) => s.place === "city" && s.home === SEED.b.id, 180_000);
    await ada.page.screenshot({ path: "artifacts/home-awake.png" });

    // Ada's panel: a code of her own (the colony's key lock was hers alone).
    await push(ada.page, { cmd: "use" });
    await until(ada.page, "Ada's panel", (s) => s.terminal === "home", FRAMES);
    await expect(ada.page.locator("#term-body")).toContainText("ITS PANEL", { timeout: FRAMES });
    await type(ada.page, "home-new-code", "4321");
    await press(ada.page, '[data-act="home-set-code"]');
    await expect(ada.page.locator("#term-log")).toContainText("THE CODE IS SET", { timeout: FRAMES });
    await ada.page.screenshot({ path: "artifacts/home-panel.png" });

    // A sofa from its catalogue, put down a little ahead of her (R turns it till it fits).
    const before = (await bc(ada.page)).home_pieces;
    await press(ada.page, '[data-act="home-place"][data-furniture="sofa"]');
    await until(ada.page, "build mode", (s) => String(s.build).startsWith("sofa"), FRAMES);
    for (let turn = 0; turn < 4 && (await bc(ada.page)).build !== "sofa:fits"; turn++) {
      await ada.page.keyboard.press("r");
      await ada.page.waitForTimeout(1_000);
    }
    await until(ada.page, "a place the sofa fits", (s) => s.build === "sofa:fits", FRAMES);
    await ada.page.screenshot({ path: "artifacts/home-ghost.png" });
    await push(ada.page, { cmd: "use" });
    await until(ada.page, "the sofa put down", (s) => s.home_pieces === before + 1 && s.build === "", FRAMES);
    await ada.page.screenshot({ path: "artifacts/home-sofa.png" });

    // Bo walks out of his flat and along the corridor to her door: shut, and locked.
    await push(bo.page, { cmd: "walk_to", spot: `home:${SEED.a.id}` });
    await until(bo.page, "Bo at Ada's door", (s) => !s.city_walking_to && s.home_door === SEED.a.id, 240_000);
    // On in: the shut door stops him (his own walker, and the server's plaza behind it).
    await push(bo.page, { cmd: "walk_to", spot: `home:${SEED.a.id}:in` });
    await bo.page.waitForTimeout(10_000);
    expect((await bc(bo.page)).home, "kept out by the shut door").not.toBe(SEED.a.id);
    await push(bo.page, { cmd: "walk_to", spot: `home:${SEED.a.id}` });
    await until(bo.page, "Bo back at her door", (s) => !s.city_walking_to && s.home_door === SEED.a.id, FRAMES);

    // Its keypad: a knock Ada hears, a wrong code that shocks, and the right one.
    await push(bo.page, { cmd: "use" });
    await until(bo.page, "Ada's keypad", (s) => s.terminal === "home", FRAMES);
    await expect(bo.page.locator("#term-body")).toContainText("E2E-ADA'S", { timeout: FRAMES });
    await press(bo.page, '[data-act="home-knock"]');
    await until(ada.page, "Ada hearing the knock", (s) => String(s.news).includes("E2E-BO KNOCKS"), FRAMES);
    await type(bo.page, "home-code", "1111");
    await press(bo.page, '[data-act="home-code"]');
    await expect(bo.page.locator("#term-log")).toContainText("WRONG CODE", { timeout: FRAMES });
    await type(bo.page, "home-code", "4321");
    await press(bo.page, '[data-act="home-code"]');
    await expect(bo.page.locator("#term-log")).toContainText("ON ITS CREW", { timeout: FRAMES });
    await bo.page.screenshot({ path: "artifacts/home-keypad.png" });
    await push(bo.page, { cmd: "back" });
    await until(bo.page, "the keypad closed", (s) => s.panel === "none", FRAMES);

    // On its crew, E at the door opens it, and he comes in: her bed and her sofa are there.
    await push(bo.page, { cmd: "use" });
    await push(bo.page, { cmd: "walk_to", spot: `home:${SEED.a.id}:in` });
    await until(bo.page, "Bo in Ada's flat", (s) => s.home === SEED.a.id, 240_000);
    const bo_sees = await until(bo.page, "the sofa and both beds", (s) => s.home_pieces >= 3, FRAMES);
    expect(bo_sees.homes_held).toBe(2);
    await bo.page.screenshot({ path: "artifacts/home-visitor.png" });

    // Ada logs out at home: asleep in her bed, where Bo sees her.
    await ada.context.close();
    await until(bo.page, "Ada asleep in her bed", (s) => String(s.home_asleep).includes("E2E-Ada"), FRAMES);
    await bo.page.screenshot({ path: "artifacts/home-asleep.png" });

    // She signs in again, and wakes beside it.
    const back = await signIn(browser, 1, "E2E-Ada", info);
    try {
      await until(back.page, "Ada awake at home again", (s) => s.place === "city" && s.home === SEED.a.id, 180_000);
      await until(bo.page, "Ada up", (s) => !String(s.home_asleep).includes("E2E-Ada"), FRAMES);
    } finally {
      await back.context.close();
    }
  } catch (e) {
    console.log("--- Ada:\n" + ada.logs.slice(-30).join("\n"));
    console.log("--- Bo:\n" + bo.logs.slice(-30).join("\n"));
    throw e;
  } finally {
    await bo.context.close();
    await ada.context.close().catch(() => {});
  }
});
