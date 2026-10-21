import { type ChildProcess, spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";

export const GUI_ROOT = resolve(import.meta.dirname, "../..");
export const REPO_ROOT = resolve(GUI_ROOT, "..");
export const VITE = resolve(GUI_ROOT, "node_modules/vite/bin/vite.js");
export const OUT_DIR = "dist-e2e";
const READY_TIMEOUT_MS = 30_000;
const BUILD_HINT = "build it with `cargo build -p umbra-cli --release --locked`";
/** Port 1 never listens: an engine-free preview must not reach a developer's engine on 7878. */
const CLOSED_ENGINE_URL = "http://127.0.0.1:1";

/** Resolves with group 1 of the first match of `pattern` on stdout; rejects on timeout, spawn error or early exit. */
function waitForLine(child: ChildProcess, pattern: RegExp, what: string): Promise<string> {
  return new Promise((resolveUrl, reject) => {
    let buffer = "";
    let matched = false;
    const timer = setTimeout(
      () => reject(new Error(`${what} printed no URL within ${READY_TIMEOUT_MS / 1000} s`)),
      READY_TIMEOUT_MS,
    );
    // Keeps draining after the match so a full pipe never blocks the child.
    child.stdout?.on("data", (chunk: Buffer) => {
      if (matched) return;
      buffer += chunk.toString();
      const match = pattern.exec(buffer);
      if (match?.[1]) {
        matched = true;
        clearTimeout(timer);
        resolveUrl(match[1]);
      }
    });
    child.once("error", (error) => {
      clearTimeout(timer);
      reject(new Error(`${what} could not start: ${error.message}`));
    });
    child.once("exit", (code, signal) => {
      clearTimeout(timer);
      reject(new Error(`${what} exited before it was ready (code ${code}, signal ${signal})`));
    });
  });
}

/** ASM-311: the release binary is a prerequisite and is never built here. */
function engineBinary(): string {
  const bin = process.env.UMBRA_BIN ?? resolve(REPO_ROOT, "target/release/umbra");
  if (!existsSync(bin)) {
    throw new Error(`umbra binary not found at ${bin}: ${BUILD_HINT} (or set UMBRA_BIN)`);
  }
  return bin;
}

/** Spawns `umbra serve` on a free loopback port; `children` receives the process. Returns its URL. */
export async function startEngine(
  children: ChildProcess[],
): Promise<ChildProcess & { url: string }> {
  const engine = spawn(
    engineBinary(),
    ["serve", "--bind", "127.0.0.1:0", "--scenario-root", "examples"],
    { cwd: REPO_ROOT, stdio: ["ignore", "pipe", "inherit"], detached: false },
  );
  children.push(engine);
  const url = await waitForLine(engine, /listening on (http:\/\/127\.0\.0\.1:\d+)/, "umbra serve");
  return Object.assign(engine, { url });
}

/**
 * `vite preview` of the e2e build on a free loopback port, its `/api/` proxy pointed at `engineUrl`
 * (a closed port when omitted: the page then sees "Engine not reachable", ASM-511, and the refused
 * proxy requests, logged on stderr by design, are dropped).
 */
export async function startPreview(children: ChildProcess[], engineUrl?: string): Promise<string> {
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    NO_COLOR: "1",
    FORCE_COLOR: "0",
    UMBRA_ENGINE_URL: engineUrl ?? CLOSED_ENGINE_URL,
  };
  const preview = spawn(
    process.execPath,
    [VITE, "preview", "--outDir", OUT_DIR, "--host", "127.0.0.1", "--port", "0"],
    { cwd: GUI_ROOT, stdio: ["ignore", "pipe", engineUrl ? "inherit" : "ignore"], env },
  );
  children.push(preview);
  return waitForLine(preview, /Local:\s+(http:\/\/127\.0\.0\.1:\d+)/, "vite preview");
}
