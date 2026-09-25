import { expect, test } from "@playwright/test";
import { bc, collectConsole, luminanceStdDev } from "./util";

// Graphics smoke test: every showcase scene at a given quality tier renders without GPU, shader or
// Rust errors, and puts something on screen. The scenes run offline (no game server) on a fixed
// 60 Hz clock, so the screenshots saved to artifacts/ are reproducible for visual review.
//
//   BC_GFX_QUALITY=high (default) | low | medium | ultra
const quality = process.env.BC_GFX_QUALITY ?? "high";
const scenes: Array<[string, number]> = [
  ["lineup", 1],
  ["lineup", 2],
  ["duel", 1],
  ["colony", 1],
  ["colony", 2],
  ["field", 1],
  ["sky", 1],
  ["sky", 2],
  ["sky", 3],
  ["sky", 4],
];

for (const [scene, cam] of scenes) {
  test(`showcase ${scene} cam ${cam} (${quality})`, async ({ page }, info) => {
    test.setTimeout(240_000);
    const logs = collectConsole(page);
    await page.goto(`/?showcase=${scene}&cam=${cam}&t=6&quality=${quality}&gfx=${info.project.name}`);
    // A few frames past start-up, so pipelines have compiled and effects are on screen.
    await page.waitForFunction("(window.__bc?.showcase_frames ?? 0) >= 12", null, {
      timeout: 200_000,
      polling: 500,
    });
    const status = await bc(page);
    console.log(`${scene}/${cam}: ${JSON.stringify(status)}`);
    expect(status.mode).toBe("showcase");
    expect(status.gfx_tier).toBe(quality);
    const shot = await page.screenshot({ path: `artifacts/gfx-${info.project.name}-${quality}-${scene}-${cam}.png` });
    expect(luminanceStdDev(shot)).toBeGreaterThan(3);
    const bad = logs.filter((l) => /\[error\]|\[pageerror\]|panicked|wgpu error|validation error/i.test(l));
    if (bad.length) console.log(bad.join("\n"));
    expect(bad).toEqual([]);
  });
}
