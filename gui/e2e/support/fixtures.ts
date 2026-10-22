import type { ChildProcess } from "node:child_process";
import { test as base } from "@playwright/test";
import { stopProcesses } from "./children.ts";
import { startEngine, startPreview } from "./processes.ts";

export interface EngineServer {
  /** The engine's own URL, for the generated client. */
  readonly engineUrl: string;
  /** A preview of the e2e build whose `/api/` proxy targets the engine; the page's `baseURL`. */
  readonly guiUrl: string;
  /** The `umbra serve` process, for failure diagnostics (`exitCode`, `signalCode`). */
  readonly engine: ChildProcess;
}

/**
 * Engine tests import `test` from here. `engineServer` starts `umbra serve` and a proxied preview once
 * per worker, only when a test asks for it (ASM-514), and stops both afterwards, also when startup fails.
 */
export const test = base.extend<{ baseURL: string }, { engineServer: EngineServer }>({
  engineServer: [
    // biome-ignore lint/correctness/noEmptyPattern: Playwright requires a destructured first argument
    async ({}, use) => {
      const children: ChildProcess[] = [];
      try {
        const engine = await startEngine(children);
        const guiUrl = await startPreview(children, engine.url);
        await use({ engineUrl: engine.url, guiUrl, engine });
      } finally {
        await stopProcesses(children);
      }
    },
    { scope: "worker" },
  ],
  baseURL: async ({ engineServer }, use) => {
    await use(engineServer.guiUrl);
  },
});

export { expect } from "@playwright/test";
