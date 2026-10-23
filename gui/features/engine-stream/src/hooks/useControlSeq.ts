import { useStore } from "zustand";
import { useConnection } from "./useConnection.ts";

/** The number of control frames (frames sent after a command) so far; re-renders only when it changes (ASM-938). */
export function useControlSeq(): number {
  return useStore(useConnection().store, (s) => s.controlSeq);
}
