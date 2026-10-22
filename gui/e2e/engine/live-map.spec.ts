import type { ChildProcess } from "node:child_process";
import type { Page } from "@playwright/test";
import type { EntitySummary } from "@umbra/data";
import { connectEngine, engineEntities, loadSteppedSkeleton, step } from "../support/engine.ts";
import { expect, test } from "../support/fixtures.ts";
import { haversineMeters, type LonLat } from "../support/geo.ts";

/**
 * M5b acceptance 1: a real headless `umbra serve` (global-setup) with walking_skeleton on a stepped clock.
 * Tolerances are ASM-310; the e2e-only hook window.__UMBRA_E2E__.map is ASM-309.
 */
const SOURCE_TOLERANCE_DEG = 1e-9;
const RENDERED_TOLERANCE_M = 2.0;
const MIN_MOVED_M = 50;
const MIN_MOVED_PX = 2;
const IDLE_TIMEOUT_MS = 10_000;
const MIN_ZOOM = 12;
const ENTITY_COUNT = 3;

interface Position extends LonLat {
  id: number;
}
interface Pixel {
  x: number;
  y: number;
}
interface Observed {
  zoom: number;
  session: number;
  tick: number;
  source: Position[];
  rendered: Position[];
  pixels: Record<number, Pixel | null>;
}

/** Waits for the hook, the flushed frame of `session`/`tick`, the expected entity count and an idle map. */
async function observeAt(
  page: Page,
  engine: ChildProcess,
  session: number,
  tick: number,
  count: number,
): Promise<Observed> {
  const expected = { session, tick, count };
  try {
    await page.waitForFunction(
      ([s, t, n]) => {
        const map = Reflect.get(window, "__UMBRA_E2E__")?.map;
        const frame = map?.frameTick();
        return (
          frame?.session === s &&
          frame.tick === t &&
          map.sourceEntities() === n &&
          map.renderedEntities() === n &&
          map.isIdle()
        );
      },
      [session, tick, count] as const,
      { timeout: IDLE_TIMEOUT_MS },
    );
  } catch (error) {
    const last = await page
      .evaluate(() => {
        const map = Reflect.get(window, "__UMBRA_E2E__")?.map;
        if (!map) return { hook: false };
        return {
          hook: true,
          frameTick: map.frameTick(),
          sourceEntities: map.sourceEntities(),
          renderedEntities: map.renderedEntities(),
          isIdle: map.isIdle(),
        };
      })
      .catch((e: unknown) => ({ unreadable: String(e) }));
    const state = {
      expected,
      last,
      engine: { exitCode: engine.exitCode, signal: engine.signalCode },
    };
    throw new Error(`the map did not reach the expected state: ${JSON.stringify(state)}`, {
      cause: error,
    });
  }
  const observed = await page.evaluate(() => {
    const map = Reflect.get(window, "__UMBRA_E2E__").map;
    const source: Position[] = map.sourcePositions();
    const rendered: Position[] = map.renderedPositions();
    const pixels: Record<number, Pixel | null> = {};
    for (const p of rendered) pixels[p.id] = map.project(p.lon, p.lat);
    return { ...map.frameTick(), zoom: map.zoom(), source, rendered, pixels } as Observed;
  });
  expect(observed.zoom, "the zoom behind the rendered tolerance (ASM-310)").toBeGreaterThanOrEqual(
    MIN_ZOOM,
  );
  return observed;
}

const byId = <T extends { id: number }>(items: readonly T[]): Map<number, T> =>
  new Map(items.map((item) => [item.id, item]));

const lonLat = (e: EntitySummary): LonLat => ({ lon: e.lon_deg, lat: e.lat_deg });

