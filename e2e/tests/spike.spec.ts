import { expect, test } from "@playwright/test";
import { bc, collectConsole, luminanceStdDev } from "./util";

// Transport spike: the Bevy client, compiled to wasm, reaches the server over WebTransport
// (self-signed certificate pinned by hash), round-trips datagrams and a control-stream frame.
test("WebTransport echo from the Bevy client", async ({ page }, info) => {
  const logs = collectConsole(page);
  await page.goto(`/?mode=echo&gfx=${info.project.name}`);
  try {
    await page.waitForFunction(() => {
      const s = (window as any).__bc;
      return s?.connected === true && s?.pongs >= 10 && s?.stream_ok === true;
    }, null, { timeout: 120_000 });
  } catch (e) {
    console.log(logs.join("\n"));
    console.log(JSON.stringify(await bc(page)));
    throw e;
  }
  const status = await bc(page);
  console.log(`spike status: ${JSON.stringify(status)}`);
  expect(status.rtt_ms).toBeLessThan(50);
  const shot = await page.screenshot({ path: `artifacts/spike-${info.project.name}.png` });
  expect(luminanceStdDev(shot)).toBeGreaterThan(2);
});
