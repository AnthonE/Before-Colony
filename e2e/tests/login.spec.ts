import { execFileSync } from "node:child_process";
import { createPrivateKey, sign as edSign } from "node:crypto";
import { expect, test, type Page } from "@playwright/test";
import { bc, built, calmPointer, collectConsole } from "./util";

// Each move whole: the pointer locks here (`util.ts`, `calmPointer`).
test.beforeEach(({ page }) => page.addInitScript(calmPointer));

// Wallet sign-in in the browser. A stub wallet (window.ethereum) signs with a throwaway test key,
// through the same path a real one takes: personal_sign over the text the game writes, checked
// by the server. Then the resume token (a dropped link and a reload come back without asking the
// wallet again), a second tab taking the pilot over, and sleeping in the cockpit: leaving signed
// in keeps the suit in the sector, and coming back wakes in it.
const SIGNER = built("examples/sign");
// With the phone wallet switched on (the platform's `wallet-config.js`, when the origin answers) a
// second row makes the picker open: choose in it.
const pick = async (page: Page, name: RegExp) => {
  const row = page.locator("#wallet-list button", { hasText: name });
  if (await row.waitFor({ timeout: 5_000 }).then(() => true, () => false)) await row.click({ timeout: 60_000 });
};
// Test key 1's address.
const ADDRESS = "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf";

