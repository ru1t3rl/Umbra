import { useRunStatus } from "@umbra/engine-stream";

/**
 * True while the engine reports `idle` (no scenario). An unknown run state (no frame yet) is not idle, so a slow
 * first frame never hides a loaded scenario (ASM-958).
 */
export function useScenarioIdle(): boolean {
  return useRunStatus()?.state === "idle";
}
