import { useStore } from "zustand";
import type { StreamStatus } from "../types.ts";
import { useConnection } from "./useConnection.ts";

/** Connection status; re-renders only when it changes. */
export function useStreamStatus(): StreamStatus {
  return useStore(useConnection().store, (s) => s.status);
}
