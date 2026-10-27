import { expect, test } from "@playwright/test";
import { bc, collectConsole, luminanceStdDev } from "./util";

// Graphics smoke test: every showcase scene at a given quality tier renders without GPU, shader or
// Rust errors, and puts something on screen. The scenes run offline (no game server) on a fixed
// 60 Hz clock, so the screenshots saved to artifacts/ are reproducible for visual review.
//
//   BC_GFX_QUALITY=high (default) | low | medium | ultra
const quality = process.env.BC_GFX_QUALITY ?? "high";
// Extra query parameters for every shot (say `look=0` for the plain look, `hz=20` for a slower
// clock), which also go into the screenshots' names so pairs sit side by side.
const extra = process.env.BC_GFX_EXTRA ?? "";
const suffix = extra ? `-${extra.replace(/[^a-z0-9=]+/gi, "_")}` : "";
// [scene, camera preset, start time (s), frames to run]. The clock stops after the last frame
// (`hold`), so each screenshot shows one exact moment; effects need their triggering event inside
// the run.
const scenes: Array<[string, number, number, number]> = [
  ["lineup", 1, 6, 12],
  ["lineup", 2, 6, 12],
  // The Corvus exploding as Altair's shot lands, with the Argo's machine-cannon tracers.
  ["duel", 1, 6.75, 30],
  // Two seconds on: the Corvus's wreck drifting on out of its blast, burning, trailing smoke.
  ["duel", 1, 6.9, 120],
  // The Heavy Beam Rifle mid-shot.
  ["duel", 2, 1.85, 12],
  // Beam sabers clashing.
  ["duel", 3, 4.4, 12],
  ["colony", 1, 6, 12],
  ["colony", 2, 6, 12],
  // The dock's ring of lights, off the docking hub.
  ["colony", 5, 6, 12],
  // The hub's end: the drum's face and its bays, the spine and the ring beyond in their lamps;
  // the same at night; the bays' doors close to; the louvres opening in the early morning.
  ["colony", 6, 6, 12],
  ["colony", 6, 1900, 12],
  ["colony", 7, 6, 12],
  ["colony", 8, 2620, 12],
  // The launch shot: an Argo thrown out of its bay's door on the hub's face, the door behind it.
  ["colony", 9, 6.4, 12],
  ["field", 1, 6, 12],
  // Wreckage after a fight: hulks, limbs shot off, loose ore.
  ["salvage", 1, 6, 12],
  ["salvage", 2, 6, 12],
  // Mining: a saber cutting into a cracked rock; the rock shattering into ore.
  ["mining", 1, 7.95, 18],
  ["mining", 2, 9.7, 12],
  ["sky", 1, 6, 12],
  ["sky", 2, 6, 12],
  ["sky", 3, 6, 12],
  ["sky", 4, 6, 12],
  // The pilot's view: boosting; hit; greying out; the Orrery engaged; the Orrery's seizure.
  ["chase", 1, 4.5, 12],
  ["chase", 1, 5.95, 12],
  ["chase", 1, 9.9, 12],
  ["chase", 1, 15.5, 12],
  ["chase", 1, 17.8, 12],
  // The same from the cockpit (the head's main camera): boosting; hit; the Orrery engaged.
  ["chase", 2, 4.5, 12],
  ["chase", 2, 5.95, 12],
  ["chase", 2, 15.5, 12],
  // The Lodestars: all in a row (Full Salvo's salvo bursting, Thuban's flame); Antares in Full
  // Salvo; Algol jamming, mid-reap; Canopus's blades, then its Pincer Strike; Thuban's
  // fang at full reach, then its flamethrower; Aquila on full burn.
  ["lodestars", 1, 7.95, 12],
  ["lodestars", 2, 7.95, 12],
  ["lodestars", 3, 6.75, 12],
  ["lodestars", 4, 6.45, 12],
  ["lodestars", 4, 9.45, 12],
  ["lodestars", 5, 6.63, 12],
  ["lodestars", 5, 7.9, 12],
  ["lodestars", 6, 6, 12],
  // The Almagest's prototype pair: the Corona's field holding as the Argo's rifle shots flash on
  // it; side on, the Sagittarius charging inside it; the Sagittarius close as it fires; the
  // Corona's racks from behind, its ward discs on their way out. (The discs fly and the arms come
  // up over the frames run, so each run starts early enough for them.)
  ["pair", 1, 7.0, 45],
  ["pair", 2, 7.3, 42],
  ["pair", 3, 7.5, 43],
  ["pair", 4, 6.3, 33],
  // The hangar bay: just in from the airlock; from its back corner with the doors opening; from
  // the cockpit, down the launch tunnel with the doors open.
  ["hangar", 1, 2, 12],
  ["hangar", 4, 6, 12],
  ["hangar", 5, 8.5, 12],
  // The Muster, the base in the hub's drum: the test hall from out on the deck, in at its doorway,
  // from its gallery; the deck rising past it till it's overhead; along Bay Row's airlocks.
  ["base", 1, 2, 8],
  ["base", 2, 2, 8],
  ["base", 3, 2, 8],
  ["base", 4, 2, 8],
  ["base", 5, 2, 8],
  // Suits on the landmarks: an Argo walking Fornax's core as the station rolls, rolling onto its toes
  // as each foot pushes off; one kneeling asleep in the Aft Well, its rim lights round it, the back
  // foot's toes tucked under; one lifting off Hermit on the verniers in its soles; one landing on
  // Hermit in a puff of dust. (The walker's stride needs a moment to get going.)
  ["surface", 1, 2, 45],
  ["surface", 2, 4, 20],
  ["surface", 3, 1.5, 20],
  ["surface", 3, 6, 20],
  // Inside the colony: down the avenue from Hub Gate (and at night), from the cap lift, downtown
  // at eye height (and at noon, its traffic and people at their busiest), at a window bank, along
  // the canal, from near the axis down the length, a tram station's platform, and in from the
  // Exchange floor's door to its boards.
  ["city", 1, 6, 4],
  ["city", 1, 1900, 4],
  ["city", 2, 6, 4],
  ["city", 3, 6, 4],
  ["city", 3, 480, 4],
  ["city", 4, 6, 4],
  ["city", 5, 6, 4],
  ["city", 6, 6, 4],
  ["city", 7, 6, 4],
  ["city", 8, 6, 4],
  // A Markab, the rocket car, in the Blast Hall: rolling up the back wall, behind it and from the
  // doors; hanging from the ceiling upside down, from under it; coming back down the wall tail
  // first; out of the blast doors in a barrel roll over Hub Gate's square, from out there and
  // behind it.
  ["markab", 1, 5.6, 4],
  ["markab", 2, 6.5, 4],
  ["markab", 3, 12, 4],
  ["markab", 5, 24, 4],
  ["markab", 4, 33.9, 4],
  ["markab", 1, 34.1, 4],
];
// What the Markab's driver is doing at each of those moments (`showcase_markab::Act`).
const markabActs: Record<string, string> = {
  "5.6": "up",
  "6.5": "up",
  "12": "hang",
  "24": "back",
  "33.9": "roll",
  "34.1": "roll",
};

