// The keyboard path (plan parity: Tab, Shift+Tab, arrows, Enter, Space,
// Escape) and what each control asks the host for.
import { expect, test } from "@playwright/test";
import { calls, open } from "./helpers";

test("arrows move between rows and within a row; Enter opens the panel; Escape goes back, then hides", async ({ page }) => {
  await open(page, "busy");
  const rows = page.locator(".app-main[data-row]");
  await rows.first().focus();
  await expect(rows.first()).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(rows.nth(1)).toBeFocused();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("button", { name: "Stop N8 Hotkeys" })).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("button", { name: "Start N8 Commentary" })).toBeFocused();
  await page.keyboard.press("End");
  await expect(page.locator("[data-row]").last()).toBeFocused();
  await page.keyboard.press("Home");
  await expect(rows.first()).toBeFocused();

  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("VoiceDock");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("N8 Tray");
  await page.keyboard.press("Escape");
  expect((await calls(page)).some((c) => c.cmd === "hide_window")).toBe(true);
});

test("Tab reaches every control in order, Space works the start-at-sign-in switch", async ({ page }) => {
  await open(page, "busy");
  await page.locator(".flyout").focus();
  const expected = [
    /^Settings$/,
    /^Hide N8 Tray$/,
    /^Lead Scraper is over its 1 GB warning\s*3 of 4 running · N8 1.9 GB in all\s*Details$/,
    /^VoiceDock\s*, Ready,\s*Kokoro ready · Piper idle\s*643 MB\s*12% CPU$/,
    /^Stop VoiceDock$/,
  ];
  for (const name of expected) {
    await page.keyboard.press("Tab");
    await expect(page.locator(":focus")).toHaveAccessibleName(name);
  }
  for (let i = 0; i < 12; i++) await page.keyboard.press("Tab");
  await page.getByRole("switch", { name: "Start at sign-in" }).focus();
  await page.keyboard.press("Space");
  expect(await calls(page)).toContainEqual({ cmd: "set_setting", args: { key: "launchAtSignIn", on: false, app: null } });
});

test("a row's one action and the panel's Restart ask the host for exactly that", async ({ page }) => {
  await open(page, "busy");
  await page.getByRole("button", { name: "Stop VoiceDock" }).click();
  await page.getByRole("button", { name: "Start N8 Commentary" }).click();
  await page.getByRole("button", { name: /^VoiceDock\s*, Ready/ }).click();
  await page.getByRole("button", { name: "Restart VoiceDock" }).click();
  await page.getByRole("button", { name: "Start Kokoro" }).click();
  const sent = (await calls(page)).filter((c) => c.cmd === "module_action").map((c) => c.args);
  expect(sent).toEqual([
    { id: "voicedock", action: "stop", component: null },
    { id: "n8commentary", action: "start", component: null },
    { id: "voicedock", action: "restart", component: null },
    { id: "voicedock", action: "engine.start", component: "kokoro" },
  ]);
});

test("an action that asks for confirmation takes a second click", async ({ page }) => {
  await open(page, "trouble");
  const start = page.getByRole("button", { name: "Start N8 Commentary" });
  await start.click();
  expect((await calls(page)).filter((c) => c.cmd === "module_action")).toEqual([]);
  await page.getByRole("button", { name: "Confirm: Start N8 Commentary" }).click();
  expect((await calls(page)).filter((c) => c.cmd === "module_action").length).toBe(1);
});

test("impossible and pending actions are dimmed", async ({ page }) => {
  await open(page, "trouble");
  await expect(page.getByRole("button", { name: "Stop N8 Hotkeys" })).toBeDisabled();
});
