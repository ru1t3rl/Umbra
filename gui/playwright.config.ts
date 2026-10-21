import { defineConfig, devices } from "@playwright/test";

// ADR 0044. `ui` needs only the preview server; `engine` (WI-16) adds a live `umbra serve`.
export default defineConfig({
  testDir: "e2e",
  globalSetup: "./e2e/global-setup.ts",
  globalTeardown: "./e2e/global-teardown.ts",
  workers: 1,
  retries: 0,
  reporter: "list",
  use: {
    viewport: { width: 1280, height: 800 },
    reducedMotion: "reduce",
    // global-setup exports UMBRA_GUI_URL (free port); workers re-read this config and inherit it
    baseURL: process.env.UMBRA_GUI_URL,
    launchOptions: { args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] },
  },
  projects: [
    { name: "ui", use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } } },
  ],
});
