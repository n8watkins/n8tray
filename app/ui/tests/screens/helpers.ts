import { expect, type Page } from "@playwright/test";

/**
 * Opens the window on a recorded snapshot and sizes the page as Rust sizes
 * the window: 380 px wide, as tall as the content reported (fit_window).
 */
export async function open(page: Page, fixture: string, view = "hub") {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await page.goto(`/?fixture=${fixture}&view=${encodeURIComponent(view)}`);
  await page.waitForFunction(() => window.__preview?.height);
  await page.waitForLoadState("networkidle");
  // Settle: icons load and the height is reported again.
  await page.waitForTimeout(150);
  const height = await page.evaluate(() => window.__preview.height!);
  expect(height).toBeGreaterThan(100);
  await page.setViewportSize({ width: 380, height: Math.min(Math.ceil(height), 1040) });
  await page.waitForTimeout(80);
  return { height, errors };
}

/** Nothing sticks out sideways. */
export async function expectNoSidewaysOverflow(page: Page) {
  const wide = await page.evaluate(() => {
    const limit = document.documentElement.clientWidth;
    return Array.from(document.querySelectorAll<HTMLElement>("body *"))
      .filter((e) => e.getBoundingClientRect().right > limit + 0.5 && e.getClientRects().length > 0)
      .map((e) => `${e.tagName.toLowerCase()}.${e.className}`)
      .slice(0, 5);
  });
  expect(wide).toEqual([]);
}

export async function calls(page: Page) {
  return page.evaluate(() => window.__preview.calls);
}