test("sign in with a wallet, come back on the token, get taken over, sleep and wake", async ({ context, page, request }, info) => {
  test.setTimeout(300_000);
  let signatures = 0;
  let decline = true;
  await context.exposeFunction("__signHex", (hex: string) => {
    if (decline) {
      decline = false;
      return { error: "User rejected the request." };
    }
    signatures += 1;
    const message = Buffer.from(hex.slice(2), "hex");
    return { signature: execFileSync(SIGNER, ["1"], { input: message }).toString().trim() };
  });
  await context.addInitScript((address) => {
    (window as any).ethereum = {
      async request({ method, params }: { method: string; params?: any[] }) {
        if (method === "eth_accounts" || method === "eth_requestAccounts") return [address];
        if (method === "personal_sign") {
          const out = await (window as any).__signHex(params![0]);
          // Wallets reject with a plain object ({ code, message }), not an Error.
          if (out.error) throw { code: 4001, message: out.error };
          return out.signature;
        }
        throw { code: 4200, message: `unsupported: ${method}` };
      },
      on() {},
    };
  }, ADDRESS);

  const logs = collectConsole(page);
  const waitOn = async (p: Page, what: string, pred: string, timeout = 60_000) => {
    try {
      await p.waitForFunction(pred, null, { timeout, polling: 250 });
    } catch (e) {
      console.log(`--- waiting for ${what} failed; status: ${JSON.stringify(await bc(p))}`);
      console.log(logs.slice(-30).join("\n"));
      throw e;
    }
  };
  const me = async () => {
    const status = await (await request.get("/status")).json();
    return (status.game?.pilots ?? []).find((p: any) => p.name === "E2E-Wallet");
  };

  await page.goto(`/?quality=low&gfx=${info.project.name}`);
  await expect(page.locator("#title")).toBeVisible({ timeout: 60_000 });
  await page.locator("#wallet-button").click();
  await pick(page, /Browser wallet/);
  await expect(page.locator("#wallet-account")).toContainText("0x7e5f");
  await page.locator("#callsign").fill("E2E-Wallet");

  // The first time, the pilot declines in the wallet: it says so, and doesn't retry by itself.
  await page.locator("#launch-button").click();
  await waitOn(page, "the refusal", "window.__bc?.link === 'failed'");
  await expect(page.locator("#link-message")).toContainText("User rejected the request");

  // Then signs.
  await page.locator("#launch-button").click();
  await waitOn(page, "signed in", "window.__bc?.link === 'ingame' && window.__bc?.signed_in && window.__bc?.resume_token");
  expect(signatures).toBe(1);
  await expect.poll(async () => (await me())?.verified, { timeout: 10_000 }).toBe(true);
  expect((await me()).address).toBe("0x7e5f…5bdf");
  await page.keyboard.press("Escape");
  await expect(page.locator("#pause-who")).toContainText("Signed in as 0x7e5f");
  await page.locator('#pause [data-cmd="resume"]').click();

  // The link drops: back in on the resume token, the wallet not asked.
  await page.evaluate(() => (window as any).bcInbox.push({ cmd: "drop_link" }));
  await waitOn(page, "the reconnect", "window.__bc?.link === 'ingame' && window.__bc?.reconnects >= 1 && window.__bc?.signed_in");
  expect(signatures).toBe(1);

  // A reload: the wallet is remembered, and this tab's token signs it straight back in.
  await page.reload();
  await expect(page.locator("#wallet-account")).toContainText("0x7e5f", { timeout: 60_000 });
  await page.locator("#launch-button").click();
  await waitOn(page, "signed in again", "window.__bc?.link === 'ingame' && window.__bc?.signed_in");
  expect(signatures).toBe(1);

  // Another tab signs in as the same pilot: it takes over, and this one lets go for good.
  const other = await context.newPage();
  await other.goto(`/?quality=low&gfx=${info.project.name}`);
  await expect(other.locator("#wallet-account")).toContainText("0x7e5f", { timeout: 60_000 });
  await other.locator("#callsign").fill("E2E-Wallet");
  await other.locator("#launch-button").click();
  await waitOn(other, "the other tab in", "window.__bc?.link === 'ingame' && window.__bc?.signed_in");
  expect(signatures).toBe(2);
  await waitOn(page, "this tab let go", "window.__bc?.link === 'failed'");
  await expect(page.locator("#link-message")).toContainText("signed in somewhere else");
  await page.waitForTimeout(3_000);
  expect((await bc(page)).link).toBe("failed");

  // The other tab took over the same suit: the first tab's went to sleep, and it woke in it.
  expect((await bc(other)).woke).toBe(true);
  const suit = async (p: Page) => {
    const b = await bc(p);
    return [b.own_slot, b.own_generation];
  };
  const mine = await suit(other);
  expect(mine[0]).toBeGreaterThanOrEqual(0);

  // Leaving signed in: the suit stays in the sector, its pilot asleep in the cockpit.
  await other.keyboard.press("Escape");
  await expect(other.locator("#pause")).toBeVisible();
  await expect(other.locator("#disconnect-button")).toHaveText("SLEEP & DISCONNECT");
  await other.locator("#disconnect-button").click();
  await waitOn(other, "the title", "window.__bc?.link === 'idle'");
  const asleep = async () => {
    const status = await (await request.get("/status")).json();
    return (status.game?.sleepers ?? []).find((p: any) => p.name === "E2E-Wallet");
  };
  await expect.poll(asleep, { timeout: 10_000 }).toBeTruthy();
  expect((await asleep()).suit).toBe(mine[0]);

  // Back: awake in the same suit, without the wallet asked again.
  await other.locator("#launch-button").click();
  await waitOn(other, "awake", "window.__bc?.link === 'ingame' && window.__bc?.woke === true");
  await expect(other.locator("#toast")).toContainText("YOU WAKE IN YOUR COCKPIT");
  await expect.poll(() => suit(other)).toEqual(mine);
  expect(await asleep()).toBeUndefined();
  expect(signatures).toBe(2);
  await other.close();

  const errors = logs.filter((l) => /%cERROR|\[pageerror\]|panicked/.test(l));
  if (errors.length) console.log(errors.join("\n"));
  expect(errors).toEqual([]);
});

// A player with only a Solana wallet, and one with only an Elo Pros login. A stub Phantom signs with
// `signin_check.json`'s throwaway key (Ed25519 seed 07…07); a stub `EloSignIn` mints a pass with Elo
// Pros' test key (`account_token.json`, its `seed_hex`; `scripts/e2e.sh` names its public half).
const SOL = "GmaDrppBC7P5ARKV8g3djiwP89vz1jLK23V2GBjuAEGB";
const ACCT = "acct_0123456789abcdef0123";
const ed25519 = (seedHex: string) =>
  createPrivateKey({
    key: Buffer.concat([Buffer.from("302e020100300506032b657004220420", "hex"), Buffer.from(seedHex, "hex")]),
    format: "der",
    type: "pkcs8",
  });
