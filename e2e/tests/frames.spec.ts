import { expect, test } from "@playwright/test";
import { bc, collectConsole, luminanceStdDev } from "./util";

// Every Gundam in a real browser against the server's Mobile Dolls: the kit-aware autopilot
// (`?autopilot=1`) flies each frame for up to three minutes, until the server's counters and the
// client's show its mechanics at work. One page at a time, small and at low quality, since
// SwiftShader renders a frame or two a second.
test.describe.configure({ mode: "serial" });

type Pilot = Record<string, any>;
type Status = Record<string, any>;

// [frame slug, what the server must count for it, what the client must have seen].
const frames: Array<[string, (p: Pilot) => boolean, (s: Status) => boolean, string]> = [
  [
    "heavyarms",
    (p) => p.specials >= 1 && p.missiles >= 4 && p.hits >= 1,
    (s) => s.missiles_seen >= 1 && s.locks_acquired >= 1,
    "Full Open, 4+ missiles and a hit; missiles seen and a lock acquired",
  ],
  [
    "deathscythe",
    (p) => p.specials >= 1 && p.hits_by_class.melee >= 1,
    (s) => s.jamming_seen >= 1,
    "jammed and reaped; the jammer seen",
  ],
  [
    "sandrock",
    (p) => p.missiles >= 2 && p.hits_by_class.missile + p.hits_by_class.melee >= 1,
    (s) => s.missiles_seen >= 1,
    "2+ missiles and a missile or shotel hit; missiles seen",
  ],
  [
    "shenlong",
    (p) => p.hits_by_class.melee + p.hits_by_class.cone >= 1,
    (s) => s.hits_melee + s.hits_cone >= 1,
    "a fang, glaive or flame hit, on both sides",
  ],
  [
    "wingzero",
    (p) => p.specials >= 2 && p.hits >= 1,
    (s) => s.transforms >= 2 && s.zero_ghosts > 0,
    "out as Neo-Bird and back, and a hit; ZERO's futures drawn",
  ],
];

for (const [slug, server, client, what] of frames) {
  test(`${slug}: ${what}`, async ({ page, request }, info) => {
    test.setTimeout(260_000);
    await page.setViewportSize({ width: 640, height: 360 });
    const logs = collectConsole(page);
    const name = `E2E-${slug}`;
    await page.goto(`/?autopilot=1&name=${name}&frame=${slug}&quality=low&gfx=${info.project.name}`);
    await page.waitForFunction("window.__bc?.welcomed && window.__bc?.snapshots >= 30", null, {
      timeout: 60_000,
      polling: 500,
    });
    const me = async (): Promise<[Pilot | undefined, Status]> => {
      const status = await (await request.get("/status")).json();
      return [(status.game?.pilots ?? []).find((p: Pilot) => p.name === name), status];
    };
    const deadline = Date.now() + 180_000;
    let done = false;
    let pilot: Pilot | undefined;
    let seen: Status = {};
    while (Date.now() < deadline) {
      [pilot] = await me();
      seen = await bc(page);
      if (pilot && server(pilot) && client(seen)) {
        done = true;
        break;
      }
      await page.waitForTimeout(3_000);
    }
    const [final, status] = await me();
    console.log(`${slug}: server ${JSON.stringify(final)}`);
    console.log(`${slug}: client ${JSON.stringify(seen)}`);
    if (!done) console.log(logs.slice(-30).join("\n"));
    expect(final?.frame === slug || (slug === "wingzero" && final?.frame === "neobird")).toBe(true);
    expect(done).toBe(true);
    expect(status.game.hot_path_allocations).toBe(0);
    expect(seen.max_snapshot).toBeLessThanOrEqual(1100);
    const shot = await page.screenshot({ path: `artifacts/frames-${info.project.name}-${slug}.png` });
    expect(luminanceStdDev(shot)).toBeGreaterThan(3);
    const errors = logs.filter((l) => /%cERROR|\[pageerror\]|panicked/.test(l));
    if (errors.length) console.log(errors.join("\n"));
    expect(errors).toEqual([]);
  });
}
