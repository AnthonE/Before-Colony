import { chromium, expect, test, type Page } from "@playwright/test";
import { bc, collectConsole } from "./util";

// The base's deck in the hub, in the browser (`scripts/e2e.sh base`: survival, the retired
// cylinder's inside shut as it is by default, no drones; `docs/RING.md`, "The base"). A pilot walks
// to their bay's airlock and out onto Bay Row, crosses the row into the test hall to its counter,
// and goes back to their airlock and in. Two pilots in two browsers step out of their bays and each
// sees the other on the row. The walking is the dev hook's (a guide walks the pilot's own legs).

const push = (page: Page, cmd: Record<string, unknown>) =>
  page.evaluate((c) => ((window as any).bcInbox ||= []).push(c), cmd);

async function until(
  page: Page,
  what: string,
  pred: (s: Record<string, any>) => boolean,
  ms: number,
): Promise<Record<string, any>> {
  const t0 = Date.now();
  for (;;) {
    const s = await bc(page);
    if (pred(s)) return s;
    if (Date.now() - t0 > ms) throw new Error(`timed out waiting for ${what}: ${JSON.stringify(s)}`);
    await page.waitForTimeout(250);
  }
}

/** Where on the deck the pilot stands: along the axis, round the deck, up. */
const deckFeet = (s: Record<string, any>) => String(s.deck_feet ?? "").split(",").map(Number);

/** Where a pilot named `name` is seen, if they are: `[x, s, h]`. */
function seenAt(s: Record<string, any>, name: string): number[] | undefined {
  const hit = String(s.people_at ?? "")
    .split(";")
    .find((p) => p.startsWith(`${name}@`));
  return hit?.slice(name.length + 1).split(",").map(Number);
}

/** Out through the bay's airlock onto Bay Row. */
async function outOntoBayRow(page: Page) {
  await push(page, { cmd: "walk_to", spot: "airlock" });
  await until(page, "at the airlock", (s) => s.focus === "airlock" && !s.walking_to, 180_000);
  await expect(page.locator("#use")).toContainText("BAY ROW");
  await push(page, { cmd: "use" });
  return until(page, "Bay Row", (s) => s.place === "base" && s.seq === "walking" && s.deck_feet !== "", 120_000);
}

const errors = (logs: string[]) => logs.filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked/i.test(l));

test("a pilot steps out onto Bay Row, into the test hall, and back into their bay", async ({ page }) => {
  test.setTimeout(900_000);
  const logs = collectConsole(page);
  await page.goto("/?autoplay=1&name=Tester&quality=low");
  await until(page, "the bay", (s) => s.place === "hangar" && s.seq === "walking", 240_000);

  // A pace out of their airlock's door, in the deck's back end (x −602): on the floor, and Bay Row
  // under them.
  let s = await outOntoBayRow(page);
  const [x0, , h0] = deckFeet(s);
  expect(x0).toBeGreaterThan(-601);
  expect(x0).toBeLessThan(-596);
  expect(Math.abs(h0)).toBeLessThan(0.2);
  await expect(page.locator("#disconnect-button")).toHaveCount(1);
  let status = await (await page.request.get("/status")).json();
  expect(status.game.base.people).toBe(1);
  expect(status.game.hangars.find((h: any) => h.name === "Tester")?.place).toBe("base");
  await page.screenshot({ path: "artifacts/base-bay-row.png" });

  // Across the row, in at the test hall's door, to its counter (x −574, 1269.5 m round).
  await push(page, { cmd: "walk_to", spot: "hall" });
  s = await until(
    page,
    "the hall's counter",
    (s) => {
      const [x, r] = deckFeet(s);
      return !s.deck_walking_to && Math.hypot(x + 574, r - 1269.5) < 1.0;
    },
    240_000,
  );
  console.log(`at the hall's counter: ${s.deck_feet}`);
  await page.screenshot({ path: "artifacts/base-hall.png" });

  // Back to their airlock, and in.
  await push(page, { cmd: "walk_to", spot: "airlock" });
  await until(page, "back at the airlock", (s) => s.focus === "airlock" && !s.deck_walking_to, 240_000);
  await expect(page.locator("#use")).toContainText("BACK INTO YOUR BAY");
  await push(page, { cmd: "use" });
  await until(page, "the bay again", (s) => s.place === "hangar" && s.seq === "walking", 120_000);
  status = await (await page.request.get("/status")).json();
  expect(status.game.base.people).toBe(0);
  expect(status.game.hot_path_allocations).toBe(0);
  const bad = errors(logs);
  if (bad.length) console.log(bad.join("\n"));
  expect(bad).toEqual([]);
});

test("two pilots step out onto Bay Row and see each other there", async ({ browser }) => {
  test.setTimeout(1_200_000);
  // Two browsers, two pilots, small windows (two software renderers share the machine; the second
  // is a browser of its own, as `colony`'s two are).
  const small = { viewport: { width: 640, height: 360 } };
  const second = await chromium.launch(test.info().project.use.launchOptions ?? {});
  const [ca, cb] = [await browser.newContext(small), await second.newContext(small)];
  const [a, b] = [await ca.newPage(), await cb.newPage()];
  const logs = [collectConsole(a), collectConsole(b)];
  const inBay = async (p: Page, name: string) => {
    for (let k = 0; k < 3; k++) {
      await p.goto(`/?autoplay=1&name=${name}&quality=low`);
      const s = await until(
        p,
        "the bay",
        (s) => (s.place === "hangar" && s.seq === "walking") || s.link === "failed",
        240_000,
      );
      if (s.link !== "failed") return;
      console.log(`${name}: the link failed (${s.last_error}); loading again`);
    }
    throw new Error(`${name} never got in`);
  };
  await inBay(a, "Kai");
  await inBay(b, "Nico");
  await Promise.all([outOntoBayRow(a), outOntoBayRow(b)]);

  // Each sees the other, by name, where the other stands (within 2 m), out of their own airlock.
  for (const [me, other, them] of [
    [a, b, "Nico"],
    [b, a, "Kai"],
  ] as const) {
    const feet = deckFeet(await bc(other));
    const near = (s: Record<string, any>) => {
      const seen = seenAt(s, them);
      return seen !== undefined && Math.hypot(seen[0] - feet[0], seen[1] - feet[1]) < 2;
    };
    const s = await until(me, `${them} seen on Bay Row at ${feet}`, near, 120_000);
    console.log(`${them}: seen at ${seenAt(s, them)}, stands at ${feet}`);
  }
  await a.screenshot({ path: "artifacts/base-two-pilots.png" });
  const status = await (await a.request.get("/status")).json();
  expect(status.game.base.people).toBe(2);

  // Nico goes back in; Kai sees him gone from the row.
  await push(b, { cmd: "walk_to", spot: "airlock" });
  await until(b, "at his airlock", (s) => s.focus === "airlock" && !s.deck_walking_to, 240_000);
  await push(b, { cmd: "use" });
  await until(b, "his bay", (s) => s.place === "hangar" && s.seq === "walking", 120_000);
  await until(a, "Nico gone", (s) => seenAt(s, "Nico") === undefined, 60_000);
  for (const l of logs) {
    const bad = errors(l);
    if (bad.length) console.log(bad.join("\n"));
    expect(bad).toEqual([]);
  }
  await second.close();
});