const b64url = (b: Buffer) => b.toString("base64url");
function mintPass(game: string, nonce: string) {
  const text = `elo account\nid: ${ACCT}\nfor: play:${game}\nshard: 127.0.0.1\nnonce: ${nonce}\nwallet: none\nexp: ${Math.floor(Date.now() / 1000) + 600}`;
  const key = ed25519("2f3e65db2d7c08d3d45cb88040eca8badb2b6287066b2d30a1861756938b6536");
  return `${b64url(Buffer.from(text))}.${b64url(edSign(null, Buffer.from(text), key))}`;
}
const onlineWallets = async (request: any) =>
  ((await (await request.get("/online.json")).json()).online as any[]).map((e) => e.wallet);

test("sign in with only a Solana wallet", async ({ context, page, request }, info) => {
  test.setTimeout(180_000);
  await context.exposeFunction("__solSign", (hex: string) =>
    Array.from(edSign(null, Buffer.from(hex, "hex"), ed25519("07".repeat(32)))),
  );
  await context.addInitScript((sol) => {
    const pk = { toString: () => sol };
    (window as any).phantom = {
      solana: {
        isPhantom: true,
        publicKey: null as any,
        async connect() {
          this.publicKey = pk;
          return { publicKey: pk };
        },
        async signMessage(bytes: Uint8Array) {
          const hex = Array.from(bytes).map((b) => b.toString(16).padStart(2, "0")).join("");
          return { signature: Uint8Array.from(await (window as any).__solSign(hex)), publicKey: pk };
        },
      },
    };
  }, SOL);
  await page.goto(`/?quality=low&gfx=${info.project.name}`);
  await expect(page.locator("#title")).toBeVisible({ timeout: 60_000 });
  await page.locator("#wallet-button").click();
  await pick(page, /Phantom/);
  await expect(page.locator("#wallet-account")).toContainText("GmaDrp");
  await page.locator("#callsign").fill("E2E-Sol");
  await page.locator("#launch-button").click();
  await page.waitForFunction("window.__bc?.link === 'ingame' && window.__bc?.signed_in", null, { timeout: 60_000 });
  expect(await onlineWallets(request)).toContain(SOL);
});

test("sign in with only an Elo Pros login, and come back on the token", async ({ context, page, request }, info) => {
  test.setTimeout(180_000);
  await context.exposeFunction("__mintPass", mintPass);
  await context.addInitScript(() => {
    (window as any).EloSignIn = {
      popup: async ({ game, nonce }: { game: string; nonce: string }) => ({ pass: await (window as any).__mintPass(game, nonce) }),
    };
  });
  await page.goto(`/?quality=low&gfx=${info.project.name}`);
  await expect(page.locator("#title")).toBeVisible({ timeout: 60_000 });
  await page.locator("#elo-button").click();
  await expect(page.locator("#wallet-account")).toContainText("Elo Pros account");
  await page.locator("#callsign").fill("E2E-Acct");
  await page.locator("#launch-button").click();
  await page.waitForFunction("window.__bc?.link === 'ingame' && window.__bc?.signed_in", null, { timeout: 60_000 });
  expect(await onlineWallets(request)).toContain(ACCT);

  // A reload needs no second pass: the token this tab kept.
  await page.waitForFunction("sessionStorage.getItem('bc.resume')", null, { timeout: 15_000 });
  await page.reload();
  await expect(page.locator("#wallet-account")).toContainText("Elo Pros account", { timeout: 60_000 });
  await page.locator("#launch-button").click();
  await page.waitForFunction("window.__bc?.link === 'ingame' && window.__bc?.signed_in", null, { timeout: 60_000 });
  expect(await onlineWallets(request)).toContain(ACCT);
});
