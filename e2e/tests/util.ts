import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { PNG } from "pngjs";
import type { Page } from "@playwright/test";

/** A program cargo built for the suites, e.g. `examples/sign`: in BC_BIN_DIR, which scripts/e2e.sh
 *  sets (it follows CARGO_TARGET_DIR), or else the repository's target/release. */
export function built(name: string): string {
  const dir = process.env.BC_BIN_DIR ?? resolve(dirname(fileURLToPath(import.meta.url)), "../../target/release");
  return resolve(dir, name);
}

/** Standard deviation of screenshot luminance: a blank or single-colour frame is ~0. */
export function luminanceStdDev(png: Buffer): number {
  const img = PNG.sync.read(png);
  let sum = 0;
  let sumSq = 0;
  const n = img.width * img.height;
  for (let i = 0; i < img.data.length; i += 4) {
    const l = 0.2126 * img.data[i] + 0.7152 * img.data[i + 1] + 0.0722 * img.data[i + 2];
    sum += l;
    sumSq += l * l;
  }
  const mean = sum / n;
  return Math.sqrt(Math.max(0, sumSq / n - mean * mean));
}

/** Reads the client's `window.__bc` status object. */
export async function bc(page: Page): Promise<Record<string, any>> {
  return (await page.evaluate(() => (window as any).__bc ?? {})) as Record<string, any>;
}

export function collectConsole(page: Page): string[] {
  const lines: string[] = [];
  page.on("console", (m) => lines.push(`[${m.type()}] ${m.text()}`));
  page.on("pageerror", (e) => lines.push(`[pageerror] ${e.message}`));
  return lines;
}

/** An init script for a page that locks the pointer: hands the game each mouse move whole.
 *  Headless Chromium, the pointer locked, fires thousands of empty raw moves a second and hands
 *  each `pointermove` hundreds of thousands of them coalesced; winit makes an event of every one,
 *  and the game's frames slow tenfold and its memory climbs by gigabytes until the wasm aborts.
 *  (`scripts/pt`'s browser does the same.) `page.addInitScript(calmPointer)` before `goto`. */
export const calmPointer = () => {
  if (window.PointerEvent?.prototype.getCoalescedEvents) {
    PointerEvent.prototype.getCoalescedEvents = function () {
      return [this];
    };
  }
};

/** The wallet button pressed, and `name`'s row chosen if the page asks which wallet; then waits
 *  for `account` (the start of its address, or a name) to show as signed in. The page asks when
 *  the browser has more than one: the stub wallet and, whenever the page reaches elopros.com's
 *  `wallet-config.js` with the phone wallet switched on, a phone wallet's row (`web/ui.js`). Waits
 *  till it has either asked or connected, so a slow page's question isn't missed. */
export async function connectWallet(page: Page, name: RegExp, account: string, timeout = 60_000) {
  await page.locator("#wallet-button").click();
  await page.waitForFunction(
    (acc) =>
      !!document.querySelector("#wallet-pick")?.hasAttribute("open") ||
      (document.querySelector("#wallet-account")?.textContent ?? "").includes(acc),
    account,
    { timeout, polling: 250 },
  );
  if (await page.locator("#wallet-pick").evaluate((d) => d.hasAttribute("open"))) {
    await page.locator("#wallet-list button", { hasText: name }).click({ timeout });
  }
  await page.waitForFunction(
    (acc) => (document.querySelector("#wallet-account")?.textContent ?? "").includes(acc),
    account,
    { timeout, polling: 250 },
  );
}

/** An init script: every status the client publishes to `window.__bc`, in order, the newest 400
 *  kept in `window.__bcLog` and all counted in `window.__bcCount`. The client publishes a frame at a
 *  time, and on a page drawing in software a state can stand for one frame (the ride down a cap
 *  lift, a drill's target struck) between two of a suite's looks: `seen` finds it here.
 *  `page.addInitScript(keepStatus)` before `goto`. */
export const keepStatus = () => {
  const w = window as any;
  let now: unknown;
  w.__bcLog = [];
  w.__bcCount = 0;
  Object.defineProperty(w, "__bc", {
    configurable: true,
    get: () => now,
    set: (v: unknown) => {
      now = v;
      w.__bcLog.push(v);
      if (w.__bcLog.length > 400) w.__bcLog.shift();
      w.__bcCount += 1;
    },
  });
};

/** How many statuses the page has published (`keepStatus`): taken before a step, it's where
 *  `seen` starts looking. */
export async function statusMark(page: Page): Promise<number> {
  return page.evaluate(() => (window as any).__bcCount ?? 0);
}

/** The first status the page published since `mark` (`statusMark`) that `pred` holds for, however
 *  briefly it stood (`keepStatus`), waited for up to `ms`. */
export async function seen(
  page: Page,
  what: string,
  pred: (s: Record<string, any>) => boolean,
  ms: number,
  mark: number,
): Promise<Record<string, any>> {
  const t0 = Date.now();
  let from = mark;
  for (;;) {
    const { states, count } = await page.evaluate((from) => {
      const w = window as any;
      const log: unknown[] = w.__bcLog ?? [];
      const count: number = w.__bcCount ?? 0;
      return { states: log.slice(Math.max(0, log.length - (count - from))), count };
    }, from);
    const hit = (states as Record<string, any>[]).find(pred);
    if (hit) return hit;
    from = count;
    if (Date.now() - t0 > ms) throw new Error(`timed out waiting for ${what}: ${JSON.stringify(await bc(page))}`);
    await page.waitForTimeout(250);
  }
}
