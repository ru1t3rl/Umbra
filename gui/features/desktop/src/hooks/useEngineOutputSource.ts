import type { JournalSource } from "@umbra/engine-stream";

/**
 * The engine process output as a journal source, or `undefined` when there is none (the web build, and
 * the desktop app until WI-22 fills this in; ASM-944 f).
 */
export function useEngineOutputSource(): JournalSource | undefined {
  return undefined;
}
