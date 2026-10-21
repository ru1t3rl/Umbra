import { type ChildProcess, spawn, spawnSync } from "node:child_process";
import { resolve } from "node:path";

const GUI_ROOT = resolve(import.meta.dirname, "..");
const VITE = resolve(GUI_ROOT, "node_modules/vite/bin/vite.js");
const OUT_DIR = "dist-e2e";
const READY_TIMEOUT_MS = 30_000;

const REGISTRY = Symbol.for("umbra.e2e.children");

/** Children started here, stopped by global-teardown.ts (kept on globalThis, not module state). */
export function childRegistry(): ChildProcess[] {
  const holder = globalThis as unknown as Record<symbol, ChildProcess[] | undefined>;
  const existing = holder[REGISTRY];
  if (existing) return existing;
  const created: ChildProcess[] = [];
  holder[REGISTRY] = created;
  return created;
}

/** Resolves with the URL from vite's "Local:" line; rejects at once if the child exits first. */
function previewUrl(child: ChildProcess): Promise<string> {
  return new Promise((resolveUrl, reject) => {
    let buffer = "";
    const timer = setTimeout(
      () => reject(new Error(`vite preview printed no URL within ${READY_TIMEOUT_MS / 1000} s`)),
      READY_TIMEOUT_MS,
    );
    child.stdout?.on("data", (chunk: Buffer) => {
      process.stdout.write(chunk);
      buffer += chunk.toString();
      const match = /Local:\s+(http:\/\/127\.0\.0\.1:\d+)/.exec(buffer);
      if (match?.[1]) {
        clearTimeout(timer);
        resolveUrl(match[1]);
      }
    });
    child.once("exit", (code, signal) => {
      clearTimeout(timer);
      reject(new Error(`vite preview exited before it was ready (code ${code}, signal ${signal})`));
    });
  });
}

/** Builds the e2e bundle and serves it with `vite preview`. The engine part arrives with WI-16. */
export default async function globalSetup(): Promise<void> {
  const build = spawnSync(process.execPath, [VITE, "build", "--mode", "e2e"], {
    cwd: GUI_ROOT,
    stdio: "inherit",
  });
  if (build.status !== 0) throw new Error("vite build --mode e2e failed");

  const preview = spawn(
    process.execPath,
    [VITE, "preview", "--outDir", OUT_DIR, "--host", "127.0.0.1", "--port", "0"],
    {
      cwd: GUI_ROOT,
      stdio: ["ignore", "pipe", "inherit"],
      env: { ...process.env, NO_COLOR: "1", FORCE_COLOR: "0" },
    },
  );
  childRegistry().push(preview);
  process.env.UMBRA_GUI_URL = await previewUrl(preview);
}
