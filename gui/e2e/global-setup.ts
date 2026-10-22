import { spawnSync } from "node:child_process";
import { childRegistry, stopChildren } from "./support/children.ts";
import { GUI_ROOT, startPreview, VITE } from "./support/processes.ts";

/**
 * Builds the e2e bundle once and serves it with an engine-free `vite preview` for project `ui`
 * (`UMBRA_GUI_URL`). The engine and its preview belong to the worker-scoped fixture in
 * support/fixtures.ts (ASM-514), so `--project ui` needs no binary. Playwright does not run
 * global-teardown when global-setup throws, so a failure stops what was started.
 */
export default async function globalSetup(): Promise<void> {
  try {
    const build = spawnSync(process.execPath, [VITE, "build", "--mode", "e2e"], {
      cwd: GUI_ROOT,
      stdio: "inherit",
    });
    if (build.status !== 0) throw new Error("vite build --mode e2e failed");
    process.env.UMBRA_GUI_URL = await startPreview(childRegistry());
  } catch (error) {
    await stopChildren();
    throw error;
  }
}
