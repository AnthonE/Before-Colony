import { expect, test } from "@playwright/test";
import { collectConsole } from "./util";

// The page's gallery (`?gallery`, web/gallery.js): every screen and panel the page draws, with
// bc_client_core::gallery's made-up data and no game. Each scene's element is up, nothing on the
// page throws, and each is photographed into artifacts/gallery/ to look over after a change to the
// page's look (two viewports: the suites' 960×540 and a laptop's 1440×900).
for (const size of [
  { width: 960, height: 540 },
  { width: 1440, height: 900 },
]) {
  test(`every screen of the page, ${size.width}×${size.height}`, async ({ page }, info) => {
    test.setTimeout(180_000);
    const logs = collectConsole(page);
    await page.setViewportSize(size);
    await page.goto("/?gallery&bare");
    await page.waitForFunction(() => !!(window as any).bcGallery, null, { timeout: 30_000 });
    const scenes: { name: string; expect: string }[] = await page.evaluate(() => (window as any).bcGallery.scenes);
    expect(scenes.length).toBeGreaterThan(20);
    for (const s of scenes) {
      await page.evaluate((name) => (window as any).bcGallery.show(name), s.name);
      await expect(page.locator(s.expect), `${s.name}: ${s.expect}`).toBeVisible();
      await page.screenshot({ path: `artifacts/gallery/${info.project.name}-${size.width}/${s.name}.png` });
    }
    const errors = logs.filter((l) => l.startsWith("[error]") || l.startsWith("[pageerror]"));
    expect(errors, errors.join("\n")).toEqual([]);
  });
}
