// Every view of the window, light and dark, and the hub in High Contrast:
// pictures in screens/ for review at each pull request (the replacement for
// the PowerShell tray's -Render), each checked for script errors, sideways
// overflow and accessibility.
import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";
import { expectNoSidewaysOverflow, open } from "./helpers";

const views: [string, string][] = [
  ["busy", "hub"],
  ["busy", "panel:voicedock"],
  ["busy", "panel:leadscraper"],
  ["busy", "details"],
  ["busy", "settings"],
  ["busy", "history"],
  ["trouble", "hub"],
  ["trouble", "panel:voicedock"],
  ["healthy", "hub"],
  ["empty", "hub"],
  ["trouble", "settings"],
  ["updating", "hub"],
  ["updateFailed", "hub"],
  ["watching", "hub"],
];

for (const scheme of ["light", "dark"] as const) {
  for (const [fixture, view] of views) {
    test(`${fixture} ${view} ${scheme}`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: scheme });
      const { errors } = await open(page, fixture, view);
      await expectNoSidewaysOverflow(page);
      await page.screenshot({ path: `screens/${fixture}-${view.replace(":", "-")}-${scheme}.png` });
      const scan = await new AxeBuilder({ page }).analyze();
      expect(scan.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
      expect(errors).toEqual([]);
    });
  }
}

for (const [fixture, view] of [["busy", "hub"], ["trouble", "hub"], ["busy", "panel:voicedock"], ["busy", "settings"]]) {
  test(`${fixture} ${view} high contrast`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: "dark", forcedColors: "active" });
    const { errors } = await open(page, fixture, view);
    await expectNoSidewaysOverflow(page);
    await page.screenshot({ path: `screens/${fixture}-${view.replace(":", "-")}-contrast.png` });
    expect(errors).toEqual([]);
  });
}
