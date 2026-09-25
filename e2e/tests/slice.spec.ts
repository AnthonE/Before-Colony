import { expect, test } from "@playwright/test";
import { bc, collectConsole, luminanceStdDev } from "./util";

// The vertical slice in a real browser: the Bevy client connects over WebTransport, the Mobile
// Doll brain flies a Wing Gundam Zero with the ZERO System engaged (`?autopilot=1`, since headless
// browsers cannot lock the pointer), and it must see Mobile Dolls and an AI agent, draw ZERO's
// predicted futures, and land hits that the server confirms.
test("fly, fight, see agents and ZERO futures", async ({ page, request }, info) => {
  test.setTimeout(300_000);
  const logs = collectConsole(page);
  const quality = process.env.BC_QUALITY ? `&quality=${process.env.BC_QUALITY}` : "";
  await page.goto(`/?autopilot=1&name=E2E-Pilot&frame=wingzero&gfx=${info.project.name}${quality}`);
  const step = async (what: string, pred: string, timeout = 120_000) => {
    try {
      await page.waitForFunction(pred, null, { timeout, polling: 500 });
    } catch (e) {
      console.log(`--- waiting for ${what} failed; status: ${JSON.stringify(await bc(page))}`);
      console.log(logs.slice(-40).join("\n"));
      throw e;
    }
    console.log(`ok: ${what}  ${JSON.stringify(await bc(page))}`);
  };
  await step("welcome + 60 snapshots", "window.__bc?.welcomed && window.__bc?.snapshots >= 60");
  await step("20+ contacts on sensors", "window.__bc?.entities >= 20");
  await step("the AI agent, flagged as MD", "window.__bc?.agents_seen >= 1");
  await step("ZERO predicted futures drawn", "window.__bc?.zero_ghosts > 0");
  await page.screenshot({ path: `artifacts/slice-${info.project.name}-zero.png` });

  // A hit by the browser pilot, confirmed by the server.
  const deadline = Date.now() + 150_000;
  let hits = 0;
  while (Date.now() < deadline) {
    const status = await (await request.get("/status")).json();
    const me = (status.game?.pilots ?? []).find((p: any) => p.name === "E2E-Pilot");
    hits = me?.hits ?? 0;
    if (hits >= 1) {
      console.log(`server confirms: ${JSON.stringify(me)}; tick p99 <= ${status.game.tick_us.p99} us; hot-path allocations ${status.game.hot_path_allocations}`);
      expect(status.game.hot_path_allocations).toBe(0);
      break;
    }
    await page.waitForTimeout(2_000);
  }
  if (hits < 1) {
    console.log(JSON.stringify(await bc(page)));
    console.log(logs.slice(-40).join("\n"));
  }
  expect(hits).toBeGreaterThanOrEqual(1);
  const shot = await page.screenshot({ path: `artifacts/slice-${info.project.name}.png` });
  expect(luminanceStdDev(shot)).toBeGreaterThan(5);
  const status = await bc(page);
  console.log(`final: ${JSON.stringify(status)}`);
  expect(status.max_snapshot).toBeLessThanOrEqual(1100);
});
