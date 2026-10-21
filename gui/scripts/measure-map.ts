// Not a gate (plan WI-12): times `setData`-to-idle for 10,000 entities in headless Chromium.
// Usage: bun scripts/measure-map.ts [frames]. Needs WebGL (SwiftShader flags as in playwright.config.ts).
import { resolve } from "node:path";
import { chromium } from "@playwright/test";
import react from "@vitejs/plugin-react";
import { createServer } from "vite";

const frames = Number(process.argv[2] ?? 20);
const gui = resolve(import.meta.dirname, "..");
const server = await createServer({
  configFile: false,
  root: resolve(import.meta.dirname, "measure-map"),
  logLevel: "error",
  plugins: [react()],
  define: { "import.meta.env.VITE_UMBRA_E2E": JSON.stringify("1") },
  server: { port: 0, host: "127.0.0.1", fs: { allow: [gui, resolve(gui, "..")] } },
});
await server.listen();
const address = server.httpServer?.address();
const url =
  typeof address === "object" && address ? `http://127.0.0.1:${address.port}/` : undefined;
if (!url) throw new Error("no dev server address");

const browser = await chromium.launch({
  args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
});
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  page.on("pageerror", (e) => console.error("pageerror:", e.message));
  await page.goto(url);
  await page.waitForFunction(() => Reflect.has(window, "__measure"), undefined, {
    timeout: 60_000,
  });
  const result = await page.evaluate(async (n) => {
    const run = Reflect.get(window, "__measure");
    return run(n);
  }, frames);
  const stats = (xs: number[]) => {
    const s = [...xs].sort((a, b) => a - b);
    const at = (p: number) => s[Math.min(s.length - 1, Math.floor(p * s.length))] ?? Number.NaN;
    return { n: s.length, median: at(0.5), p95: at(0.95), max: s.at(-1) ?? Number.NaN };
  };
  console.log("setData-to-idle (ms), 10,000 entities:", stats(result.idleMs));
  console.log(
    "flush on the main thread (ms; feature build + MapLibre setData hand-off):",
    stats(result.flushMs),
  );
  console.log(
    "animation-frame gaps while waiting (ms; main-thread blocking):",
    stats(result.gapMs),
  );
  console.log("frame generation in the page (ms, not part of the app):", stats(result.generateMs));
} finally {
  await browser.close();
  await server.close();
}
