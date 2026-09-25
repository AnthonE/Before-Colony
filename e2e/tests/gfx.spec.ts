import { expect, test } from "@playwright/test";
import { bc, collectConsole, luminanceStdDev } from "./util";

// Graphics smoke test: every showcase scene at a given quality tier renders without GPU, shader or
// Rust errors, and puts something on screen. The scenes run offline (no game server) on a fixed
// 60 Hz clock, so the screenshots saved to artifacts/ are reproducible for visual review.
//
//   BC_GFX_QUALITY=high (default) | low | medium | ultra
const quality = process.env.BC_GFX_QUALITY ?? "high";
// [scene, camera preset, start time (s), frames to run]. The clock stops after the last frame
// (`hold`), so each screenshot shows one exact moment; effects need their triggering event inside
// the run.
const scenes: Array<[string, number, number, number]> = [
  ["lineup", 1, 6, 12],
  ["lineup", 2, 6, 12],
  // The Taurus exploding as Wing Zero's shot lands, with the Leo's machine-cannon tracers.
  ["duel", 1, 6.75, 30],
  // The Twin Buster Rifle mid-shot.
  ["duel", 2, 1.85, 12],
  // Beam sabers clashing.
  ["duel", 3, 4.4, 12],
  ["colony", 1, 6, 12],
  ["colony", 2, 6, 12],
  ["field", 1, 6, 12],
  ["sky", 1, 6, 12],
  ["sky", 2, 6, 12],
  ["sky", 3, 6, 12],
  ["sky", 4, 6, 12],
  // The pilot's view: boosting; hit; greying out; ZERO engaged; ZERO's seizure.
  ["chase", 1, 4.5, 12],
  ["chase", 1, 5.95, 12],
  ["chase", 1, 9.9, 12],
  ["chase", 1, 15.5, 12],
  ["chase", 1, 17.8, 12],
];

for (const [scene, cam, t, frames] of scenes) {
  test(`showcase ${scene} cam ${cam} t ${t} (${quality})`, async ({ page }, info) => {
    test.setTimeout(240_000);
    const logs = collectConsole(page);
    await page.goto(
      `/?showcase=${scene}&cam=${cam}&t=${t}&hold=${frames}&quality=${quality}&gfx=${info.project.name}`,
    );
    // A few frames past start-up, so pipelines have compiled and effects are on screen. A GPU
    // validation error or a panic stops the app, so fail on one at once rather than time out.
    // (Bevy logs its errors, a shader that won't compile among them, through console.log.)
    const broken = new Promise<string>((resolve) =>
      page.on("console", (m) => {
        if (/Caught rendering error|panicked|%cERROR/.test(m.text())) resolve(m.text());
      }),
    );
    const ready = page
      .waitForFunction(`(window.__bc?.showcase_frames ?? 0) >= ${frames}`, null, {
        timeout: 200_000,
        polling: 500,
      })
      .then(() => "");
    const error = await Promise.race([ready, broken]);
    expect(error.slice(0, 800)).toBe("");
    const status = await bc(page);
    console.log(`${scene}/${cam}: ${JSON.stringify(status)}`);
    expect(status.mode).toBe("showcase");
    expect(status.gfx_tier).toBe(quality);
    const name = `gfx-${info.project.name}-${quality}-${scene}-${cam}-t${t}.png`;
    const shot = await page.screenshot({ path: `artifacts/${name}` });
    expect(luminanceStdDev(shot)).toBeGreaterThan(3);
    const bad = logs.filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked|wgpu error|validation error/i.test(l));
    if (bad.length) console.log(bad.join("\n"));
    expect(bad).toEqual([]);
  });
}
