import { useStore } from "zustand";
import { useConnection } from "./useConnection.ts";

/**
 * Only the run state word (`idle`, `running`, ...), or null before the first run status. Unlike `useRunStatus`
 * it re-renders when the word changes, not on every tick, so a component that gates on the state stays quiet.
 */
export function useRunState(): string | null {
  return useStore(useConnection().store, (s) => s.run?.state ?? null);
}
