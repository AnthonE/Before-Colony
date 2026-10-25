// Films the showcase (`?showcase=`) a frame at a time through the client's film hook
// (`window.__bcFilm`, crates/bc-client/src/showcase.rs), in headless Chromium:
//
//   node film.mjs SHOTS.json OUTDIR [name,…]
//
// A shot list (scripts/trailer/shots/*.json):
//   { "fps": 30, "size": [1920, 804], "quality": "high", "shots": [
//     { "name": "duel-beam", "scene": "duel", "cam": 2, "t": 1.2, "len": 2.5,
//       "move": { "yaw": [0, 0.25], "zoom": [1, 0.8], "ease": "io" }, "query": "frame=altair" } ] }
// A shot's frames land in OUTDIR/<name>/001.jpg… (skipped when they are all there already); a shot
// with "len": 0 is a still, OUTDIR/<name>.jpg. "pre" (s) runs the clock that long before `t`
// unfilmed, so an effect fired before the first frame is already in flight. `move` interpolates
// the hook's yaw, pitch, zoom and fwd over the shot (`pan: true` turns about the eye; `follow: i`
// keeps the camera on the scene's suit i). The clock steps 1/fps a frame, so a shot is the same
// however slowly SwiftShader draws it; "warm" is how many frames the client draws first (10), for a
// scene as slow as the city. BC_URL is the page (make.sh serves web/dist).
import { chromium } from "playwright";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";

const [listPath, out, only] = process.argv.slice(2);
if (!listPath || !out) {
  console.error("film.mjs SHOTS.json OUTDIR [name,…]");
  process.exit(2);
}
const list = JSON.parse(readFileSync(listPath, "utf8"));
const url = process.env.BC_URL || "http://127.0.0.1:8080";
const fps = list.fps || 30;
const pick = only ? new Set(only.split(",")) : null;
const ease = {
  lin: (u) => u,
  io: (u) => (u < 0.5 ? 4 * u * u * u : 1 - Math.pow(-2 * u + 2, 3) / 2),
  out: (u) => 1 - Math.pow(1 - u, 3),
  in: (u) => u * u * u,
};
const lerp = (v, u) => (Array.isArray(v) ? v[0] + (v[1] - v[0]) * u : v);

const br = await chromium.launch({
  args: ["--no-proxy-server", "--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"],
});
mkdirSync(out, { recursive: true });
const failed = [];

for (const shot of list.shots) {
  if (pick && !pick.has(shot.name)) continue;
  const still = !shot.len;
  const n = still ? 1 : Math.round(shot.len * fps);
  const pre = Math.round((shot.pre || 0) * fps);
  const dir = `${out}/${shot.name}`;
  if (still ? existsSync(`${out}/${shot.name}.jpg`) : existsSync(dir) && readdirSync(dir).length >= n) {
    console.log(`film: ${shot.name} already shot`);
    continue;
  }
  try {
    await film(shot, still, n, pre, dir);
  } catch (e) {
    // One scene that will not draw (or not in time) is not the whole list's loss: say so, go on.
    console.log(`film: ${shot.name} FAILED: ${String(e.message || e).split("\n")[0]}`);
    failed.push(shot.name);
  }
}
await br.close();
if (failed.length) {
  console.log(`film: not shot: ${failed.join(", ")}`);
  process.exit(1);
}

async function film(shot, still, n, pre, dir) {
  const [w, h] = shot.size || list.size || [1920, 804];
  const page = await br.newPage({ viewport: { width: w, height: h }, deviceScaleFactor: 1 });
  try {
    await filmOn(page, shot, still, n, pre, dir);
  } finally {
    await page.close().catch(() => {});
  }
}

async function filmOn(page, shot, still, n, pre, dir) {
  // A frame of the city takes SwiftShader the best part of a minute; the screenshot waits for it.
  page.setDefaultTimeout(600_000);
  const errors = [];
  page.on("console", (m) => {
    if (/panicked|Caught rendering error|validation error/i.test(m.text())) errors.push(m.text());
  });
  page.on("pageerror", (e) => errors.push(e.message));
  const move = shot.move || {};
  const e = ease[move.ease || "io"];
  const at = (k) => {
    const u = n > 1 ? e(Math.max(0, k - pre) / (n - 1)) : 0;
    return {
      step: k,
      yaw: lerp(move.yaw ?? 0, u),
      pitch: lerp(move.pitch ?? 0, u),
      zoom: lerp(move.zoom ?? 1, u),
      fwd: lerp(move.fwd ?? 0, u),
      pan: !!move.pan,
      ...(move.follow != null ? { follow: move.follow } : {}),
      ...(shot.warm ? { warm: shot.warm } : {}),
    };
  };
  await page.addInitScript((f) => {
    window.__bcFilm = f;
    // The app sleeps while the clock stands where it was asked; a pointer event on the canvas
    // wakes it for one more update.
    window.__bcWake = () =>
      document.getElementById("bc")?.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 2, clientY: 2 }));
    addEventListener("DOMContentLoaded", () => {
      const s = document.createElement("style");
      s.textContent = "#boot{display:none!important}";
      document.head.append(s);
    });
  }, at(pre));
  const q = `showcase=${shot.scene}&cam=${shot.cam || 1}&t=${(shot.t || 0) - pre / fps}&hz=${fps}&quality=${shot.quality || list.quality || "high"}&clean=1${shot.query ? `&${shot.query}` : ""}`;
  const t0 = Date.now();
  await page.goto(`${url}/?${q}`);
  // `every` ms, wake the app again while waiting, in case a frame went unpublished.
  const waitFor = async (expr, every = 10000) => {
    let last = Date.now();
    for (;;) {
      if (errors.length) throw new Error(`${shot.name}: ${errors[0].slice(0, 600)}`);
      if (await page.evaluate(expr)) return;
      if (Date.now() - last > every) {
        await page.evaluate(() => window.__bcWake());
        last = Date.now();
      }
      await page.waitForTimeout(15);
    }
  };
  // Started, run on through the pre-roll and its first frames (the client draws `warm` of them
  // before it sleeps, for its pipelines to compile), with the first frame's camera.
  await waitFor(`(window.__bc?.showcase_frames ?? 0) >= ${shot.warm || 10} && (window.__bc?.showcase_steps ?? -1) >= ${pre}`);
  if (!still) mkdirSync(dir, { recursive: true });
  for (let k = pre; k < pre + n; k++) {
    // The update that takes the clock to step k reads the camera and draws it.
    if (k > pre) {
      await page.evaluate((f) => {
        window.__bcFilm = f;
        window.__bcWake();
      }, at(k));
      await waitFor(`(window.__bc?.showcase_steps ?? -1) >= ${k}`);
    }
    const buf = await page.screenshot({ type: "jpeg", quality: 93 });
    const i = k - pre + 1;
    writeFileSync(still ? `${out}/${shot.name}.jpg` : `${dir}/${String(i).padStart(3, "0")}.jpg`, buf);
    if (i % 30 === 0) console.log(`film: ${shot.name} ${i}/${n}, ${((Date.now() - t0) / 1000).toFixed(0)} s`);
  }
  console.log(`film: ${shot.name} done, ${n} frames in ${((Date.now() - t0) / 1000).toFixed(0)} s`);
}
