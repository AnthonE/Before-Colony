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
  await expect.poll(async () => (await bc(page)).frame).toBe("heavyarms");
  // Headless Chromium can't lock the pointer, so the prompt stays up.
  await expect(page.locator("#prompt")).toBeVisible();
  const me = async () => {
    const status = await (await request.get("/status")).json();
    return (status.game?.pilots ?? []).find((p: any) => p.name === "E2E-Title");
  };
  expect(await me()).toBeTruthy();

  // Sound: the whole bank is built, the context runs (the test browser allows autoplay), and
  // the cockpit has started sounds (the launch, the engines' loop).
  await wait(
    "the sound bank",
    "window.__bc?.audio_state === 'running' && window.__bc?.audio_built === window.__bc?.audio_cues",
  );
  await wait("a sound", "window.__bc?.audio_started > 0");

  // F1: the controls sheet, over the world.
  await page.keyboard.press("F1");
  await expect(page.locator("#help")).toBeVisible();
  await page.keyboard.press("F1");
  await expect(page.locator("#help")).toBeHidden();

  // Tab: into the cockpit and back out to the chase camera. The view is a setting, kept for the
  // next sortie. (A suit shot down meanwhile is watched from behind whatever the setting.)
  const saved = () => page.evaluate(() => localStorage.getItem("bc.settings") ?? "");
  const camera = async () => {
    const s = await bc(page);
    return `${s.camera_view}/${s.alive ? s.camera : s.camera_view}`;
  };
  await page.focus("#bc");
  await page.keyboard.press("Tab");
  await expect.poll(camera).toBe("cockpit/cockpit");
  await expect.poll(saved, { timeout: 10_000 }).toContain("camera = cockpit");
  // From the seat: the monitors round the view carry the instruments.
  await page.waitForTimeout(1_500);
  await page.screenshot({ path: `artifacts/ui-${info.project.name}-cockpit.png` });
  await page.keyboard.press("Tab");
  await expect.poll(camera).toBe("chase/chase");
  await expect.poll(saved, { timeout: 10_000 }).toContain("camera = chase");

  // Anime flight rules (the server's default): the tank is a boost gauge. And an objective with a
  // waypoint: under arcade rules the first is a Doll, and the patrols (or a Doll in sight) mark it.
  expect((await bc(page)).anime).toBe(true);
  await expect.poll(async () => (await bc(page)).objective).toBe("DOWN A MOBILE DOLL");
  await expect.poll(async () => (await bc(page)).waypoint).toMatch(/MOBILE DOLL|PATROLS/);
  await page.screenshot({ path: `artifacts/ui-${info.project.name}-objective.png` });
  // M: the map of the sector, over the world; M again closes it.
  await page.keyboard.press("m");
  await expect.poll(async () => (await bc(page)).map_open).toBe(true);
  await page.waitForTimeout(1_000);
  await page.screenshot({ path: `artifacts/ui-${info.project.name}-map.png` });
  await page.keyboard.press("m");
  await expect.poll(async () => (await bc(page)).map_open).toBe(false);

  // /: the colony's radio. Its line opens, focused; what's typed there doesn't fly the suit; Enter
  // says it, and everyone connected hears it (the speaker too); the server counts it.
  await page.focus("#bc");
  await page.keyboard.press("/");
  await expect(page.locator("#chat-input")).toBeVisible();
  await expect(page.locator("#chat-input")).toBeFocused();
  await page.keyboard.type("o7 from the e2e");
  await page.keyboard.press("Enter");
  await expect(page.locator("#chat-input")).toBeHidden();
  await expect(page.locator("#chat-log")).toContainText("E2E-Title o7 from the e2e");
  const radio = await (await request.get("/status")).json();
  expect(radio.game?.radio_lines).toBe(1);
  // Esc closes it without a word, and without opening the menu.
  await page.focus("#bc");
  await page.keyboard.press("/");
  await expect(page.locator("#chat-input")).toBeFocused();
  await page.keyboard.type("never mind");
  await page.keyboard.press("Escape");
  await expect(page.locator("#chat-input")).toBeHidden();
  await expect(page.locator("#pause")).toBeHidden();
  await expect(page.locator("#chat-log")).not.toContainText("never mind");

  // Esc: the menu; Resume closes it.
  await page.focus("#bc");
  await page.keyboard.press("Escape");
  await expect(page.locator("#pause")).toBeVisible();
  // window.__bc is published four times a second: poll it.
  await expect.poll(async () => (await bc(page)).panel).toBe("pause");
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

// The menu says what leaving does with a signed-in pilot's suit: hidden in a spot, parked where it
// rests, coming down to park from the air in a body's grip, or drifting on. (The page draws what
// it's given: each view is drawn and read back in one go, before the game's next frame.)
test("the menu says what leaving does with the suit", async ({ page }, info) => {
  await page.goto(`/?quality=low&gfx=${info.project.name}`);
  await expect(page.locator("#title")).toBeVisible({ timeout: 60_000 });
  const menu = (extra: Record<string, unknown>) =>
    page.evaluate((extra) => {
      const v = { screen: "playing", panel: "pause", help: false, signedIn: true, place: "space", ...extra };
      (window as any).bcUi.update(v);
      const text = (id: string) => document.getElementById(id)?.textContent ?? "";
      return [text("disconnect-button"), text("pause-who")];
    }, extra);
  const [hidden, hiddenWho] = await menu({ hideSpot: "AFT WELL", survival: true, parkable: true });
  expect(hidden).toBe("LEAVE SUIT HIDDEN");
  expect(hiddenWho).toContain("Hidden in AFT WELL");
  const [parked, parkedWho] = await menu({ parkable: true });
  expect(parked).toBe("PARK & DISCONNECT");
  expect(parkedWho).toContain("parked here");
  const [aloft, aloftWho] = await menu({ aloft: true });
  expect(aloft).toBe("SLEEP & DISCONNECT");
  expect(aloftWho).toContain("parks where it lands");
  expect(aloftWho).not.toContain("drifting");
  const [drift, driftWho] = await menu({});
  expect(drift).toBe("SLEEP & DISCONNECT");
  expect(driftWho).toContain("drifting on");
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
  await page.locator('#settings select[data-key="camera"]').selectOption("cockpit");
  await page.locator('#settings [data-cmd="settings-close"]').click();
  await expect(page.locator("#settings")).toBeHidden();
  await expect.poll(saved, { timeout: 10_000 }).toContain("fov = 88");
  const text = await saved();
  expect(text).toContain("invert_y = true");
  expect(text).toContain("gfx = medium");
  expect(text).toContain("camera = cockpit");
  expect(text).toContain("future_knob = 42");

  await page.reload();
  await expect(page.locator("#title")).toBeVisible({ timeout: 60_000 });
  await page.locator("#title-settings").click();
  await expect(page.locator('#settings input[data-key="fov"]')).toHaveValue("88");
  await expect(page.locator('#settings select[data-key="camera"]')).toHaveValue("cockpit");
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
