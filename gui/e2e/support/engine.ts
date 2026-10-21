import {
  configureEngineUrl,
  type EntitySummary,
  listEntities,
  loadScenario,
  startRun,
  stepRun,
} from "@umbra/data";

/** The scenario every engine test loads, relative to the server's `--scenario-root` (ADR 0044). */
export const WALKING_SKELETON = "scenarios/walking_skeleton/scenario.toml";

/** Points the generated client at the engine of the `engineServer` fixture (plain functions are allowed in e2e). */
export function connectEngine(engineUrl: string): void {
  configureEngineUrl(engineUrl);
}

/** Loads walking_skeleton on a stepped clock with seed 42 and starts it: tick 0, accepting `step`. */
export async function loadSteppedSkeleton(): Promise<{ session: number }> {
  await loadScenario({
    path: WALKING_SKELETON,
    clock: { mode: "stepped" },
    seed: "42",
  });
  const run = await startRun();
  return { session: run.session };
}

/** Steps `count` ticks and returns the tick reached (asserts completion). */
export async function step(count: number): Promise<number> {
  const result = await stepRun({ count });
  if (!result.completed) throw new Error(`step(${count}) did not complete`);
  return result.reached;
}

/** The engine's whole entity list and the tick it was read at. */
export async function engineEntities(): Promise<{ tick: number; items: EntitySummary[] }> {
  const page = await listEntities({ limit: 1000 });
  return { tick: page.tick, items: page.items };
}
