import { resolve } from "node:path";
import { defineConfig, devices } from "@playwright/test";

// ADR 0044, ASM-514. `ui` runs against a preview with no engine behind it (URL from global-setup); `engine`
// (e2e/engine/**) gets a live `umbra serve` and a proxied preview from a worker-scoped fixture.
export default defineConfig({
  testDir: "e2e",
  globalSetup: "./e2e/global-setup.ts",
  globalTeardown: "./e2e/global-teardown.ts",
  snapshotPathTemplate: "{testDir}/__screenshots__/{platform}/{arg}{ext}",
  workers: 1,
  retries: 0,
  reporter: "list",
  use: {
    reducedMotion: "reduce",
    // Linux only: pins fontconfig rendering so the native <select> text rasterizes the same on every
    // machine (ASM-932). Chromium ignores the variable on other platforms.
    launchOptions: {
      args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--disable-lcd-text"],
      env: {
        ...process.env,
        FONTCONFIG_FILE: resolve(import.meta.dirname, "e2e/support/fonts.conf"),
      },
    },
  },
  // global-setup exports the URLs (free ports); workers re-read this config and inherit them
  projects: [
    {
      name: "ui",
      testIgnore: "engine/**",
      use: {
        ...devices["Desktop Chrome"],
        viewport: { width: 1280, height: 800 },
        baseURL: process.env.UMBRA_GUI_URL,
      },
    },
    {
      name: "engine",
      testMatch: "engine/**/*.spec.ts",
      use: {
        ...devices["Desktop Chrome"],
        viewport: { width: 1280, height: 800 },
        // baseURL comes from the engineServer fixture (e2e/support/fixtures.ts)
      },
    },
  ],
});