for (const [scene, cam, t, frames] of scenes) {
  test(`showcase ${scene} cam ${cam} t ${t} (${quality})`, async ({ page }, info) => {
    // The colony's inside is the heaviest scene: its full city shader takes several seconds a frame
    // on SwiftShader, and the screenshot waits for one more.
    const heavy = scene === "city" || scene === "markab";
    test.setTimeout(heavy ? 480_000 : 240_000);
    const logs = collectConsole(page);
    await page.goto(
      `/?showcase=${scene}&cam=${cam}&t=${t}&hold=${frames}&quality=${quality}&gfx=${info.project.name}${extra ? `&${extra}` : ""}`,
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
        timeout: heavy ? 420_000 : 200_000,
        polling: 500,
      })
      .then(() => "");
    const error = await Promise.race([ready, broken]);
    expect(error.slice(0, 800)).toBe("");
    const status = await bc(page);
    console.log(`${scene}/${cam}: ${JSON.stringify(status)}`);
    expect(status.mode).toBe("showcase");
    expect(status.gfx_tier).toBe(quality);
    // The city's life each camera draws (logged: from up high, the first and second, there's
    // little or none till the far lights land). Downtown at eye height the people and traffic are
    // always about (bc-client-core's `the_third_camera_sees_life` counts them at these hours),
    // unless `life=0` hides them.
    if (scene === "city") {
      console.log(`life: ${status.ambient_people} people, ${status.ambient_cars} cars, ${status.life_ms} ms`);
    }
    if (scene === "markab") {
      expect(status.markab_act).toBe(markabActs[String(t)]);
    }
    if (scene === "city" && cam === 3 && !/(^|&)life=0/.test(extra)) {
      expect(status.ambient_people).toBeGreaterThan(0);
      expect(status.ambient_cars).toBeGreaterThan(0);
    }
    const name = `gfx-${info.project.name}-${quality}-${scene}-${cam}-t${t}${suffix}.png`;
    const shot = await page.screenshot({ path: `artifacts/${name}` });
    expect(luminanceStdDev(shot)).toBeGreaterThan(3);
    const bad = logs.filter((l) => /\[error\]|\[pageerror\]|%cERROR|panicked|wgpu error|validation error/i.test(l));
    if (bad.length) console.log(bad.join("\n"));
    expect(bad).toEqual([]);
  });
}
