import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { expect, test, type Page } from "@playwright/test";
import { bc, calmPointer } from "./util";

// Players' reports (`scripts/e2e.sh report`: survival as the game runs it, a data dir so the server
// takes them; `crates/bc-server/src/bugs.rs`, `web/ui.js` "Reports"). A pilot wakes in the ring's
// town, presses F7, says what's wrong and presses Enter: a report lands under the data dir with the
// build, where they stand and a picture of the frame, and nothing of who they are. A trap (what a
// panic ends in) files one crash report, however often it happens.

const DIR = process.env.BC_REPORT_DIR ?? "";

/** The reports kept so far: each file's name and, for the JSON, the document. */
function kept(): { name: string; doc?: Record<string, any> }[] {
  if (!DIR || !existsSync(DIR)) return [];
  return readdirSync(DIR)
    .filter((n) => n.startsWith("bc-report-"))
    .map((name) => ({
      name,
      doc: name.endsWith(".json") ? JSON.parse(readFileSync(path.join(DIR, name), "utf8")) : undefined,
    }));
}

async function until<T>(what: string, f: () => Promise<T | undefined> | T | undefined, ms: number): Promise<T> {
  const t0 = Date.now();
  for (;;) {
    const v = await f();
    if (v) return v;
    if (Date.now() - t0 > ms) throw new Error(`timed out waiting for ${what}`);
    await new Promise((r) => setTimeout(r, 250));
  }
}

async function inTheTown(page: Page) {
  await page.addInitScript(calmPointer);
  await page.goto("/?autoplay=1&name=Tester&quality=low");
  return until(
    "the town",
    async () => {
      const s = await bc(page);
      return s.screen === "playing" && s.place === "town" && s.town_feet ? s : undefined;
    },
    240_000,
  );
}

test.beforeAll(() => {
  expect(DIR, "scripts/e2e.sh report names the reports' directory").not.toBe("");
});

test("F7 files a report with the frame, and nothing of who the pilot is", async ({ page }) => {
  // The town drawn in software takes minutes to wake in.
  test.setTimeout(600_000);
  const s = await inTheTown(page);
  // The build stamp: the page's (config.js) and the client's agree.
  const build = await page.evaluate(() => (window as any).bcBuild);
  expect(build.commit).not.toBe("");
  expect(s.build_commit).toBe(build.commit);
  expect(s.build_version).toBe(build.version);

  await page.keyboard.press("F7");
  await expect(page.locator("#report")).toBeVisible();
  await expect(page.locator("#report-input")).toBeFocused();
  // The game stands aside: the pointer goes free, the keys aren't the suit's.
  await until("the pointer let go", async () => ((await bc(page)).click_to_fly === false ? true : undefined), 10_000);
  // The game's picture of the frame, taken as it opened (a few of its frames: seconds in software).
  await expect(page.locator("#report")).toHaveAttribute("data-shot", "taken", { timeout: 60_000 });
  await expect(page.locator("#report-shot")).toContainText("PICTURE");
  await page.keyboard.press("Tab"); // the next kind: THE WORLD
  await page.keyboard.type("the lamp post by the lift floats a hand above the kerb");
  await page.keyboard.press("Enter");
  await expect(page.locator("#report-note")).toContainText("SENT", { timeout: 30_000 });
  await expect(page.locator("#report")).toBeHidden({ timeout: 10_000 });

  const files = await until("the report's files", () => (kept().length >= 2 ? kept() : undefined), 10_000);
  const json = files.find((f) => f.doc)!;
  const doc = json.doc!;
  expect(doc.kind).toBe("world");
  expect(doc.words).toBe("the lamp post by the lift floats a hand above the kerb");
  expect(doc.issue_title).toBe("[world] the lamp post by the lift floats a hand above the kerb");
  expect(doc.read_first).toBe("docs/FREE_ROAM.md");
  expect(doc.build).toMatchObject({ version: build.version, commit: build.commit, platform: "web", runtime: "browser" });
  expect(doc.place.place).toBe("town");
  expect(doc.place.town_feet).toBeTruthy();
  expect(typeof doc.vitals.fps).toBe("number");
  expect(doc.net.link).toBeTruthy();
  // The server's fingerprint is the page's.
  const fp = await page.evaluate(
    ([commit]) => (window as any).bcReports.fingerprint("world", commit, "docs/FREE_ROAM.md", ""),
    [build.commit],
  );
  expect(doc.fingerprint).toBe(fp);
  // The picture of the frame.
  const png = files.find((f) => f.name.endsWith(".png"))!;
  expect(doc.intake.shot).toBe(png.name);
  expect(readFileSync(path.join(DIR, png.name)).subarray(0, 8)).toEqual(Buffer.from("\x89PNG\r\n\x1a\n", "latin1"));
  // Nothing of who the pilot is.
  const text = readFileSync(path.join(DIR, json.name), "utf8");
  for (const word of ["Tester", "resume", "signature", "signed_in", "people_names", "credits"]) {
    expect(text, `the report holds ${word}`).not.toContain(word);
  }
  expect(Object.keys(doc.place)).not.toContain("name");

  // An agent files the same way (`scripts/pt report`): marked as an agent's, with the frame.
  const got = await page.evaluate(() => (window as any).bcReport.file("verb", "a playtest agent's line"));
  expect(got.seen).toBe(1);
  const agent = await until(
    "the agent's report",
    () => kept().find((f) => f.name === `bc-report-${got.id}.json`)?.doc,
    10_000,
  );
  expect(agent.build.runtime).toBe("agent");
  expect(agent.issue_title).toBe("[verb] a playtest agent's line");
  expect(agent.read_first).toBe("docs/CONTROLS.md");
});

test("a trap files one crash report, once", async ({ page }) => {
  test.setTimeout(600_000);
  await inTheTown(page);
  const crashes = () => kept().filter((f) => f.doc?.kind === "crash");
  const before = crashes().length;
  // What a panic ends in, twice: the page files it the first time only.
  for (let i = 0; i < 2; i++) {
    await page.evaluate(() => {
      setTimeout(() => {
        throw new WebAssembly.RuntimeError("unreachable executed");
      }, 0);
    });
    await page.waitForTimeout(500);
  }
  const got = await until("the crash report", () => (crashes().length > before ? crashes() : undefined), 15_000);
  await page.waitForTimeout(2000);
  expect(crashes().length).toBe(before + 1);
  const doc = got[got.length - 1].doc!;
  expect(doc.read_first).toBe("CLAUDE.md");
  expect(doc.crash.what).toBe("panic");
  expect(doc.crash.started).toBe(true);
  expect(doc.issue_title).toBe("[crash] the game stopped");
});