/** Largest source error (degrees) and rendered error (metres) against the engine list. */
function compare(observed: Observed, engine: readonly EntitySummary[]) {
  expect(observed.source.map((p) => p.id).sort()).toEqual(engine.map((e) => e.id).sort());
  const source = byId(observed.source);
  const rendered = byId(observed.rendered);
  let maxSourceDeg = 0;
  let maxRenderedM = 0;
  for (const e of engine) {
    const s = source.get(e.id);
    const r = rendered.get(e.id);
    if (!s || !r) throw new Error(`entity ${e.id} missing from the map`);
    maxSourceDeg = Math.max(maxSourceDeg, Math.abs(s.lon - e.lon_deg), Math.abs(s.lat - e.lat_deg));
    maxRenderedM = Math.max(maxRenderedM, haversineMeters(r, lonLat(e)));
  }
  expect(maxSourceDeg).toBeLessThanOrEqual(SOURCE_TOLERANCE_DEG);
  expect(maxRenderedM).toBeLessThanOrEqual(RENDERED_TOLERANCE_M);
  // Visible in the list reporter: the measured errors behind the tolerances (ASM-310).
  console.log(
    `max source error ${maxSourceDeg} deg, max rendered error ${maxRenderedM.toFixed(3)} m at zoom ${observed.zoom.toFixed(2)}`,
  );
}

// Longer than the 10 s waits, so a failed wait reports its own message with the last observed state.
test.setTimeout(60_000);

test.beforeEach(({ engineServer }) => {
  connectEngine(engineServer.engineUrl);
});

test("all walking_skeleton entities appear on the map", async ({ page, engineServer }) => {
  const { session } = await loadSteppedSkeleton();
  await page.goto("/");
  const observed = await observeAt(page, engineServer.engine, session, 0, ENTITY_COUNT);
  const engine = await engineEntities();
  expect(engine.tick).toBe(0);
  expect(engine.items).toHaveLength(ENTITY_COUNT);
  compare(observed, engine.items);
});

test("after step(600) the entity layer equals the engine list for that tick and every marker moved", async ({
  page,
  engineServer,
}) => {
  const { session } = await loadSteppedSkeleton();
  await page.goto("/");
  const before = await observeAt(page, engineServer.engine, session, 0, ENTITY_COUNT);
  const engineBefore = await engineEntities();
  compare(before, engineBefore.items);

  expect(await step(600)).toBe(600);
  const after = await observeAt(page, engineServer.engine, session, 600, ENTITY_COUNT);
  const engineAfter = await engineEntities();
  expect(engineAfter.tick).toBe(600);
  compare(after, engineAfter.items);

  const engineThen = byId(engineBefore.items);
  const renderedThen = byId(before.rendered);
  const renderedNow = byId(after.rendered);
  let minMovedM = Number.POSITIVE_INFINITY;
  let minMovedPx = Number.POSITIVE_INFINITY;
  for (const e of engineAfter.items) {
    const then = engineThen.get(e.id);
    const rThen = renderedThen.get(e.id);
    const rNow = renderedNow.get(e.id);
    const pixelThen = before.pixels[e.id];
    const pixelNow = after.pixels[e.id];
    if (!then || !rThen || !rNow || !pixelThen || !pixelNow) {
      throw new Error(`entity ${e.id} has no position before or after`);
    }
    expect(
      haversineMeters(lonLat(then), lonLat(e)),
      `engine move of ${e.id}`,
    ).toBeGreaterThanOrEqual(MIN_MOVED_M);
    expect(haversineMeters(rThen, rNow), `rendered move of ${e.id}`).toBeGreaterThanOrEqual(
      MIN_MOVED_M,
    );
    const px = Math.hypot(pixelNow.x - pixelThen.x, pixelNow.y - pixelThen.y);
    expect(px, `pixel move of ${e.id}`).toBeGreaterThanOrEqual(MIN_MOVED_PX);
    minMovedM = Math.min(minMovedM, haversineMeters(rThen, rNow));
    minMovedPx = Math.min(minMovedPx, px);
  }
  console.log(
    `min rendered move ${minMovedM.toFixed(1)} m, min pixel move ${minMovedPx.toFixed(1)} px`,
  );
});

test("a reload of the scenario replaces the entities", async ({ page, engineServer }) => {
  const first = await loadSteppedSkeleton();
  await page.goto("/");
  await observeAt(page, engineServer.engine, first.session, 0, ENTITY_COUNT);
  await step(600);
  await observeAt(page, engineServer.engine, first.session, 600, ENTITY_COUNT);

  const second = await loadSteppedSkeleton();
  expect(second.session).toBeGreaterThan(first.session);
  const observed = await observeAt(page, engineServer.engine, second.session, 0, ENTITY_COUNT);
  const engine = await engineEntities();
  expect(engine.tick).toBe(0);
  expect(engine.items).toHaveLength(ENTITY_COUNT);
  compare(observed, engine.items);
});
