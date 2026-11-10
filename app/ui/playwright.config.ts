import { defineConfig, devices } from "@playwright/test";

// The window against recorded snapshots (preview/), in Chromium: the
// pictures for review (screens/), the keyboard path and an accessibility
// scan. The real window in Tauri is tested by the WebDriver tests.
export default defineConfig({
  testDir: "tests/screens",
  outputDir: "test-results",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? [["list"], ["github"]] : "list",
  use: { baseURL: "http://localhost:1430", deviceScaleFactor: 2, viewport: { width: 380, height: 700 } },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 380, height: 700 }, deviceScaleFactor: 2 } }],
  webServer: {
    command: "npx vite --config vite.preview.config.ts",
    url: "http://localhost:1430",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
