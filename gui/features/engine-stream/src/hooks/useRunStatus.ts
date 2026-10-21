import { useStore } from "zustand";
import type { RunStatus } from "../types.ts";
import { useConnection } from "./useConnection.ts";

/** The latest run status; re-renders only when its content changes. */
export function useRunStatus(): RunStatus | undefined {
  return useStore(useConnection().store, (s) => s.run);
}
