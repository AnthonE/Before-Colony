import { expect, test } from "@playwright/test";
import { bc, collectConsole } from "./util";

// The page around the game, as a player meets it: the title screen, launching, the controls sheet,
// the menu, disconnecting, a link that drops and comes back, and a server that can't be reached.
test("title, launch, menu, reconnect, disconnect", async ({ page, request }, info) => {
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

  // The title: a callsign, the six Gundams (and the Leo), the controls.
  await expect(page.locator("#title")).toBeVisible({ timeout: 60_000 });
  await expect(page.locator("#frames .frame")).toHaveCount(6);
  await expect(page.locator("#controls-list tr").first()).toBeAttached();
  expect(await bc(page)).toMatchObject({ link: "idle" });

  // A server that can't be reached says so, and Retry is offered.
  await page.evaluate(() => {
    (window as any).__realDiscover = (window as any).bcDiscover;
    (window as any).bcDiscover = async () => {
      throw new Error("no route to the sector");
    };
  });
  await page.locator("#callsign").fill("E2E-Title");
  await page.locator("#launch-button").click();
  await wait("the dial to fail", "window.__bc?.link === 'failed'");
  await expect(page.locator("#link-message")).toContainText("Can't reach the server");
  await expect(page.locator("#launch-button")).toHaveText("RETRY");
  await page.evaluate(() => ((window as any).bcDiscover = (window as any).__realDiscover));

  // Launch in the Heavyarms.
  await page.locator('#frames .frame[data-slug="heavyarms"]').click();
  await page.locator("#launch-button").click();
  await wait("in the world", "window.__bc?.link === 'ingame' && window.__bc?.snapshots >= 30");
  await expect(page.locator("#title")).toBeHidden();
  expect((await bc(page)).frame).toBe("heavyarms");
  // Headless Chromium can't lock the pointer, so the prompt stays up.
  await expect(page.locator("#prompt")).toBeVisible();
  const me = async () => {
    const status = await (await request.get("/status")).json();
    return (status.game?.pilots ?? []).find((p: any) => p.name === "E2E-Title");
  };
  expect(await me()).toBeTruthy();

  // F1: the controls sheet, over the world.
  await page.keyboard.press("F1");
  await expect(page.locator("#help")).toBeVisible();
  await page.keyboard.press("F1");
  await expect(page.locator("#help")).toBeHidden();

  // Esc: the menu; Resume closes it.
  await page.keyboard.press("Escape");
  await expect(page.locator("#pause")).toBeVisible();
  expect((await bc(page)).panel).toBe("pause");
  await page.locator('#pause [data-cmd="resume"]').click();
  await expect(page.locator("#pause")).toBeHidden();

  // The link drops: the banner, then back in the world by itself.
  await page.evaluate(() => (window as any).bcInbox.push({ cmd: "drop_link" }));
  await wait("the link to drop", "window.__bc?.link === 'retrying' || window.__bc?.link === 'dialing'");
  await expect(page.locator("#banner")).toBeVisible();
  await wait("the reconnect", "window.__bc?.link === 'ingame' && window.__bc?.reconnects >= 1 && window.__bc?.snapshots >= 10");
  await expect(page.locator("#banner")).toBeHidden();

  // Disconnect from the menu: back to the title, and the server lets the pilot go.
  await page.keyboard.press("Escape");
  await page.locator('#pause [data-cmd="disconnect"]').click();
  await wait("the title", "window.__bc?.link === 'idle'");
  await expect(page.locator("#title")).toBeVisible();
  await expect.poll(me, { timeout: 20_000 }).toBeUndefined();

  const errors = logs.filter((l) => /%cERROR|\[pageerror\]|panicked/.test(l));
  if (errors.length) console.log(errors.join("\n"));
  expect(errors).toEqual([]);
});

// Settings: changed on the panel, kept in this browser across reloads, with keys a newer build
// wrote left alone; and the last launch (callsign, frame) remembered for the title.
test("settings persist across a reload", async ({ page }, info) => {
  test.setTimeout(180_000);
  const saved = () => page.evaluate(() => localStorage.getItem("bc.settings") ?? "");
  await page.goto(`/?gfx=${info.project.name}`);
  await expect(page.locator("#title")).toBeVisible({ timeout: 60_000 });
  await page.evaluate(() => localStorage.setItem("bc.settings", "version = 1\nfov = 70\nfuture_knob = 42\n"));
  await page.reload();
  await expect(page.locator("#title")).toBeVisible({ timeout: 60_000 });

  await page.locator("#title-settings").click();
  await expect(page.locator("#settings")).toBeVisible();
  await page.locator('#settings input[data-key="fov"]').evaluate((el: HTMLInputElement) => {
    el.value = "88";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await page.locator('#settings input[data-key="invert_y"]').check();
  await page.locator('#settings select[data-key="gfx"]').selectOption("medium");
  await page.locator('#settings [data-cmd="settings-close"]').click();
  await expect(page.locator("#settings")).toBeHidden();
  await expect.poll(saved, { timeout: 10_000 }).toContain("fov = 88");
  const text = await saved();
  expect(text).toContain("invert_y = true");
  expect(text).toContain("gfx = medium");
  expect(text).toContain("future_knob = 42");

  await page.reload();
  await expect(page.locator("#title")).toBeVisible({ timeout: 60_000 });
  await page.locator("#title-settings").click();
  await expect(page.locator('#settings input[data-key="fov"]')).toHaveValue("88");
  await expect.poll(async () => (await bc(page)).gfx_tier).toBe("medium");
  await page.keyboard.press("Escape");
  await expect(page.locator("#settings")).toBeHidden();

  await page.locator("#callsign").fill("E2E-Settings");
  await page.locator('#frames .frame[data-slug="sandrock"]').click();
  await page.locator("#launch-button").click();
  await page.waitForFunction("window.__bc?.link === 'ingame'", null, { timeout: 60_000 });
  await expect.poll(saved, { timeout: 10_000 }).toContain("name = E2E-Settings");
  await page.reload();
  await expect(page.locator("#callsign")).toHaveValue("E2E-Settings", { timeout: 60_000 });
  await expect(page.locator("#frames .frame.chosen")).toHaveAttribute("data-slug", "sandrock");
});
