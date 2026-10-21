// Browser side of `bun scripts/measure-map.ts`: LiveMap on a synthetic 10,000-entity source (not shipped).
import "@umbra/ui-kit/styles.css";
import "@umbra/map-view/styles.css";
import type { EntityFrame, EntitySource, EntitySummary } from "@umbra/engine-stream";
import { LiveMap } from "@umbra/map-view";
import { ThemeProvider } from "@umbra/ui-kit";
import { createElement } from "react";
import { createRoot } from "react-dom/client";

const COUNT = 10_000;
const FORCES = ["blue", "red", "green", null] as const;

const listeners = new Set<(frame: EntityFrame) => void>();
let current: EntityFrame | undefined;
const source: EntitySource = {
  getFrame: () => current,
  subscribe(listener) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
};

function frame(seq: number): EntityFrame {
  const entities: EntitySummary[] = [];
  for (let i = 0; i < COUNT; i++) {
    entities.push({
      id: i + 1,
      alt_m: 0,
      heading_deg: (i * 37 + seq) % 360,
      lat_deg: 50 + (i % 100) * 0.01 + seq * 0.00001,
      lon_deg: 5 + Math.floor(i / 100) * 0.01 + seq * 0.00001,
      ownership: i % 7 === 0 ? "remote" : "owned",
      speed_mps: i % 3 === 0 ? 0 : 5,
      force: FORCES[i % FORCES.length] ?? null,
    });
  }
  return { session: 1, tick: seq, time_s: seq, seq, dropped: 0, entities };
}

const push = (next: EntityFrame): void => {
  current = next;
  for (const l of [...listeners]) l(next);
};

/** Gaps between animation frames while waiting: a long gap is main-thread work (flush + render). */
const frameGaps: number[] = [];

const waitFor = async (done: () => boolean, timeoutMs: number): Promise<void> => {
  const start = performance.now();
  let last = start;
  while (!done()) {
    if (performance.now() - start > timeoutMs) throw new Error("timeout waiting for idle");
    await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));
    const now = performance.now();
    frameGaps.push(now - last);
    last = now;
  }
};

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");
createRoot(root).render(createElement(ThemeProvider, null, createElement(LiveMap, { source })));

/** Pushes `frames` frames of 10,000 entities and times each push until the map is idle. */
async function run(
  frames: number,
): Promise<{ idleMs: number[]; flushMs: number[]; generateMs: number[]; gapMs: number[] }> {
  const hook = Reflect.get(window, "__UMBRA_E2E__");
  await waitFor(() => hook?.map.isIdle() === true, 60_000);
  const idleMs: number[] = [];
  const flushMs: number[] = [];
  const generateMs: number[] = [];
  frameGaps.length = 0;
  for (let seq = 0; seq <= frames; seq++) {
    const g = performance.now();
    const next = frame(seq);
    generateMs.push(performance.now() - g);
    const t = performance.now();
    push(next);
    await waitFor(() => hook.map.isIdle() === true && hook.map.sourceEntities() === COUNT, 60_000);
    if (seq > 0) {
      idleMs.push(performance.now() - t);
      // main thread: toFeatureCollection plus MapLibre's setData hand-off (its own serialisation included)
      flushMs.push(hook.map.lastFlushMs());
    } // frame 0 includes the first fit and tile setup
  }
  return { idleMs, flushMs, generateMs, gapMs: [...frameGaps] };
}

Reflect.set(window, "__measure", run);
