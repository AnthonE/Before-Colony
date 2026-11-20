import { mkdirSync, readFileSync } from "node:fs";
import { expect, test } from "@playwright/test";
import { bc, collectConsole } from "./util";

// The look, plain beside painted (`scripts/e2e.sh look`): a showcase scene's cameras shot at High,
// once under `?look=plain` and once in the default (painted) look, and one contact sheet of them,
// each camera's pair side by side, in artifacts/look/sheet.png (`docs/RING.md`, "The look").
//
//   BC_LOOK_SCENE=town (default)   BC_LOOK_CAMS=1-8 (default; or a list, `1,3,9`)   BC_LOOK_T=2
const scene = process.env.BC_LOOK_SCENE ?? "town";
const t = Number(process.env.BC_LOOK_T ?? 2);
const frames = 8;
const range = (spec: string) =>
  spec.split(",").flatMap((p) => {
    const [a, b] = p.split("-").map(Number);
    return b ? Array.from({ length: b - a + 1 }, (_, k) => a + k) : [a];
  });
const cams = range(process.env.BC_LOOK_CAMS ?? "1-8");
const looks = [
  ["plain", "look=plain"],
  ["painted", ""],
] as const;
const dir = "artifacts/look";
const shot = (cam: number, look: string) => `${dir}/${scene}-${cam}-${look}.png`;

test.describe.configure({ mode: "serial" });

for (const cam of cams) {
  for (const [look, query] of looks) {
    test(`${scene} cam ${cam} ${look}`, async ({ page }, info) => {
      test.setTimeout(300_000);
      mkdirSync(dir, { recursive: true });
      const logs = collectConsole(page);
      await page.goto(
        `/?showcase=${scene}&cam=${cam}&t=${t}&hold=${frames}&quality=high&gfx=${info.project.name}${query ? `&${query}` : ""}`,
      );
      await page.waitForFunction(`(window.__bc?.showcase_frames ?? 0) >= ${frames}`, null, {
        timeout: 280_000,
        polling: 500,
      });
      expect((await bc(page)).gfx_tier).toBe("high");
      await page.screenshot({ path: shot(cam, look) });
      expect(logs.filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked/i.test(l))).toEqual([]);
    });
  }
}

test(`${scene}: the contact sheet`, async ({ page }) => {
  const img = (path: string) => `data:image/png;base64,${readFileSync(path).toString("base64")}`;
  const rows = cams
    .map(
      (cam) => `<div class="row"><div class="cam">${cam}</div>${looks
        .map(([look]) => `<figure><img src="${img(shot(cam, look))}"><figcaption>${scene} ${cam} · ${look}</figcaption></figure>`)
        .join("")}</div>`,
    )
    .join("");
  await page.setViewportSize({ width: 1000, height: 600 });
  await page.setContent(`<!doctype html><style>
    body { margin: 0; padding: 12px; background: #15171c; color: #d8dde6; font: 13px system-ui, sans-serif; }
    h1 { font-size: 15px; margin: 0 0 10px; font-weight: 600; }
    .row { display: flex; gap: 8px; align-items: center; margin-bottom: 8px; }
    .cam { width: 22px; font-size: 18px; font-weight: 700; text-align: center; }
    figure { margin: 0; }
    img { display: block; width: 480px; height: 270px; }
    figcaption { padding: 3px 0 0; color: #9aa3b2; }
  </style><h1>${scene}: plain (?look=plain) | painted (the default) · High, t ${t} s</h1>${rows}`);
  await page.screenshot({ path: `${dir}/sheet.png`, fullPage: true });
});
