import { execFileSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { bc, collectConsole } from "./util";

// Wallet sign-in in the browser. A stub wallet (window.ethereum) signs with a throwaway test key,
// through the same path a real one takes: personal_sign over the text the game writes, checked
// by the server. Then the resume token (a dropped link and a reload come back without asking the
// wallet again), and a second tab taking the pilot over.
const SIGNER = resolve(dirname(fileURLToPath(import.meta.url)), "../../target/release/examples/sign");
// Test key 1's address.
const ADDRESS = "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf";

test("sign in with a wallet, come back on the token, and get taken over", async ({ context, page, request }, info) => {
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
  await other.close();

  const errors = logs.filter((l) => /%cERROR|\[pageerror\]|panicked/.test(l));
  if (errors.length) console.log(errors.join("\n"));
  expect(errors).toEqual([]);
});
